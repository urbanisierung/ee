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
    path::PathBuf,
    process,
    rc::Rc,
};

const HISTORY_FILE: &str = ".ee_history";
const COUNTS_FILE: &str = ".ee_counts";
const ENVVARS_DIR: &str = ".config/envvars";

fn envvars_dir() -> PathBuf {
    let home = std::env::var("HOME").expect("HOME environment variable must be set");
    PathBuf::from(home).join(ENVVARS_DIR)
}

fn history_path() -> PathBuf {
    envvars_dir().join(HISTORY_FILE)
}

fn counts_path() -> PathBuf {
    envvars_dir().join(COUNTS_FILE)
}

fn load_counts(path: &PathBuf) -> HashMap<String, u64> {
    match fs::File::open(path) {
        Ok(f) => BufReader::new(f)
            .lines()
            .map_while(Result::ok)
            .filter_map(|l| {
                let mut parts = l.splitn(2, '\t');
                let name = parts.next()?.to_string();
                let count = parts.next()?.parse::<u64>().ok()?;
                Some((name, count))
            })
            .collect(),
        Err(_) => HashMap::new(),
    }
}

fn save_counts(path: &PathBuf, counts: &HashMap<String, u64>) -> io::Result<()> {
    let content: String = counts
        .iter()
        .map(|(name, count)| format!("{}\t{}", name, count))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    fs::write(path, content)
}

fn increment_count(path: &PathBuf, filename: &str) {
    let mut counts = load_counts(path);
    *counts.entry(filename.to_string()).or_insert(0) += 1;
    let _ = save_counts(path, &counts);
}

/// Load the MRU history list (filenames only, not paths).
fn load_history(path: &PathBuf) -> Vec<String> {
    match fs::File::open(path) {
        Ok(f) => BufReader::new(f)
            .lines()
            .map_while(Result::ok)
            .filter(|l| !l.trim().is_empty())
            .collect(),
        Err(_) => vec![],
    }
}

/// Persist a new history list (most recent first).
fn save_history(path: &PathBuf, history: &[String]) -> io::Result<()> {
    let content = history.join("\n") + "\n";
    fs::write(path, content)
}

/// Record a file as most recently used.
fn record_usage(history_path: &PathBuf, filename: &str) {
    let mut history = load_history(history_path);
    history.retain(|h| h != filename);
    history.insert(0, filename.to_string());
    let _ = save_history(history_path, &history);
}

/// Discover files in the envvars directory, sorted by MRU then alphabetically.
fn discover_files(dir: &PathBuf, history: &[String]) -> Vec<String> {
    let entries: Vec<String> = match fs::read_dir(dir) {
        Ok(rd) => {
            let mut names: Vec<String> = rd
                .filter_map(|e| e.ok())
                .filter(|e| {
                    e.file_type().map(|ft| ft.is_file()).unwrap_or(false)
                        && e.file_name() != HISTORY_FILE
                        && e.file_name() != COUNTS_FILE
                })
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect();
            names.sort();
            names
        }
        Err(_) => vec![],
    };

    // MRU first (only those present in entries), then remaining alphabetically
    let mut result: Vec<String> = vec![];
    for h in history {
        if entries.contains(h) {
            result.push(h.clone());
        }
    }
    for e in &entries {
        if !result.contains(e) {
            result.push(e.clone());
        }
    }
    result
}

/// Filter files by case-insensitive substring query.
fn filter_files<'a>(files: &'a [String], query: &str) -> Vec<&'a String> {
    let q = query.to_lowercase();
    if q.is_empty() {
        files.iter().collect()
    } else {
        files
            .iter()
            .filter(|f| f.to_lowercase().contains(&q))
            .collect()
    }
}

/// Read a file into a vec of lines for the preview.
fn load_file_lines(path: &PathBuf) -> Vec<String> {
    match fs::File::open(path) {
        Ok(f) => BufReader::new(f)
            .lines()
            .map(|l| l.unwrap_or_else(|e| format!("(error reading line: {})", e)))
            .collect(),
        Err(e) => vec![format!("(error reading file: {})", e)],
    }
}

fn open_in_editor<W: Write>(tty_write: &mut W, file_path: &PathBuf) -> io::Result<()> {
    execute!(tty_write, LeaveAlternateScreen, cursor::Show)?;
    disable_raw_mode()?;

    let editor = std::env::var("EDITOR")
        .or_else(|_| std::env::var("VISUAL"))
        .unwrap_or_else(|_| "vi".to_string());
    let tty_in = fs::File::open("/dev/tty")?;
    let tty_out = fs::OpenOptions::new().write(true).open("/dev/tty")?;
    let tty_err = fs::OpenOptions::new().write(true).open("/dev/tty")?;
    let _ = process::Command::new(&editor)
        .arg(file_path)
        .stdin(tty_in)
        .stdout(tty_out)
        .stderr(tty_err)
        .status();

    enable_raw_mode()?;
    execute!(tty_write, EnterAlternateScreen, cursor::Hide)?;
    Ok(())
}

enum Mode {
    List,
    /// Preview mode carries the lines of the currently-previewed file via a
    /// reference-counted pointer so that the per-frame clone is O(1).
    Preview(Rc<Vec<String>>),
    Report,
}

struct App {
    files: Vec<String>,
    query: String,
    selection: usize,
    scroll_offset: usize,
    mode: Mode,
    preview_scroll: usize,
    report_scroll: usize,
}

impl App {
    fn new(files: Vec<String>) -> Self {
        App {
            files,
            query: String::new(),
            selection: 0,
            scroll_offset: 0,
            mode: Mode::List,
            preview_scroll: 0,
            report_scroll: 0,
        }
    }

    fn filtered(&self) -> Vec<&String> {
        filter_files(&self.files, &self.query)
    }

    fn clamp_selection(&mut self) {
        let len = self.filtered().len();
        if len == 0 {
            self.selection = 0;
        } else if self.selection >= len {
            self.selection = len - 1;
        }
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

fn render_list<W: Write>(w: &mut W, app: &App, term_width: u16, term_height: u16) -> io::Result<()> {
    // Layout:
    //  row 0: search input
    //  row 1: separator
    //  rows 2..term_height-2: file list
    //  row term_height-1: footer

    let list_height = if term_height > 3 {
        (term_height - 3) as usize
    } else {
        0
    };

    let filtered = app.filtered();

    // Clear screen and move to top-left
    queue!(w, cursor::MoveTo(0, 0), Clear(ClearType::All))?;

    // --- Search input row ---
    queue!(
        w,
        cursor::MoveTo(0, 0),
        SetAttribute(Attribute::Bold),
        Print("  > "),
        Print(&app.query),
        SetAttribute(Attribute::Reset),
    )?;

    // --- Separator ---
    let sep_width = if term_width > 4 { (term_width - 4) as usize } else { 0 };
    let sep: String = "─".repeat(sep_width);
    queue!(
        w,
        cursor::MoveTo(0, 1),
        SetForegroundColor(Color::DarkGrey),
        Print("  "),
        Print(&sep),
        ResetColor,
    )?;

    // --- File list ---
    let visible: Vec<_> = filtered
        .iter()
        .enumerate()
        .skip(app.scroll_offset)
        .take(list_height)
        .collect();

    for (i, (idx, name)) in visible.iter().enumerate() {
        let row = (2 + i) as u16;
        queue!(w, cursor::MoveTo(0, row))?;
        if *idx == app.selection {
            queue!(
                w,
                SetAttribute(Attribute::Reverse),
                SetAttribute(Attribute::Bold),
                Print(format!("  {}", name)),
                SetAttribute(Attribute::Reset),
            )?;
        } else {
            queue!(w, Print(format!("  {}", name)))?;
        }
    }

    // --- Footer ---
    let footer = "  ↑↓ navigate  → preview  enter source  e edit  r report  esc quit";
    queue!(
        w,
        cursor::MoveTo(0, term_height - 1),
        SetForegroundColor(Color::DarkGrey),
        Print(footer),
        ResetColor,
    )?;

    // Park cursor after query text on input row
    let cursor_col = 4 + app.query.len() as u16;
    queue!(w, cursor::MoveTo(cursor_col, 0))?;

    w.flush()
}

fn render_report<W: Write>(
    w: &mut W,
    files: &[String],
    counts: &HashMap<String, u64>,
    scroll: usize,
    term_width: u16,
    term_height: u16,
) -> io::Result<()> {
    let content_height = if term_height > 3 {
        (term_height - 3) as usize
    } else {
        0
    };

    queue!(w, cursor::MoveTo(0, 0), Clear(ClearType::All))?;

    // --- Header ---
    queue!(
        w,
        cursor::MoveTo(0, 0),
        SetAttribute(Attribute::Bold),
        Print("  Source report"),
        SetAttribute(Attribute::Reset),
    )?;

    // --- Separator ---
    let sep_width = if term_width > 4 { (term_width - 4) as usize } else { 0 };
    let sep: String = "─".repeat(sep_width);
    queue!(
        w,
        cursor::MoveTo(0, 1),
        SetForegroundColor(Color::DarkGrey),
        Print("  "),
        Print(&sep),
        ResetColor,
    )?;

    // Build sorted entries: descending by count, then alphabetical
    let mut entries: Vec<(&String, u64)> = files
        .iter()
        .map(|f| (f, *counts.get(f).unwrap_or(&0)))
        .collect();
    entries.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));

    let max_count = entries.iter().map(|(_, c)| *c).max().unwrap_or(0);
    let max_name_len = entries.iter().map(|(n, _)| n.len()).max().unwrap_or(0);

    // Available width for the bar (after name + count + padding)
    let count_width = if max_count > 0 {
        (max_count as f64).log10().floor() as usize + 1
    } else {
        1
    };
    // Layout: "  name  count  ████"
    let bar_budget = (term_width as usize)
        .saturating_sub(2 + max_name_len + 2 + count_width + 2);

    let visible = entries.iter().skip(scroll).take(content_height);
    for (i, (name, count)) in visible.enumerate() {
        let row = (2 + i) as u16;
        queue!(w, cursor::MoveTo(0, row))?;

        let bar_len = if max_count > 0 {
            (*count as f64 / max_count as f64 * bar_budget as f64).round() as usize
        } else {
            0
        };
        let bar: String = "█".repeat(bar_len);

        queue!(
            w,
            Print(format!("  {:width$}  ", name, width = max_name_len)),
            SetForegroundColor(Color::DarkGrey),
            Print(format!("{:>cw$}  ", count, cw = count_width)),
            ResetColor,
            SetForegroundColor(Color::Green),
            Print(&bar),
            ResetColor,
        )?;
    }

    // --- Footer ---
    let footer = "  ← esc back  ↑↓ scroll";
    queue!(
        w,
        cursor::MoveTo(0, term_height - 1),
        SetForegroundColor(Color::DarkGrey),
        Print(footer),
        ResetColor,
    )?;

    w.flush()
}

fn render_preview<W: Write>(
    w: &mut W,
    filename: &str,
    lines: &[String],
    scroll: usize,
    term_width: u16,
    term_height: u16,
) -> io::Result<()> {
    // Layout:
    //  row 0: filename header
    //  row 1: separator
    //  rows 2..term_height-2: file content (scrollable)
    //  row term_height-1: footer

    let content_height = if term_height > 3 {
        (term_height - 3) as usize
    } else {
        0
    };

    queue!(w, cursor::MoveTo(0, 0), Clear(ClearType::All))?;

    // --- Header: filename ---
    queue!(
        w,
        cursor::MoveTo(0, 0),
        SetAttribute(Attribute::Bold),
        Print(format!("  {}", filename)),
        SetAttribute(Attribute::Reset),
    )?;

    // --- Separator ---
    let sep_width = if term_width > 4 { (term_width - 4) as usize } else { 0 };
    let sep: String = "─".repeat(sep_width);
    queue!(
        w,
        cursor::MoveTo(0, 1),
        SetForegroundColor(Color::DarkGrey),
        Print("  "),
        Print(&sep),
        ResetColor,
    )?;

    // --- File content ---
    let max_chars = term_width.saturating_sub(4) as usize;
    for i in 0..content_height {
        let row = (2 + i) as u16;
        queue!(w, cursor::MoveTo(0, row))?;
        if let Some(line) = lines.get(scroll + i) {
            // Truncate long lines to avoid terminal wrapping artifacts
            let display: String = line.chars().take(max_chars).collect();
            queue!(w, Print(format!("  {}", display)))?;
        }
    }

    // --- Footer ---
    let footer = "  ← esc back  ↑↓ scroll  enter source  e edit  r report";
    queue!(
        w,
        cursor::MoveTo(0, term_height - 1),
        SetForegroundColor(Color::DarkGrey),
        Print(footer),
        ResetColor,
    )?;

    w.flush()
}

fn run() -> io::Result<Option<String>> {
    let dir = envvars_dir();

    // Ensure directory exists
    if !dir.exists() {
        fs::create_dir_all(&dir)?;
    }

    let hist_path = history_path();
    let cnt_path = counts_path();
    let history = load_history(&hist_path);
    let files = discover_files(&dir, &history);

    if files.is_empty() {
        eprintln!("No env files in ~/.config/envvars");
        process::exit(1);
    }

    // Open /dev/tty for TUI so that stdout stays clean
    let tty = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/tty")?;
    let mut tty_write = tty.try_clone()?;

    enable_raw_mode()?;
    execute!(tty_write, EnterAlternateScreen, cursor::Hide)?;

    let mut app = App::new(files);
    let result: io::Result<Option<String>> = (|| {
        loop {
            let (term_width, term_height) = terminal::size()?;

            // Clone preview lines (if in preview mode) before any rendering so that we
            // hold no borrow on `app.mode` when we later mutate it in the event handler.
            // `Rc::clone` is O(1) — just a reference-count increment.
            let current_mode = match &app.mode {
                Mode::Preview(lines) => Some((true, Rc::clone(lines))),
                Mode::Report => Some((false, Rc::new(vec![]))),
                Mode::List => None,
            };

            if let Some((is_preview, lines)) = current_mode {
            if !is_preview {
                // --- Report mode ---
                let counts = load_counts(&cnt_path);
                render_report(
                    &mut tty_write,
                    &app.files,
                    &counts,
                    app.report_scroll,
                    term_width,
                    term_height,
                )?;

                let content_height = if term_height > 3 {
                    (term_height - 3) as usize
                } else {
                    0
                };

                match event::read()? {
                    Event::Key(KeyEvent { code: KeyCode::Esc, .. })
                    | Event::Key(KeyEvent { code: KeyCode::Left, .. }) => {
                        app.mode = Mode::List;
                    }
                    Event::Key(KeyEvent {
                        code: KeyCode::Char('c'),
                        modifiers: KeyModifiers::CONTROL,
                        ..
                    }) => {
                        return Ok(None);
                    }
                    Event::Key(KeyEvent { code: KeyCode::Up, .. }) => {
                        if app.report_scroll > 0 {
                            app.report_scroll -= 1;
                        }
                    }
                    Event::Key(KeyEvent { code: KeyCode::Down, .. }) => {
                        let total = app.files.len();
                        if app.report_scroll + content_height < total {
                            app.report_scroll += 1;
                        }
                    }
                    Event::Resize(_, _) => {}
                    _ => {}
                }
            } else {
                // --- Preview mode ---
                let filename = filter_files(&app.files, &app.query)
                    .get(app.selection)
                    .map(|s| s.to_string())
                    .unwrap_or_default();

                render_preview(
                    &mut tty_write,
                    &filename,
                    &lines,
                    app.preview_scroll,
                    term_width,
                    term_height,
                )?;

                let content_height = if term_height > 3 {
                    (term_height - 3) as usize
                } else {
                    0
                };

                match event::read()? {
                    Event::Key(KeyEvent { code: KeyCode::Esc, .. })
                    | Event::Key(KeyEvent { code: KeyCode::Left, .. }) => {
                        app.mode = Mode::List;
                    }
                    Event::Key(KeyEvent {
                        code: KeyCode::Char('c'),
                        modifiers: KeyModifiers::CONTROL,
                        ..
                    }) => {
                        return Ok(None);
                    }
                    Event::Key(KeyEvent { code: KeyCode::Enter, .. }) => {
                        let filtered = filter_files(&app.files, &app.query);
                        if let Some(name) = filtered.get(app.selection) {
                            let name = name.to_string();
                            record_usage(&hist_path, &name);
                            increment_count(&cnt_path, &name);
                            return Ok(Some(name));
                        }
                    }
                    Event::Key(KeyEvent { code: KeyCode::Up, .. }) => {
                        if app.preview_scroll > 0 {
                            app.preview_scroll -= 1;
                        }
                    }
                    Event::Key(KeyEvent { code: KeyCode::Down, .. }) => {
                        if app.preview_scroll + content_height < lines.len() {
                            app.preview_scroll += 1;
                        }
                    }
                    Event::Key(KeyEvent { code: KeyCode::Char('e'), .. }) => {
                        let filtered = filter_files(&app.files, &app.query);
                        if let Some(name) = filtered.get(app.selection) {
                            let file_path = dir.join(name.as_str());
                            open_in_editor(&mut tty_write, &file_path)?;
                            let new_lines = Rc::new(load_file_lines(&file_path));
                            app.mode = Mode::Preview(new_lines);
                        }
                    }
                    Event::Key(KeyEvent { code: KeyCode::Char('r'), .. }) => {
                        app.mode = Mode::Report;
                        app.report_scroll = 0;
                    }
                    Event::Resize(_, _) => {}
                    _ => {}
                }
            }
            } else {
                // --- List mode ---
                let list_height = if term_height > 3 {
                    (term_height - 3) as usize
                } else {
                    0
                };
                app.adjust_scroll(list_height);
                render_list(&mut tty_write, &app, term_width, term_height)?;

                match event::read()? {
                    Event::Key(KeyEvent { code: KeyCode::Esc, .. }) => {
                        return Ok(None);
                    }
                    Event::Key(KeyEvent {
                        code: KeyCode::Char('c'),
                        modifiers: KeyModifiers::CONTROL,
                        ..
                    }) => {
                        return Ok(None);
                    }
                    Event::Key(KeyEvent { code: KeyCode::Enter, .. }) => {
                        let filtered = app.filtered();
                        if let Some(name) = filtered.get(app.selection) {
                            let name = name.to_string();
                            record_usage(&hist_path, &name);
                            increment_count(&cnt_path, &name);
                            return Ok(Some(name));
                        }
                    }
                    Event::Key(KeyEvent { code: KeyCode::Up, .. }) => {
                        if app.selection > 0 {
                            app.selection -= 1;
                        }
                    }
                    Event::Key(KeyEvent { code: KeyCode::Down, .. }) => {
                        let len = app.filtered().len();
                        if len > 0 && app.selection < len - 1 {
                            app.selection += 1;
                        }
                    }
                    Event::Key(KeyEvent { code: KeyCode::Right, .. }) => {
                        let filtered = app.filtered();
                        if let Some(name) = filtered.get(app.selection) {
                            let file_path = dir.join(name.as_str());
                            let lines = Rc::new(load_file_lines(&file_path));
                            app.mode = Mode::Preview(lines);
                            app.preview_scroll = 0;
                        }
                    }
                    Event::Key(KeyEvent { code: KeyCode::Backspace, .. }) => {
                        app.query.pop();
                        app.selection = 0;
                        app.scroll_offset = 0;
                    }
                    Event::Key(KeyEvent {
                        code: KeyCode::Char('e'),
                        modifiers: KeyModifiers::NONE,
                        ..
                    }) if app.query.is_empty() => {
                        let filtered = app.filtered();
                        if let Some(name) = filtered.get(app.selection) {
                            let file_path = dir.join(name.as_str());
                            open_in_editor(&mut tty_write, &file_path)?;
                        }
                    }
                    Event::Key(KeyEvent {
                        code: KeyCode::Char('r'),
                        modifiers: KeyModifiers::NONE,
                        ..
                    }) if app.query.is_empty() => {
                        app.mode = Mode::Report;
                        app.report_scroll = 0;
                    }
                    Event::Key(KeyEvent {
                        code: KeyCode::Char(c),
                        modifiers,
                        ..
                    }) if modifiers == KeyModifiers::NONE || modifiers == KeyModifiers::SHIFT => {
                        app.query.push(c);
                        app.selection = 0;
                        app.scroll_offset = 0;
                        app.clamp_selection();
                    }
                    Event::Resize(_, _) => {}
                    _ => {}
                }
            }
        }
    })();

    // Always restore terminal
    let _ = execute!(tty_write, LeaveAlternateScreen, cursor::Show);
    let _ = disable_raw_mode();

    result
}

fn main() {
    // Set up a panic hook that restores the terminal before printing the panic message
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = execute!(io::stderr(), LeaveAlternateScreen, cursor::Show);
        let _ = disable_raw_mode();
        default_hook(info);
    }));

    match run() {
        Ok(Some(filename)) => {
            let dir = envvars_dir();
            let path = dir.join(&filename);
            // Print confirmation to the terminal (not stdout, which is reserved for eval)
            if let Ok(mut tty) = fs::OpenOptions::new().write(true).open("/dev/tty") {
                let msg = format!("\r\n  \u{2713} sourced {}\r\n", filename);
                let _ = tty.write_all(msg.as_bytes());
            }
            println!("source {}", path.display());
        }
        Ok(None) => {
            // Esc or Ctrl-C: exit cleanly, no output
            process::exit(0);
        }
        Err(e) => {
            eprintln!("ee: error: {}", e);
            process::exit(1);
        }
    }
}
