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
    io::{self, BufReader, Write},
    path::PathBuf,
    process,
    time::{SystemTime, UNIX_EPOCH},
};

const HISTORY_FILE: &str = "history";
const COUNTS_FILE: &str = "counts";
const USAGE_LOG_FILE: &str = "usage_log";
const CONFIG_DIR: &str = ".config/eee";
const THREE_DAYS_SECS: u64 = 3 * 24 * 60 * 60;

fn config_dir() -> PathBuf {
    let home = std::env::var("HOME").expect("HOME environment variable must be set");
    PathBuf::from(home).join(CONFIG_DIR)
}

fn repos_dir() -> PathBuf {
    let home = std::env::var("HOME").expect("HOME environment variable must be set");
    PathBuf::from(home).join("github.com")
}

fn history_path() -> PathBuf {
    config_dir().join(HISTORY_FILE)
}

fn counts_path() -> PathBuf {
    config_dir().join(COUNTS_FILE)
}

fn usage_log_path() -> PathBuf {
    config_dir().join(USAGE_LOG_FILE)
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn append_usage_log(path: &PathBuf, name: &str) {
    let entry = format!("{}\t{}\n", now_secs(), name);
    let _ = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .and_then(|mut f| f.write_all(entry.as_bytes()));
}

fn load_3day_counts(path: &PathBuf) -> HashMap<String, u64> {
    let cutoff = now_secs().saturating_sub(THREE_DAYS_SECS);
    match fs::File::open(path) {
        Ok(f) => ee_tools::parse_windowed_counts(BufReader::new(f), cutoff),
        Err(_) => HashMap::new(),
    }
}

fn load_counts(path: &PathBuf) -> HashMap<String, u64> {
    match fs::File::open(path) {
        Ok(f) => ee_tools::parse_counts(BufReader::new(f)),
        Err(_) => HashMap::new(),
    }
}

fn save_counts(path: &PathBuf, counts: &HashMap<String, u64>) -> io::Result<()> {
    fs::write(path, ee_tools::format_counts(counts))
}

fn increment_count(path: &PathBuf, name: &str) {
    let mut counts = load_counts(path);
    *counts.entry(name.to_string()).or_insert(0) += 1;
    let _ = save_counts(path, &counts);
}

fn load_history(path: &PathBuf) -> Vec<String> {
    match fs::File::open(path) {
        Ok(f) => ee_tools::parse_history(BufReader::new(f)),
        Err(_) => vec![],
    }
}

fn save_history(path: &PathBuf, history: &[String]) -> io::Result<()> {
    fs::write(path, ee_tools::format_history(history))
}

fn record_usage(history_path: &PathBuf, usage_log_path: &PathBuf, name: &str) {
    let mut history = load_history(history_path);
    ee_tools::record_mru(&mut history, name);
    let _ = save_history(history_path, &history);
    append_usage_log(usage_log_path, name);
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum SortMode {
    Frequency,
    Alphabetical,
}

/// Discover repos at ~/github.com/<org>/<repo> (two levels deep).
fn discover_repos(base: &PathBuf) -> Vec<String> {
    let mut repos: Vec<String> = Vec::new();

    let orgs = match fs::read_dir(base) {
        Ok(rd) => rd,
        Err(_) => return repos,
    };

    for org_entry in orgs.filter_map(|e| e.ok()) {
        if !org_entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false) {
            continue;
        }
        let org_name = org_entry.file_name().to_string_lossy().into_owned();
        if let Ok(rd) = fs::read_dir(org_entry.path()) {
            for repo_entry in rd.filter_map(|e| e.ok()) {
                if repo_entry
                    .file_type()
                    .map(|ft| ft.is_dir())
                    .unwrap_or(false)
                {
                    let repo_name = repo_entry.file_name().to_string_lossy().into_owned();
                    repos.push(format!("{}/{}", org_name, repo_name));
                }
            }
        }
    }

    repos.sort();
    repos
}

fn sort_repos(repos: &[String], freq_counts: &HashMap<String, u64>, mode: SortMode) -> Vec<String> {
    match mode {
        SortMode::Alphabetical => repos.to_vec(),
        SortMode::Frequency => {
            let mut frequent: Vec<&String> = repos
                .iter()
                .filter(|r| freq_counts.get(r.as_str()).unwrap_or(&0) > &0)
                .collect();
            frequent.sort_by(|a, b| {
                let ca = freq_counts.get(a.as_str()).unwrap_or(&0);
                let cb = freq_counts.get(b.as_str()).unwrap_or(&0);
                cb.cmp(ca).then(a.cmp(b))
            });
            let mut result: Vec<String> = frequent.into_iter().cloned().collect();
            for r in repos {
                if !result.contains(r) {
                    result.push(r.clone());
                }
            }
            result
        }
    }
}

fn filter_repos<'a>(repos: &'a [String], query: &str) -> Vec<&'a String> {
    ee_tools::filter_by_substring(repos, query, |r| r.as_str())
}

enum Mode {
    List,
    Report,
}

struct App {
    all_repos: Vec<String>,
    repos: Vec<String>,
    query: String,
    selection: usize,
    scroll_offset: usize,
    mode: Mode,
    report_scroll: usize,
    sort_mode: SortMode,
    freq_counts: HashMap<String, u64>,
}

impl App {
    fn new(all_repos: Vec<String>, freq_counts: HashMap<String, u64>) -> Self {
        let repos = sort_repos(&all_repos, &freq_counts, SortMode::Frequency);
        App {
            all_repos,
            repos,
            query: String::new(),
            selection: 0,
            scroll_offset: 0,
            mode: Mode::List,
            report_scroll: 0,
            sort_mode: SortMode::Frequency,
            freq_counts,
        }
    }

    fn toggle_sort(&mut self) {
        self.sort_mode = match self.sort_mode {
            SortMode::Frequency => SortMode::Alphabetical,
            SortMode::Alphabetical => SortMode::Frequency,
        };
        self.repos = sort_repos(&self.all_repos, &self.freq_counts, self.sort_mode);
        self.selection = 0;
        self.scroll_offset = 0;
    }

    fn filtered(&self) -> Vec<&String> {
        filter_repos(&self.repos, &self.query)
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

fn render_list<W: Write>(
    w: &mut W,
    app: &App,
    counts: &HashMap<String, u64>,
    term_width: u16,
    term_height: u16,
) -> io::Result<()> {
    let list_height = if term_height > 3 {
        (term_height - 3) as usize
    } else {
        0
    };

    let filtered = app.filtered();

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

    // --- Repo list ---
    let visible: Vec<_> = filtered
        .iter()
        .enumerate()
        .skip(app.scroll_offset)
        .take(list_height)
        .collect();

    for (i, (idx, name)) in visible.iter().enumerate() {
        let row = (2 + i) as u16;
        let count = counts.get(name.as_str()).unwrap_or(&0);
        let count_str = if *count > 0 {
            format!(" ({})", count)
        } else {
            String::new()
        };
        queue!(w, cursor::MoveTo(0, row))?;
        if *idx == app.selection {
            queue!(
                w,
                SetAttribute(Attribute::Reverse),
                SetAttribute(Attribute::Bold),
                Print(format!("  {}", name)),
                SetAttribute(Attribute::Reset),
                SetForegroundColor(Color::DarkGrey),
                Print(&count_str),
                ResetColor,
            )?;
        } else {
            queue!(
                w,
                Print(format!("  {}", name)),
                SetForegroundColor(Color::DarkGrey),
                Print(&count_str),
                ResetColor,
            )?;
        }
    }

    // --- Footer ---
    let sort_label = match app.sort_mode {
        SortMode::Frequency => "freq",
        SortMode::Alphabetical => "a-z",
    };
    let footer = format!(
        "  ↑↓ navigate  enter cd  ^r report  ^e sort ({})  esc quit",
        sort_label
    );
    queue!(
        w,
        cursor::MoveTo(0, term_height - 1),
        SetForegroundColor(Color::DarkGrey),
        Print(&footer),
        ResetColor,
    )?;

    let cursor_col = 4 + app.query.len() as u16;
    queue!(w, cursor::MoveTo(cursor_col, 0))?;

    w.flush()
}

fn render_report<W: Write>(
    w: &mut W,
    repos: &[String],
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

    queue!(
        w,
        cursor::MoveTo(0, 0),
        SetAttribute(Attribute::Bold),
        Print("  Repo usage report"),
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

    let mut entries: Vec<(&String, u64)> = repos
        .iter()
        .map(|r| (r, *counts.get(r).unwrap_or(&0)))
        .filter(|(_, c)| *c > 0)
        .collect();
    entries.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));

    let max_count = entries.iter().map(|(_, c)| *c).max().unwrap_or(0);
    let max_name_len = entries.iter().map(|(n, _)| n.len()).max().unwrap_or(0);

    let count_width = if max_count > 0 {
        (max_count as f64).log10().floor() as usize + 1
    } else {
        1
    };
    let bar_budget = (term_width as usize).saturating_sub(2 + max_name_len + 2 + count_width + 2);

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

fn run() -> io::Result<Option<String>> {
    let base = repos_dir();
    let cfg = config_dir();

    if !base.exists() {
        eprintln!("eee: ~/github.com not found");
        process::exit(1);
    }

    if !cfg.exists() {
        fs::create_dir_all(&cfg)?;
    }

    let hist_path = history_path();
    let cnt_path = counts_path();
    let ulog_path = usage_log_path();
    let repos = discover_repos(&base);

    if repos.is_empty() {
        eprintln!("eee: no repos found in ~/github.com");
        process::exit(1);
    }

    let counts = load_counts(&cnt_path);
    let freq_counts = load_3day_counts(&ulog_path);

    let tty = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/tty")?;
    let mut tty_write = tty.try_clone()?;

    enable_raw_mode()?;
    execute!(tty_write, EnterAlternateScreen, cursor::Hide)?;

    let mut app = App::new(repos, freq_counts);
    let result: io::Result<Option<String>> = (|| loop {
        let (term_width, term_height) = terminal::size()?;

        match &app.mode {
            Mode::Report => {
                let counts = load_counts(&cnt_path);
                render_report(
                    &mut tty_write,
                    &app.repos,
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
                    Event::Key(KeyEvent {
                        code: KeyCode::Esc, ..
                    })
                    | Event::Key(KeyEvent {
                        code: KeyCode::Left,
                        ..
                    }) => {
                        app.mode = Mode::List;
                    }
                    Event::Key(KeyEvent {
                        code: KeyCode::Char('c'),
                        modifiers: KeyModifiers::CONTROL,
                        ..
                    }) => {
                        return Ok(None);
                    }
                    Event::Key(KeyEvent {
                        code: KeyCode::Up, ..
                    }) => {
                        if app.report_scroll > 0 {
                            app.report_scroll -= 1;
                        }
                    }
                    Event::Key(KeyEvent {
                        code: KeyCode::Down,
                        ..
                    }) => {
                        let total = app.repos.len();
                        if app.report_scroll + content_height < total {
                            app.report_scroll += 1;
                        }
                    }
                    Event::Resize(_, _) => {}
                    _ => {}
                }
            }
            Mode::List => {
                let list_height = if term_height > 3 {
                    (term_height - 3) as usize
                } else {
                    0
                };
                app.adjust_scroll(list_height);
                render_list(&mut tty_write, &app, &counts, term_width, term_height)?;

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
                            record_usage(&hist_path, &ulog_path, &name);
                            increment_count(&cnt_path, &name);
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
                        code: KeyCode::Char('r'),
                        modifiers: KeyModifiers::CONTROL,
                        ..
                    }) => {
                        app.mode = Mode::Report;
                        app.report_scroll = 0;
                    }
                    Event::Key(KeyEvent {
                        code: KeyCode::Char('e'),
                        modifiers: KeyModifiers::CONTROL,
                        ..
                    }) => {
                        app.toggle_sort();
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

    let _ = execute!(tty_write, LeaveAlternateScreen, cursor::Show);
    let _ = disable_raw_mode();

    result
}

fn main() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = execute!(io::stderr(), LeaveAlternateScreen, cursor::Show);
        let _ = disable_raw_mode();
        default_hook(info);
    }));

    match run() {
        Ok(Some(repo)) => {
            let base = repos_dir();
            let path = base.join(&repo);
            if let Ok(mut tty) = fs::OpenOptions::new().write(true).open("/dev/tty") {
                let msg = format!("\r\n  \u{2713} {}\r\n", repo);
                let _ = tty.write_all(msg.as_bytes());
            }
            println!("cd {}", path.display());
        }
        Ok(None) => {
            process::exit(0);
        }
        Err(e) => {
            eprintln!("eee: error: {}", e);
            process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repos(names: &[&str]) -> Vec<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn sort_repos_alphabetical_is_passthrough() {
        let r = repos(&["b/two", "a/one"]);
        let sorted = sort_repos(&r, &HashMap::new(), SortMode::Alphabetical);
        assert_eq!(sorted, r);
    }

    #[test]
    fn sort_repos_frequency_ranks_used_repos_first() {
        let r = repos(&["a/one", "b/two", "c/three"]);
        let mut counts = HashMap::new();
        counts.insert("c/three".to_string(), 5u64);
        counts.insert("a/one".to_string(), 2u64);
        let sorted = sort_repos(&r, &counts, SortMode::Frequency);
        assert_eq!(sorted, repos(&["c/three", "a/one", "b/two"]));
    }

    #[test]
    fn sort_repos_frequency_ties_break_alphabetically() {
        let r = repos(&["b/two", "a/one"]);
        let mut counts = HashMap::new();
        counts.insert("b/two".to_string(), 1u64);
        counts.insert("a/one".to_string(), 1u64);
        let sorted = sort_repos(&r, &counts, SortMode::Frequency);
        assert_eq!(sorted, repos(&["a/one", "b/two"]));
    }

    #[test]
    fn filter_repos_is_case_insensitive_substring() {
        let r = repos(&["org/Frontend", "org/backend"]);
        let result = filter_repos(&r, "FRONT");
        assert_eq!(result, vec![&"org/Frontend".to_string()]);
    }

    #[test]
    fn toggle_sort_flips_mode_and_resets_selection() {
        let mut app = App::new(repos(&["a/one", "b/two"]), HashMap::new());
        app.selection = 1;
        assert_eq!(app.sort_mode, SortMode::Frequency);
        app.toggle_sort();
        assert_eq!(app.sort_mode, SortMode::Alphabetical);
        assert_eq!(app.selection, 0);
    }

    #[test]
    fn clamp_selection_pulls_back_into_range_after_filter_shrinks() {
        let mut app = App::new(repos(&["a/one", "b/two"]), HashMap::new());
        app.selection = 1;
        app.query = "zzz".to_string();
        app.clamp_selection();
        assert_eq!(app.selection, 0);
    }
}
