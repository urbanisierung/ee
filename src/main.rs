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
    fs,
    io::{self, BufRead, BufReader, Write},
    path::PathBuf,
    process,
};

const HISTORY_FILE: &str = ".ee_history";
const ENVVARS_DIR: &str = ".config/envvars";

fn envvars_dir() -> PathBuf {
    let home = std::env::var("HOME").expect("HOME environment variable must be set");
    PathBuf::from(home).join(ENVVARS_DIR)
}

fn history_path() -> PathBuf {
    envvars_dir().join(HISTORY_FILE)
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

struct App {
    files: Vec<String>,
    query: String,
    selection: usize,
    scroll_offset: usize,
}

impl App {
    fn new(files: Vec<String>) -> Self {
        App {
            files,
            query: String::new(),
            selection: 0,
            scroll_offset: 0,
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

fn render<W: Write>(w: &mut W, app: &App, term_width: u16, term_height: u16) -> io::Result<()> {
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
    let footer = "  ↑↓ navigate  enter source  esc quit";
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

fn run() -> io::Result<Option<String>> {
    let dir = envvars_dir();

    // Ensure directory exists
    if !dir.exists() {
        fs::create_dir_all(&dir)?;
    }

    let hist_path = history_path();
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
            let list_height = if term_height > 3 {
                (term_height - 3) as usize
            } else {
                0
            };
            app.adjust_scroll(list_height);
            render(&mut tty_write, &app, term_width, term_height)?;

            match event::read()? {
                Event::Key(KeyEvent {
                    code: KeyCode::Esc, ..
                }) => {
                    return Ok(None);
                }
                Event::Key(KeyEvent {
                    code: KeyCode::Char('c'),
                    modifiers: KeyModifiers::CONTROL,
                    ..
                }) => {
                    return Ok(None);
                }
                Event::Key(KeyEvent {
                    code: KeyCode::Enter,
                    ..
                }) => {
                    let filtered = app.filtered();
                    if let Some(name) = filtered.get(app.selection) {
                        let name = name.to_string();
                        record_usage(&hist_path, &name);
                        return Ok(Some(name));
                    }
                }
                Event::Key(KeyEvent {
                    code: KeyCode::Up, ..
                }) => {
                    if app.selection > 0 {
                        app.selection -= 1;
                    }
                }
                Event::Key(KeyEvent {
                    code: KeyCode::Down,
                    ..
                }) => {
                    let len = app.filtered().len();
                    if len > 0 && app.selection < len - 1 {
                        app.selection += 1;
                    }
                }
                Event::Key(KeyEvent {
                    code: KeyCode::Backspace,
                    ..
                }) => {
                    app.query.pop();
                    app.selection = 0;
                    app.scroll_offset = 0;
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
                Event::Resize(_, _) => {
                    // Re-render on next iteration
                }
                _ => {}
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
