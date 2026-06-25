use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute, queue,
    style::{Attribute, Color, Print, ResetColor, SetAttribute, SetForegroundColor},
    terminal::{
        self, disable_raw_mode, enable_raw_mode, Clear, ClearType, EnterAlternateScreen,
        LeaveAlternateScreen,
    },
};
use std::{
    collections::HashMap,
    fs,
    io::{self, BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process,
};

const COMMANDS_FILE: &str = "commands";
const COUNTS_FILE: &str = "counts";
const CONFIG_DIR: &str = ".config/gg";

fn config_dir() -> PathBuf {
    let home = std::env::var("HOME").expect("HOME environment variable must be set");
    PathBuf::from(home).join(CONFIG_DIR)
}

fn commands_path() -> PathBuf {
    config_dir().join(COMMANDS_FILE)
}

fn counts_path() -> PathBuf {
    config_dir().join(COUNTS_FILE)
}

/// Absolute, canonicalized current working directory (falls back to the raw
/// path if canonicalization fails, e.g. because a component was removed).
fn cwd() -> String {
    let dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let canon = fs::canonicalize(&dir).unwrap_or(dir);
    canon.to_string_lossy().into_owned()
}

/// A single stored command. An empty `name` denotes the directory default.
#[derive(Clone)]
struct Cmd {
    path: String,
    name: String,
    command: String,
}

impl Cmd {
    /// Composite key (path + name) used for usage counts.
    fn count_key(&self) -> String {
        format!("{}\t{}", self.path, self.name)
    }

    /// Human-facing label for the slot.
    fn label(&self) -> &str {
        if self.name.is_empty() {
            "(default)"
        } else {
            &self.name
        }
    }
}

fn load_commands() -> Vec<Cmd> {
    let path = commands_path();
    match fs::File::open(&path) {
        Ok(f) => BufReader::new(f)
            .lines()
            .map_while(Result::ok)
            .filter_map(|l| {
                if l.trim().is_empty() {
                    return None;
                }
                // path \t name \t command   (command may itself contain tabs)
                let mut parts = l.splitn(3, '\t');
                let path = parts.next()?.to_string();
                let name = parts.next()?.to_string();
                let command = parts.next()?.to_string();
                Some(Cmd {
                    path,
                    name,
                    command,
                })
            })
            .collect(),
        Err(_) => vec![],
    }
}

fn save_commands(cmds: &[Cmd]) -> io::Result<()> {
    let dir = config_dir();
    if !dir.exists() {
        fs::create_dir_all(&dir)?;
    }
    let content: String = cmds
        .iter()
        .map(|c| format!("{}\t{}\t{}", c.path, c.name, c.command))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    fs::write(commands_path(), content)
}

/// Commands configured for `dir`, default first then alphabetical by name.
fn commands_for(cmds: &[Cmd], dir: &str) -> Vec<Cmd> {
    let mut here: Vec<Cmd> = cmds.iter().filter(|c| c.path == dir).cloned().collect();
    here.sort_by(|a, b| a.name.cmp(&b.name));
    here
}

fn load_counts() -> HashMap<String, u64> {
    match fs::File::open(counts_path()) {
        Ok(f) => BufReader::new(f)
            .lines()
            .map_while(Result::ok)
            .filter_map(|l| {
                // path \t name \t count
                let mut parts = l.rsplitn(2, '\t');
                let count = parts.next()?.parse::<u64>().ok()?;
                let key = parts.next()?.to_string();
                Some((key, count))
            })
            .collect(),
        Err(_) => HashMap::new(),
    }
}

fn save_counts(counts: &HashMap<String, u64>) -> io::Result<()> {
    let dir = config_dir();
    if !dir.exists() {
        fs::create_dir_all(&dir)?;
    }
    let content: String = counts
        .iter()
        .map(|(key, count)| format!("{}\t{}", key, count))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    fs::write(counts_path(), content)
}

fn increment_count(key: &str) {
    let mut counts = load_counts();
    *counts.entry(key.to_string()).or_insert(0) += 1;
    let _ = save_counts(&counts);
}

fn is_reserved(name: &str) -> bool {
    matches!(
        name,
        "set" | "rm" | "unset" | "remove" | "list" | "ls" | "edit" | "report" | "help"
    )
}

/// Run a stored command through the user's shell, appending any extra args,
/// then exit with the command's status code.
fn run_command(cmd: &Cmd, extra: &[String]) -> ! {
    let mut full = cmd.command.clone();
    if !extra.is_empty() {
        full.push(' ');
        full.push_str(&extra.join(" "));
    }

    eprintln!("\x1b[2m▶ {}\x1b[0m", full);
    increment_count(&cmd.count_key());

    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
    let status = process::Command::new(&shell)
        .arg("-c")
        .arg(&full)
        .current_dir(&cmd.path)
        .status();

    match status {
        Ok(s) => process::exit(s.code().unwrap_or(1)),
        Err(e) => {
            eprintln!("gg: failed to run command: {}", e);
            process::exit(1);
        }
    }
}

fn print_commands_for(dir: &str) {
    let here = commands_for(&load_commands(), dir);
    if here.is_empty() {
        eprintln!("gg: no commands configured for {}", dir);
        eprintln!("    set one with:  gg set <command…>");
        return;
    }
    let counts = load_counts();
    let width = here.iter().map(|c| c.label().len()).max().unwrap_or(0);
    println!("{}", dir);
    for c in &here {
        let n = counts.get(&c.count_key()).copied().unwrap_or(0);
        let runs = if n > 0 {
            format!("  \x1b[2m({} runs)\x1b[0m", n)
        } else {
            String::new()
        };
        println!(
            "  {:width$}  {}{}",
            c.label(),
            c.command,
            runs,
            width = width
        );
    }
}

fn print_all_dirs() {
    let cmds = load_commands();
    if cmds.is_empty() {
        eprintln!("gg: no commands configured anywhere");
        return;
    }
    let mut dirs: Vec<&String> = cmds.iter().map(|c| &c.path).collect();
    dirs.sort();
    dirs.dedup();
    let counts = load_counts();
    for dir in dirs {
        println!("{}", dir);
        let mut here: Vec<&Cmd> = cmds.iter().filter(|c| &c.path == dir).collect();
        here.sort_by(|a, b| a.name.cmp(&b.name));
        let width = here.iter().map(|c| c.label().len()).max().unwrap_or(0);
        for c in here {
            let n = counts.get(&c.count_key()).copied().unwrap_or(0);
            let runs = if n > 0 {
                format!("  \x1b[2m({} runs)\x1b[0m", n)
            } else {
                String::new()
            };
            println!(
                "  {:width$}  {}{}",
                c.label(),
                c.command,
                runs,
                width = width
            );
        }
    }
}

fn handle_set(rest: &[String]) {
    if rest.is_empty() {
        eprintln!("gg: nothing to set");
        eprintln!("    usage:  gg set <command…>            (default)");
        eprintln!("            gg set <name> -- <command…>  (named)");
        process::exit(1);
    }

    // `gg set <name> -- <command…>` => named; otherwise the whole rest is the
    // default command.
    let (name, command) = match rest.iter().position(|a| a == "--") {
        Some(idx) => {
            if idx != 1 {
                eprintln!("gg: the name before `--` must be a single token");
                process::exit(1);
            }
            let name = rest[0].clone();
            let command = rest[idx + 1..].join(" ");
            (name, command)
        }
        None => (String::new(), rest.join(" ")),
    };

    if command.trim().is_empty() {
        eprintln!("gg: empty command");
        process::exit(1);
    }
    if name.contains('\t') {
        eprintln!("gg: command names may not contain tabs");
        process::exit(1);
    }
    if !name.is_empty() && is_reserved(&name) {
        eprintln!(
            "gg: warning: '{}' is a reserved word — `gg {}` won't run it, use `gg list` to see it",
            name, name
        );
    }

    let dir = cwd();
    let mut cmds = load_commands();
    let existed = cmds.iter().any(|c| c.path == dir && c.name == name);
    cmds.retain(|c| !(c.path == dir && c.name == name));
    cmds.push(Cmd {
        path: dir.clone(),
        name: name.clone(),
        command: command.clone(),
    });
    cmds.sort_by(|a, b| a.path.cmp(&b.path).then(a.name.cmp(&b.name)));

    if let Err(e) = save_commands(&cmds) {
        eprintln!("gg: failed to save: {}", e);
        process::exit(1);
    }

    let label = if name.is_empty() { "(default)" } else { &name };
    let verb = if existed { "replaced" } else { "set" };
    eprintln!("gg: {} {} = {}", verb, label, command);
}

fn handle_rm(rest: &[String]) {
    let dir = cwd();
    let mut cmds = load_commands();

    if rest.iter().any(|a| a == "--all") {
        let before = cmds.len();
        cmds.retain(|c| c.path != dir);
        let _ = save_commands(&cmds);
        eprintln!("gg: removed {} command(s) for {}", before - cmds.len(), dir);
        return;
    }

    let name = rest.first().cloned().unwrap_or_default();
    let existed = cmds.iter().any(|c| c.path == dir && c.name == name);
    if !existed {
        let label = if name.is_empty() { "(default)" } else { &name };
        eprintln!("gg: no command '{}' configured for {}", label, dir);
        process::exit(1);
    }
    cmds.retain(|c| !(c.path == dir && c.name == name));
    let _ = save_commands(&cmds);
    let label = if name.is_empty() { "(default)" } else { &name };
    eprintln!("gg: removed {}", label);
}

fn handle_edit() {
    let path = commands_path();
    let dir = config_dir();
    if !dir.exists() {
        let _ = fs::create_dir_all(&dir);
    }
    if !path.exists() {
        let _ = fs::write(&path, "");
    }
    let editor = std::env::var("EDITOR")
        .or_else(|_| std::env::var("VISUAL"))
        .unwrap_or_else(|_| "vi".to_string());
    let _ = process::Command::new(&editor).arg(&path).status();
}

fn print_help() {
    println!(
        "gg — per-directory command shortcuts

USAGE
  gg                       run the default command for this directory
  gg <name>                run the named command (e.g. gg build)
  gg <name> [args…]        run it, appending extra args (e.g. gg test --watch)
  gg set <command…>        set/replace the default command for this directory
  gg set <name> -- <cmd…>  set/replace a named command
  gg rm [name]             remove the default (or named) command
  gg rm --all              remove every command for this directory
  gg list [--all]          list commands for this dir (or all dirs)
  gg edit                  open the config file in $EDITOR
  gg report                show a usage report
  gg help                  show this help

Running bare `gg` with no default but several named commands opens a picker.
Commands run through $SHELL, so pipes, &&, and env expansion all work."
    );
}

// ---------------------------------------------------------------------------
// Interactive picker (bare `gg` with no default) and usage report.
// ---------------------------------------------------------------------------

fn filter_cmds<'a>(cmds: &'a [Cmd], query: &str) -> Vec<&'a Cmd> {
    let q = query.to_lowercase();
    if q.is_empty() {
        cmds.iter().collect()
    } else {
        cmds.iter()
            .filter(|c| c.name.to_lowercase().contains(&q) || c.command.to_lowercase().contains(&q))
            .collect()
    }
}

struct Picker {
    cmds: Vec<Cmd>,
    query: String,
    selection: usize,
    scroll_offset: usize,
}

impl Picker {
    fn filtered(&self) -> Vec<&Cmd> {
        filter_cmds(&self.cmds, &self.query)
    }

    fn adjust_scroll(&mut self, list_height: usize) {
        if list_height == 0 {
            self.scroll_offset = 0;
            return;
        }
        if self.selection < self.scroll_offset {
            self.scroll_offset = self.selection;
        } else if self.selection >= self.scroll_offset + list_height {
            self.scroll_offset = self.selection - list_height + 1;
        }
    }
}

fn render_picker<W: Write>(
    w: &mut W,
    picker: &Picker,
    counts: &HashMap<String, u64>,
    dir: &str,
    term_width: u16,
    term_height: u16,
) -> io::Result<()> {
    let list_height = if term_height > 3 {
        (term_height - 3) as usize
    } else {
        0
    };
    let filtered = picker.filtered();

    queue!(w, cursor::MoveTo(0, 0), Clear(ClearType::All))?;

    queue!(
        w,
        cursor::MoveTo(0, 0),
        SetAttribute(Attribute::Bold),
        Print("  > "),
        Print(&picker.query),
        SetAttribute(Attribute::Reset),
    )?;

    let sep_width = if term_width > 4 {
        (term_width - 4) as usize
    } else {
        0
    };
    let sep: String = "─".repeat(sep_width);
    queue!(
        w,
        cursor::MoveTo(0, 1),
        SetForegroundColor(Color::DarkGrey),
        Print("  "),
        Print(&sep),
        ResetColor,
    )?;

    let label_width = filtered.iter().map(|c| c.label().len()).max().unwrap_or(0);

    let visible: Vec<_> = filtered
        .iter()
        .enumerate()
        .skip(picker.scroll_offset)
        .take(list_height)
        .collect();

    for (i, (idx, c)) in visible.iter().enumerate() {
        let row = (2 + i) as u16;
        let n = counts.get(&c.count_key()).copied().unwrap_or(0);
        let count_str = if n > 0 {
            format!("  ({})", n)
        } else {
            String::new()
        };
        let line = format!("  {:width$}  {}", c.label(), c.command, width = label_width);
        queue!(w, cursor::MoveTo(0, row))?;
        if *idx == picker.selection {
            queue!(
                w,
                SetAttribute(Attribute::Reverse),
                SetAttribute(Attribute::Bold),
                Print(&line),
                SetAttribute(Attribute::Reset),
                SetForegroundColor(Color::DarkGrey),
                Print(&count_str),
                ResetColor,
            )?;
        } else {
            queue!(
                w,
                Print(&line),
                SetForegroundColor(Color::DarkGrey),
                Print(&count_str),
                ResetColor,
            )?;
        }
    }

    let footer = format!("  {}  —  ↑↓ navigate  enter run  esc quit", dir);
    let footer: String = footer
        .chars()
        .take((term_width as usize).saturating_sub(1))
        .collect();
    queue!(
        w,
        cursor::MoveTo(0, term_height - 1),
        SetForegroundColor(Color::DarkGrey),
        Print(&footer),
        ResetColor,
    )?;

    let cursor_col = 4 + picker.query.chars().count() as u16;
    queue!(w, cursor::MoveTo(cursor_col, 0))?;

    w.flush()
}

/// Show a picker over `cmds`; returns the chosen command, or None if cancelled.
fn pick(cmds: Vec<Cmd>, dir: &str) -> io::Result<Option<Cmd>> {
    let counts = load_counts();
    let tty = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/tty")?;
    let mut tty_write = tty.try_clone()?;

    enable_raw_mode()?;
    execute!(tty_write, EnterAlternateScreen, cursor::Hide)?;

    let mut picker = Picker {
        cmds,
        query: String::new(),
        selection: 0,
        scroll_offset: 0,
    };

    let result: io::Result<Option<Cmd>> = (|| loop {
        let (term_width, term_height) = terminal::size()?;
        let list_height = if term_height > 3 {
            (term_height - 3) as usize
        } else {
            0
        };
        picker.adjust_scroll(list_height);
        render_picker(
            &mut tty_write,
            &picker,
            &counts,
            dir,
            term_width,
            term_height,
        )?;

        match event::read()? {
            Event::Key(KeyEvent {
                code: KeyCode::Esc, ..
            }) => return Ok(None),
            Event::Key(KeyEvent {
                code: KeyCode::Char('c'),
                modifiers: KeyModifiers::CONTROL,
                ..
            }) => return Ok(None),
            Event::Key(KeyEvent {
                code: KeyCode::Enter,
                ..
            }) => {
                let filtered = picker.filtered();
                if let Some(c) = filtered.get(picker.selection) {
                    return Ok(Some((*c).clone()));
                }
            }
            Event::Key(KeyEvent {
                code: KeyCode::Up, ..
            }) => {
                if picker.selection > 0 {
                    picker.selection -= 1;
                }
            }
            Event::Key(KeyEvent {
                code: KeyCode::Down,
                ..
            }) => {
                let len = picker.filtered().len();
                if len > 0 && picker.selection < len - 1 {
                    picker.selection += 1;
                }
            }
            Event::Key(KeyEvent {
                code: KeyCode::Backspace,
                ..
            }) => {
                picker.query.pop();
                picker.selection = 0;
                picker.scroll_offset = 0;
            }
            Event::Key(KeyEvent {
                code: KeyCode::Char(c),
                modifiers,
                ..
            }) if modifiers == KeyModifiers::NONE || modifiers == KeyModifiers::SHIFT => {
                picker.query.push(c);
                picker.selection = 0;
                picker.scroll_offset = 0;
            }
            Event::Resize(_, _) => {}
            _ => {}
        }
    })();

    let _ = execute!(tty_write, LeaveAlternateScreen, cursor::Show);
    let _ = disable_raw_mode();
    result
}

fn report() {
    let cmds = load_commands();
    let counts = load_counts();
    let mut entries: Vec<(&Cmd, u64)> = cmds
        .iter()
        .map(|c| (c, counts.get(&c.count_key()).copied().unwrap_or(0)))
        .filter(|(_, n)| *n > 0)
        .collect();
    if entries.is_empty() {
        eprintln!("gg: no usage recorded yet");
        return;
    }
    entries.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.command.cmp(&b.0.command)));

    let max_count = entries.iter().map(|(_, n)| *n).max().unwrap_or(0);
    let max_label = entries
        .iter()
        .map(|(c, _)| c.command.len())
        .max()
        .unwrap_or(0);
    println!("gg usage report");
    for (c, n) in entries {
        let bar_budget = 40usize;
        let bar_len = if max_count > 0 {
            (n as f64 / max_count as f64 * bar_budget as f64).round() as usize
        } else {
            0
        };
        let bar = "█".repeat(bar_len);
        println!(
            "  {:width$}  {:>4}  {}",
            c.command,
            n,
            bar,
            width = max_label
        );
    }
}

fn shorten(dir: &str) -> &str {
    // Display the basename in messages when the path is long.
    Path::new(dir)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(dir)
}

fn run_or_pick() -> ! {
    let dir = cwd();
    let here = commands_for(&load_commands(), &dir);

    if here.is_empty() {
        eprintln!("gg: no command configured for {}", shorten(&dir));
        eprintln!("    set one with:  gg set <command…>");
        process::exit(1);
    }

    // Prefer the default if present.
    if let Some(def) = here.iter().find(|c| c.name.is_empty()) {
        run_command(def, &[]);
    }

    // No default: a single named command runs directly, several open a picker.
    if here.len() == 1 {
        run_command(&here[0], &[]);
    }

    match pick(here, &dir) {
        Ok(Some(c)) => run_command(&c, &[]),
        Ok(None) => process::exit(0),
        Err(e) => {
            eprintln!("gg: {}", e);
            process::exit(1);
        }
    }
}

fn run_named(name: &str, extra: &[String]) -> ! {
    let dir = cwd();
    let here = commands_for(&load_commands(), &dir);
    if let Some(c) = here.iter().find(|c| c.name == name) {
        run_command(c, extra);
    }
    eprintln!("gg: no command '{}' configured for {}", name, shorten(&dir));
    if !here.is_empty() {
        let names: Vec<&str> = here.iter().map(|c| c.label()).collect();
        eprintln!("    available: {}", names.join(", "));
    }
    process::exit(1);
}

fn main() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = execute!(io::stderr(), LeaveAlternateScreen, cursor::Show);
        let _ = disable_raw_mode();
        default_hook(info);
    }));

    let args: Vec<String> = std::env::args().skip(1).collect();

    match args.first().map(|s| s.as_str()) {
        None => run_or_pick(),
        Some("help") | Some("-h") | Some("--help") => print_help(),
        Some("set") => handle_set(&args[1..]),
        Some("rm") | Some("unset") | Some("remove") => handle_rm(&args[1..]),
        Some("list") | Some("ls") => {
            if args[1..].iter().any(|a| a == "--all") {
                print_all_dirs();
            } else {
                print_commands_for(&cwd());
            }
        }
        Some("edit") => handle_edit(),
        Some("report") => report(),
        Some(name) => run_named(name, &args[1..]),
    }
}
