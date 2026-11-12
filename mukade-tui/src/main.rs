//! A deliberately small native terminal browser for Mukade sessions.
//!
//! Left: every harness's standard store, then every installation a scan found (portable and
//! custom ones included), with their session counts. Directory filters precede the session list.
//! Right: the selected session's metadata, or its transcript after `Enter`.
//! `R` resumes in the session's own installation; `h` hops it to another and shows the manifest;
//! `e` exports it to an interchange document; `i` imports a document into an installation;
//! `o` scans a folder for more installations (`--scan <dir>` does it at start).

mod directory;
mod location;
#[cfg(test)]
mod scratch;
mod tree;

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use location::Location;
use mukade::catalog;
use mukade::dest;
use mukade::exe;
use mukade::hop::{
    Compaction, HopError, HopRequest, ImportOpts, KeepFrom, ResumePrompt, SessionRef, SourceFacts,
    SummarySource, WriteDetails,
};
use mukade::model::{HarnessId, Kind, Session};
use mukade::policy::ReasoningPolicy;
use mukade::reader::{self, SessionInfo};
use mukade::roots::StoreRoots;
use mukade::summary::SummarizeOpts;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::{Frame, Terminal};
use std::io::Stdout;
use std::process::Command;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::Duration;

type Tui = Terminal<CrosstermBackend<Stdout>>;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Focus {
    Harnesses,
    Directories,
    Sessions,
    Transcript,
}

/// One location's sessions, as the left and middle columns show them.
struct HarnessRow {
    location: Location,
    sessions: Vec<SessionInfo>,
    directories: directory::Directories,
    error: Option<String>,
}

impl HarnessRow {
    fn new(location: Location, sessions: Vec<SessionInfo>, error: Option<String>) -> Self {
        let directories = directory::Directories::new(&sessions);
        Self {
            location,
            sessions,
            directories,
            error,
        }
    }
}

struct Transcript {
    title: String,
    session: Session,
    lines: Vec<Line<'static>>,
    hidden_turns: usize,
}

enum Mode {
    Normal,
    Hop(Box<HopState>),
    Result(ResultView),
    /// The folder to scan for installations, as typed so far.
    Open(String),
    Import(Box<ImportState>),
}

struct ResultView {
    title: &'static str,
    lines: Vec<String>,
    scroll: u16,
}

/// The import dialog: a document file, the installation to write it into, an optional prompt.
struct ImportState {
    path: String,
    destinations: Vec<Location>,
    dest: usize,
    prompt: String,
    field: usize,
}

struct HopState {
    source: Location,
    source_id: String,
    source_title: String,
    source_cwd: Option<String>,
    /// Every other location, the same harness elsewhere included.
    destinations: Vec<Location>,
    /// The chosen one, an index into `destinations`.
    dest: usize,
    compaction: usize,
    /// Carry the source's subagent sessions too (`HopRequest::children`). Off until asked.
    children: bool,
    /// How many subagent sessions the source has in the listing, for the dialog's line.
    subagents: usize,
    /// Read and write every branch the source keeps (`HopRequest::all_branches`). Off until asked;
    /// offered only where the library reads the source's branches.
    all_branches: bool,
    prompt: String,
    /// The fields this dialog offers, in `tab` order, and the one that has the focus (an index).
    fields: Vec<HopField>,
    field: usize,
}

/// One field of the hop dialog. `Children` is offered only for a source that has subagents,
/// `AllBranches` only for a source whose branches the library reads.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum HopField {
    Destination,
    Compaction,
    Children,
    AllBranches,
    Prompt,
}

impl HopState {
    fn focus(&self) -> HopField {
        self.fields[self.field % self.fields.len()]
    }

    fn focused(&self, field: HopField) -> bool {
        self.focus() == field
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum JobKind {
    Scan,
    Read,
    Hop,
    Export,
    Import,
}

enum JobResult {
    Loaded {
        locations: Vec<Location>,
        rows: Vec<HarnessRow>,
        status: String,
    },
    Read {
        info: SessionInfo,
        fallback: String,
        result: Box<Result<Session, String>>,
    },
    Hopped(Vec<String>),
    Exported(Result<Exported, String>),
    Imported(Vec<String>),
}

/// A document `e` wrote.
struct Exported {
    path: std::path::PathBuf,
    messages: usize,
}

struct App {
    /// Where sessions are listed from: the standard stores, then scanned installations.
    locations: Vec<Location>,
    /// The standard stores, which a scanned installation keeps for the other harnesses.
    standard: StoreRoots,
    rows: Vec<HarnessRow>,
    harness: usize,
    directory: usize,
    session: usize,
    focus: Focus,
    transcript: Option<Transcript>,
    scroll: u16,
    status: String,
    mode: Mode,
    job: Option<Receiver<JobResult>>,
    job_kind: JobKind,
    busy: bool,
    viewport_width: u16,
    viewport_height: u16,
    turn_limit: usize,
    default_turns: usize,
    /// The last document `e` wrote: the import dialog starts with it.
    last_export: Option<String>,
}

impl App {
    fn new(
        locations: Vec<Location>,
        standard: StoreRoots,
        rows: Vec<HarnessRow>,
        turn_limit: usize,
    ) -> Self {
        Self {
            locations,
            standard,
            rows,
            harness: 0,
            directory: 0,
            session: 0,
            focus: Focus::Harnesses,
            transcript: None,
            scroll: 0,
            status: "ready".to_string(),
            mode: Mode::Normal,
            job: None,
            job_kind: JobKind::Scan,
            busy: false,
            viewport_width: 80,
            viewport_height: 20,
            turn_limit,
            default_turns: turn_limit,
            last_export: None,
        }
    }

    /// `None` until the first scan returns: the interactive explorer draws before it has rows.
    fn selected_row(&self) -> Option<&HarnessRow> {
        self.rows
            .get(self.harness.min(self.rows.len().saturating_sub(1)))
    }

    fn selected_session(&self) -> Option<&SessionInfo> {
        let row = self.selected_row()?;
        let index = *row.directories.sessions(self.directory).get(self.session)?;
        row.sessions.get(index)
    }

    fn visible_sessions(&self) -> &[usize] {
        self.selected_row()
            .map_or(&[], |row| row.directories.sessions(self.directory))
    }

    /// The selected session together with the location that holds it.
    fn selected_pair(&self) -> Option<(&Location, &SessionInfo)> {
        let row = self.selected_row()?;
        Some((&row.location, self.selected_session()?))
    }

    fn clear_session(&mut self) {
        self.session = 0;
        self.transcript = None;
        self.scroll = 0;
    }

    fn next_focus(&mut self) {
        self.focus = match self.focus {
            Focus::Harnesses => Focus::Directories,
            Focus::Directories => Focus::Sessions,
            Focus::Sessions => Focus::Harnesses,
            Focus::Transcript => Focus::Transcript,
        };
    }

    fn previous_focus(&mut self) {
        self.focus = match self.focus {
            Focus::Harnesses => Focus::Sessions,
            Focus::Directories => Focus::Harnesses,
            Focus::Sessions => Focus::Directories,
            Focus::Transcript => Focus::Transcript,
        };
    }

    fn back(&mut self) {
        if self.transcript.take().is_some() {
            self.scroll = 0;
            self.focus = Focus::Sessions;
        } else {
            self.focus = match self.focus {
                Focus::Sessions | Focus::Transcript => Focus::Directories,
                Focus::Directories | Focus::Harnesses => Focus::Harnesses,
            };
        }
    }

    fn move_down(&mut self) {
        match self.focus {
            Focus::Harnesses => {
                self.harness = (self.harness + 1).min(self.rows.len().saturating_sub(1));
                self.directory = 0;
                self.clear_session();
            }
            Focus::Directories => {
                let len = self.selected_row().map_or(1, |row| row.directories.len());
                self.directory = (self.directory + 1).min(len - 1);
                self.clear_session();
            }
            Focus::Sessions => {
                let len = self.visible_sessions().len();
                if len > 0 {
                    self.session = (self.session + 1).min(len - 1);
                }
                self.transcript = None;
            }
            Focus::Transcript => {
                self.scroll = self.scroll.saturating_add(1);
                self.clamp_scroll();
            }
        }
    }

    fn move_up(&mut self) {
        match self.focus {
            Focus::Harnesses => {
                self.harness = self.harness.saturating_sub(1);
                self.directory = 0;
                self.clear_session();
            }
            Focus::Directories => {
                self.directory = self.directory.saturating_sub(1);
                self.clear_session();
            }
            Focus::Sessions => {
                self.session = self.session.saturating_sub(1);
                self.transcript = None;
            }
            Focus::Transcript => {
                self.scroll = self.scroll.saturating_sub(1);
            }
        }
    }

    fn clamp_scroll(&mut self) {
        let Some(transcript) = &self.transcript else {
            self.scroll = 0;
            return;
        };
        let wrapped = Paragraph::new(transcript.lines.clone())
            .wrap(Wrap { trim: false })
            .line_count(self.viewport_width.max(1));
        let visible = usize::from(self.viewport_height.max(1));
        let max = wrapped.saturating_sub(visible);
        self.scroll = self.scroll.min(u16::try_from(max).unwrap_or(u16::MAX));
    }
}

fn main() -> std::io::Result<()> {
    let standard = resolve_roots();
    let mut locations = location::standard_locations(&standard);
    let args: Vec<String> = std::env::args().collect();
    let turn_limit = parse_turns(&args);
    let folders = parse_scan_folders(&args);
    let headless = args
        .iter()
        .any(|arg| matches!(arg.as_str(), "--check" | "--snapshot" | "--snapshot-hop"));
    let rows = if headless {
        if !folders.is_empty() {
            location::scan_folders(&mut locations, &folders, &standard);
        }
        load_rows(&locations)
    } else {
        Vec::new()
    };

    if args.iter().any(|arg| arg == "--check") {
        let width = rows
            .iter()
            .map(|row| row.location.label.chars().count())
            .max()
            .unwrap_or(0);
        for row in &rows {
            let label = &row.location.label;
            match &row.error {
                Some(error) => println!("{label:<width$}  error: {error}"),
                None => println!("{label:<width$}  {} sessions", row.sessions.len()),
            }
            if let Some(store) = &row.location.store {
                println!("{:<width$}  store  {}", "", store.display());
                println!(
                    "{:<width$}  engine {}",
                    "",
                    row.location
                        .exe
                        .as_deref()
                        .unwrap_or("(standard resolution)")
                );
            }
        }
        return Ok(());
    }

    let mut app = App::new(locations, standard, rows, turn_limit);

    if args.iter().any(|arg| arg == "--snapshot-hop") {
        open_hop(&mut app);
        snapshot(&mut app);
        return Ok(());
    }

    if args.iter().any(|arg| arg == "--snapshot") {
        snapshot(&mut app);
        return Ok(());
    }

    let _guard = TerminalGuard;
    install_panic_cleanup();
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout))?;

    let result = run(&mut terminal, &mut app, folders);
    restore_terminal(&mut terminal);
    result
}

/// `--scan <dir>` (or `--scan=<dir>`), any number of times: folders scanned for installations
/// before the first listing.
fn parse_scan_folders(args: &[String]) -> Vec<std::path::PathBuf> {
    let mut folders = Vec::new();
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        if arg == "--scan" {
            if let Some(folder) = args.get(index + 1) {
                folders.push(std::path::PathBuf::from(folder));
                index += 1;
            }
        } else if let Some(folder) = arg.strip_prefix("--scan=") {
            folders.push(std::path::PathBuf::from(folder));
        }
        index += 1;
    }
    folders
}

/// `--turns N` (or `--turns=N`): how many user turns the transcript opens with. `0` means all.
/// The default is 50, so a huge session does not build every line before the first frame.
fn parse_turns(args: &[String]) -> usize {
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        if arg == "--turns" {
            return args
                .get(index + 1)
                .and_then(|value| value.parse().ok())
                .unwrap_or(50);
        }
        if let Some(value) = arg.strip_prefix("--turns=") {
            return value.parse().unwrap_or(50);
        }
        index += 1;
    }
    50
}

struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        cleanup_terminal();
    }
}

fn cleanup_terminal() {
    let _ = disable_raw_mode();
    let mut stdout = std::io::stdout();
    let _ = execute!(stdout, LeaveAlternateScreen);
}

fn install_panic_cleanup() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        cleanup_terminal();
        previous(info);
    }));
}

fn restore_terminal(terminal: &mut Tui) {
    let _ = disable_raw_mode();
    let _ = execute!(terminal.backend_mut(), LeaveAlternateScreen);
    let _ = terminal.show_cursor();
}

fn snapshot(app: &mut App) {
    let backend = ratatui::backend::TestBackend::new(120, 40);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| draw(frame, app))
        .expect("render snapshot");
    let buffer = terminal.backend().buffer();
    for y in 0..buffer.area.height {
        let mut line = String::new();
        for x in 0..buffer.area.width {
            line.push_str(buffer.cell((x, y)).map(|cell| cell.symbol()).unwrap_or(" "));
        }
        println!("{}", line.trim_end());
    }
}

fn run(terminal: &mut Tui, app: &mut App, folders: Vec<std::path::PathBuf>) -> std::io::Result<()> {
    if app.rows.is_empty() {
        start_load(app, folders);
    }
    loop {
        poll_job(app);
        terminal.draw(|frame| draw(frame, app))?;
        if !event::poll(Duration::from_millis(100))? {
            continue;
        }
        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        if app.busy {
            if key.code == KeyCode::Char('q')
                && !matches!(app.job_kind, JobKind::Hop | JobKind::Import)
            {
                return Ok(());
            }
            continue;
        }
        match &mut app.mode {
            Mode::Hop(_) => {
                handle_hop_key(app, key.code);
                continue;
            }
            Mode::Open(_) => {
                handle_open_key(app, key.code);
                continue;
            }
            Mode::Import(_) => {
                handle_import_key(app, key.code);
                continue;
            }
            Mode::Result(view) => {
                match key.code {
                    KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q') => app.mode = Mode::Normal,
                    KeyCode::Down | KeyCode::Char('j') => {
                        view.scroll = view.scroll.saturating_add(1)
                    }
                    KeyCode::Up | KeyCode::Char('k') => view.scroll = view.scroll.saturating_sub(1),
                    KeyCode::PageDown => view.scroll = view.scroll.saturating_add(10),
                    KeyCode::PageUp => view.scroll = view.scroll.saturating_sub(10),
                    KeyCode::Char('g') => view.scroll = 0,
                    KeyCode::Char('G') => view.scroll = u16::MAX,
                    _ => {}
                }
                continue;
            }
            Mode::Normal => {}
        }

        match key.code {
            KeyCode::Char('q') => return Ok(()),
            KeyCode::Char('h') => open_hop(app),
            KeyCode::Char('e') => start_export(app),
            KeyCode::Char('i') => open_import(app),
            KeyCode::Char('R') => resume_selected(terminal, app)?,
            KeyCode::Char('o') => app.mode = Mode::Open(String::new()),
            KeyCode::Tab | KeyCode::Right => app.next_focus(),
            KeyCode::BackTab | KeyCode::Left => app.previous_focus(),
            KeyCode::Esc => app.back(),
            KeyCode::Enter => match app.focus {
                Focus::Harnesses => app.focus = Focus::Directories,
                Focus::Directories => app.focus = Focus::Sessions,
                Focus::Sessions => start_read(app),
                Focus::Transcript => {}
            },
            KeyCode::Down | KeyCode::Char('j') => app.move_down(),
            KeyCode::Up | KeyCode::Char('k') => app.move_up(),
            KeyCode::Char('g') => {
                if app.transcript.is_some() {
                    app.scroll = 0;
                    app.focus = Focus::Transcript;
                }
            }
            KeyCode::Char('G') => {
                if app.transcript.is_some() {
                    app.clamp_scroll();
                    app.scroll = u16::MAX;
                    app.clamp_scroll();
                    app.focus = Focus::Transcript;
                }
            }
            KeyCode::PageDown => {
                app.scroll = app.scroll.saturating_add(10);
                app.clamp_scroll();
            }
            KeyCode::PageUp => {
                app.scroll = app.scroll.saturating_sub(10);
            }
            KeyCode::Char('r') => start_load(app, Vec::new()),
            KeyCode::Char('t') => toggle_turns(app),
            _ => {}
        }
    }
}

/// List every location's sessions again, after scanning `folders` for more installations.
fn start_load(app: &mut App, folders: Vec<std::path::PathBuf>) {
    let mut locations = app.locations.clone();
    let standard = app.standard.clone();
    let scanning = !folders.is_empty();
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let status = if scanning {
            location::scan_folders(&mut locations, &folders, &standard)
        } else {
            "ready".to_string()
        };
        let rows = load_rows(&locations);
        let _ = sender.send(JobResult::Loaded {
            locations,
            rows,
            status,
        });
    });
    app.job = Some(receiver);
    app.job_kind = JobKind::Scan;
    app.busy = true;
    app.status = if scanning {
        "scanning for installations…".to_string()
    } else {
        "scanning stores…".to_string()
    };
}

fn start_read(app: &mut App) {
    if refuse_unreadable(app) {
        return;
    }
    let Some((location, info)) = app.selected_pair() else {
        app.status = "no session selected".to_string();
        return;
    };
    let harness = location.harness;
    let roots = location.roots.clone();
    let info = info.clone();
    let fallback = session_label(&info);
    let id = info.id.clone();
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let result = mukade::export(&SessionRef::id(harness, info.id.clone()), &roots)
            .map_err(|error| error.to_string());
        let _ = sender.send(JobResult::Read {
            info,
            fallback,
            result: Box::new(result),
        });
    });
    app.job = Some(receiver);
    app.job_kind = JobKind::Read;
    app.busy = true;
    app.status = format!("reading {id}…");
}

fn start_hop(app: &mut App) {
    let Mode::Hop(hop) = std::mem::replace(&mut app.mode, Mode::Normal) else {
        return;
    };
    let Some(label) = hop
        .destinations
        .get(hop.dest)
        .map(|dest| dest.label.clone())
    else {
        return;
    };
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let lines = execute_hop(&hop);
        let _ = sender.send(JobResult::Hopped(lines));
    });
    app.job = Some(receiver);
    app.job_kind = JobKind::Hop;
    app.busy = true;
    app.status = format!("hopping → {label}…");
}

fn toggle_turns(app: &mut App) {
    let Some(mut transcript) = app.transcript.take() else {
        return;
    };
    let next = if app.turn_limit == 0 {
        app.default_turns.max(1)
    } else {
        0
    };
    let (lines, hidden_turns) = transcript_lines(&transcript.session, next);
    transcript.lines = lines;
    transcript.hidden_turns = hidden_turns;
    app.transcript = Some(transcript);
    app.turn_limit = next;
    app.scroll = 0;
    app.clamp_scroll();
    app.status = if next == 0 {
        "full history".to_string()
    } else {
        format!("last {next} turns")
    };
}

fn poll_job(app: &mut App) {
    let outcome = {
        let Some(receiver) = &app.job else {
            return;
        };
        match receiver.try_recv() {
            Ok(result) => Some(result),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                app.job = None;
                app.busy = false;
                app.status = "job failed: worker stopped".to_string();
                return;
            }
        }
    };
    let Some(result) = outcome else {
        return;
    };
    app.job = None;
    app.busy = false;
    match result {
        JobResult::Loaded {
            locations,
            rows,
            status,
        } => {
            app.locations = locations;
            app.rows = rows;
            app.harness = app.harness.min(app.rows.len().saturating_sub(1));
            app.directory = 0;
            app.clear_session();
            app.focus = Focus::Harnesses;
            app.status = status;
        }
        JobResult::Read {
            info,
            fallback,
            result,
        } => match *result {
            Ok(session) => {
                let title = session
                    .meta
                    .title
                    .clone()
                    .filter(|title| !title.is_empty())
                    .unwrap_or_else(|| {
                        if fallback.is_empty() {
                            info.id.clone()
                        } else {
                            fallback
                        }
                    });
                let (lines, hidden_turns) = transcript_lines(&session, app.turn_limit);
                app.transcript = Some(Transcript {
                    title,
                    session,
                    lines,
                    hidden_turns,
                });
                app.scroll = 0;
                app.focus = Focus::Transcript;
                app.status = if hidden_turns > 0 {
                    format!("last {} turns", app.turn_limit)
                } else {
                    "transcript".to_string()
                };
            }
            Err(error) => {
                app.status = format!("read failed: {error}");
            }
        },
        JobResult::Hopped(lines) => {
            app.status = if lines.first().is_some_and(|line| {
                line.starts_with("hop failed") || line.starts_with("resume failed")
            }) {
                "hop failed".to_string()
            } else {
                "hop finished".to_string()
            };
            app.mode = Mode::Result(ResultView {
                title: " hop result ",
                lines,
                scroll: 0,
            });
        }
        JobResult::Exported(Ok(exported)) => {
            let path = exported.path.display().to_string();
            app.status = format!("exported → {path}");
            app.last_export = Some(path.clone());
            app.mode = Mode::Result(ResultView {
                title: " export result ",
                lines: vec![
                    format!("document     {path}"),
                    format!("messages     {}", exported.messages),
                    String::new(),
                    "press i to import it into another installation,".to_string(),
                    format!("or run: mukade write <harness> \"{path}\""),
                ],
                scroll: 0,
            });
        }
        JobResult::Exported(Err(error)) => {
            app.status = format!("export failed: {error}");
        }
        JobResult::Imported(lines) => {
            app.status = if lines.first().is_some_and(|line| {
                line.starts_with("import failed") || line.starts_with("resume failed")
            }) {
                "import failed".to_string()
            } else {
                "import finished".to_string()
            };
            app.mode = Mode::Result(ResultView {
                title: " import result ",
                lines,
                scroll: 0,
            });
        }
    }
}

/// `e`: write the selected session's interchange document to `mukade-data/exports/`.
fn start_export(app: &mut App) {
    if refuse_unreadable(app) {
        return;
    }
    let Some((location, info)) = app.selected_pair() else {
        app.status = "no session selected".to_string();
        return;
    };
    let Some(dir) = data_dir().map(|dir| dir.join("exports")) else {
        app.status = "export failed: no mukade-data folder beside the executable".to_string();
        return;
    };
    let location = location.clone();
    let id = info.id.clone();
    let (sender, receiver) = mpsc::channel();
    let shown = id.clone();
    std::thread::spawn(move || {
        let _ = sender.send(JobResult::Exported(export_session(&location, &id, &dir)));
    });
    app.job = Some(receiver);
    app.job_kind = JobKind::Export;
    app.busy = true;
    app.status = format!("exporting {shown}…");
}

/// Where `e` writes a session's document: `<dir>/<harness>-<id>.json`, the id made safe for a file
/// name.
fn export_file(dir: &std::path::Path, harness: HarnessId, id: &str) -> std::path::PathBuf {
    let id: String = id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect();
    dir.join(format!("{}-{id}.json", harness.as_str()))
}

/// Read a session and write its document (the same one `mukade dump` prints) into `dir`.
fn export_session(
    location: &Location,
    id: &str,
    dir: &std::path::Path,
) -> Result<Exported, String> {
    let session = mukade::export(&SessionRef::id(location.harness, id), &location.roots)
        .map_err(|error| error.to_string())?;
    let doc = mukade::hop::document(&session, None);
    let messages = doc
        .get("messages")
        .and_then(serde_json::Value::as_array)
        .map_or(0, Vec::len);
    std::fs::create_dir_all(dir).map_err(|error| format!("{}: {error}", dir.display()))?;
    let path = export_file(dir, location.harness, id);
    let text = serde_json::to_string_pretty(&doc).map_err(|error| error.to_string())?;
    std::fs::write(&path, text).map_err(|error| format!("{}: {error}", path.display()))?;
    Ok(Exported { path, messages })
}

/// `i`: the import dialog, over every location, starting with the last exported document.
fn open_import(app: &mut App) {
    let destinations: Vec<Location> = app.rows.iter().map(|row| row.location.clone()).collect();
    if destinations.is_empty() {
        app.status = "no installation to import into".to_string();
        return;
    }
    let dest = app.harness.min(destinations.len() - 1);
    app.mode = Mode::Import(Box::new(ImportState {
        path: app.last_export.clone().unwrap_or_default(),
        destinations,
        dest,
        prompt: String::new(),
        field: 0,
    }));
}

fn handle_import_key(app: &mut App, code: KeyCode) {
    let mut run = false;
    {
        let Mode::Import(state) = &mut app.mode else {
            return;
        };
        let count = state.destinations.len().max(1);
        match code {
            KeyCode::Esc => app.mode = Mode::Normal,
            KeyCode::Tab => state.field = (state.field + 1) % 3,
            KeyCode::BackTab => state.field = (state.field + 2) % 3,
            KeyCode::Up | KeyCode::Down if state.field == 1 => {
                state.dest = if code == KeyCode::Up {
                    (state.dest + count - 1) % count
                } else {
                    (state.dest + 1) % count
                };
            }
            KeyCode::Enter if state.field < 2 => state.field += 1,
            KeyCode::Enter => run = true,
            KeyCode::Backspace if state.field == 0 => {
                state.path.pop();
            }
            KeyCode::Backspace if state.field == 2 => {
                state.prompt.pop();
            }
            KeyCode::Char(character) if state.field == 0 => state.path.push(character),
            KeyCode::Char(character) if state.field == 2 => state.prompt.push(character),
            _ => {}
        }
    }
    if run {
        start_import(app);
    }
}

fn start_import(app: &mut App) {
    let Mode::Import(state) = std::mem::replace(&mut app.mode, Mode::Normal) else {
        return;
    };
    let path = state.path.trim().trim_matches('"').to_string();
    if path.is_empty() {
        app.status = "import: no document given".to_string();
        app.mode = Mode::Import(state);
        return;
    }
    let Some(dest) = state.destinations.get(state.dest).cloned() else {
        return;
    };
    let prompt = Some(state.prompt.trim().to_string()).filter(|prompt| !prompt.is_empty());
    let label = dest.label.clone();
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let lines = execute_import(std::path::Path::new(&path), &dest, prompt);
        let _ = sender.send(JobResult::Imported(lines));
    });
    app.job = Some(receiver);
    app.job_kind = JobKind::Import;
    app.busy = true;
    app.status = format!("importing → {label}…");
}

/// Read a document file and write it into `dest` as a new session, with the destination's own
/// version, protocol, model and project; the result lines are the receipt, as a hop shows it.
fn execute_import(path: &std::path::Path, dest: &Location, prompt: Option<String>) -> Vec<String> {
    let (doc, session) = match read_document_file(path) {
        Ok(read) => read,
        Err(error) => return vec![format!("import failed: {error}")],
    };
    let write = write_details_for(dest, session.meta.cwd.as_deref());
    match import_read(dest, &doc, &session, write, prompt) {
        Ok(receipt) => receipt_lines(&receipt, dest),
        Err(HopError::ResumeFailed { reason, receipt }) => {
            let mut lines = vec![format!("resume failed: {reason}"), String::new()];
            lines.extend(receipt_lines(&receipt, dest));
            lines
        }
        Err(error) => vec![format!("import failed: {error}")],
    }
}

/// A document file and the session it holds.
fn read_document_file(path: &std::path::Path) -> Result<(serde_json::Value, Session), String> {
    let text =
        std::fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let doc: serde_json::Value = serde_json::from_str(text.trim_start_matches('\u{feff}'))
        .map_err(|error| format!("{}: not a JSON document: {error}", path.display()))?;
    let session = mukade::hop::read_document(&doc).map_err(|error| error.to_string())?;
    Ok((doc, session))
}

/// The import itself, with the destination details already decided (`write`).
fn import_read(
    dest: &Location,
    doc: &serde_json::Value,
    session: &Session,
    write: WriteDetails,
    prompt: Option<String>,
) -> Result<mukade::Receipt, HopError> {
    let opts = ImportOpts {
        cwd: session.meta.cwd.clone(),
        roots: dest.roots.clone(),
        reasoning: ReasoningPolicy::default(),
        write,
        resume: prompt.map(ResumePrompt::new),
        source_facts: SourceFacts::of(session),
        compaction: "none".to_string(),
        summary: None,
        native_budget: None,
    };
    mukade::import_document(dest.harness, doc, session, &opts)
}

/// What a write into `dest` needs from the destination itself: its executable and version, its
/// protocol, the model its own configuration names, the shell it runs and the tools it offers in
/// `cwd`, and for OpenCode the project of `cwd`.
fn write_details_for(dest: &Location, cwd: Option<&str>) -> WriteDetails {
    let exe = dest.engine();
    let configured = dest::configured_model_in(dest.harness, &dest.roots);
    let project = cwd.map(std::path::Path::new);
    let project_id = cwd.and_then(|cwd| {
        mukade::harnesses::get(dest.harness)
            .destination()?
            .project_id(&exe, cwd, &dest.roots, mukade::cache::dir().as_deref())
            .ok()
            .flatten()
    });
    WriteDetails {
        dest_version: exe::version_of(&exe),
        dest_protocol: mukade::policy::destination_protocol(dest.harness),
        dest_shell: dest::shell_in(dest.harness, &dest.roots, project),
        offered: Some(mukade::tools::offered_in(
            dest.harness,
            &dest.roots,
            project,
        )),
        model_provider: configured.as_ref().map(|(provider, _)| provider.clone()),
        model: configured.and_then(|(_, model)| model),
        project_id,
        exe: Some(exe),
        ..Default::default()
    }
}

/// The `mukade-data` folder beside the executable (or `MUKADE_DATA_DIR`).
fn data_dir() -> Option<std::path::PathBuf> {
    Some(mukade::cache::dir()?.parent()?.to_path_buf())
}

fn open_hop(app: &mut App) {
    if refuse_unreadable(app) {
        return;
    }
    let Some((source, info)) = app.selected_pair() else {
        app.status = "no session selected".to_string();
        return;
    };
    let subagents = app.selected_row().map_or(0, |row| {
        tree::children_of(&row.sessions, &info.id).subagents
    });
    let rows: Vec<Location> = app.rows.iter().map(|row| row.location.clone()).collect();
    let destinations = location::destinations(&rows, app.harness);
    if destinations.is_empty() {
        return;
    }
    app.mode = Mode::Hop(Box::new(HopState {
        source: source.clone(),
        source_id: info.id.clone(),
        source_title: session_label(info),
        source_cwd: info.cwd.clone(),
        destinations,
        dest: 0,
        compaction: 0,
        children: false,
        subagents,
        all_branches: false,
        prompt: String::new(),
        fields: [
            Some(HopField::Destination),
            Some(HopField::Compaction),
            (subagents > 0).then_some(HopField::Children),
            mukade::hop::reads_all_branches(source.harness).then_some(HopField::AllBranches),
            Some(HopField::Prompt),
        ]
        .into_iter()
        .flatten()
        .collect(),
        field: 0,
    }));
}

fn handle_hop_key(app: &mut App, code: KeyCode) {
    let mut run = false;
    {
        let Mode::Hop(hop) = &mut app.mode else {
            return;
        };
        let count = hop.destinations.len().max(1);
        let fields = hop.fields.len();
        match code {
            KeyCode::Esc => app.mode = Mode::Normal,
            KeyCode::Tab => hop.field = (hop.field + 1) % fields,
            KeyCode::BackTab => hop.field = (hop.field + fields - 1) % fields,
            KeyCode::Up | KeyCode::Down | KeyCode::Left | KeyCode::Right => match hop.focus() {
                HopField::Destination => {
                    hop.dest = if matches!(code, KeyCode::Up | KeyCode::Left) {
                        (hop.dest + count - 1) % count
                    } else {
                        (hop.dest + 1) % count
                    };
                }
                HopField::Compaction => {
                    hop.compaction = if matches!(code, KeyCode::Up | KeyCode::Left) {
                        (hop.compaction + 2) % 3
                    } else {
                        (hop.compaction + 1) % 3
                    };
                }
                HopField::Children => hop.children = !hop.children,
                HopField::AllBranches => hop.all_branches = !hop.all_branches,
                HopField::Prompt => {}
            },
            KeyCode::Char(' ') if hop.focused(HopField::Children) => hop.children = !hop.children,
            KeyCode::Char(' ') if hop.focused(HopField::AllBranches) => {
                hop.all_branches = !hop.all_branches
            }
            KeyCode::Enter if !hop.focused(HopField::Prompt) => hop.field += 1,
            KeyCode::Enter => run = true,
            KeyCode::Backspace if hop.focused(HopField::Prompt) => {
                hop.prompt.pop();
            }
            KeyCode::Char(character) if hop.focused(HopField::Prompt) => {
                hop.prompt.push(character);
            }
            _ => {}
        }
    }
    if run {
        start_hop(app);
    }
}

/// The folder prompt `o` opens: Enter scans the folder for installations and lists them.
fn handle_open_key(app: &mut App, code: KeyCode) {
    let Mode::Open(path) = &mut app.mode else {
        return;
    };
    match code {
        KeyCode::Esc => app.mode = Mode::Normal,
        KeyCode::Backspace => {
            path.pop();
        }
        KeyCode::Char(character) => path.push(character),
        KeyCode::Enter => {
            let folder = path.trim().trim_matches('"').to_string();
            app.mode = Mode::Normal;
            if folder.is_empty() {
                app.status = "no folder given".to_string();
            } else {
                start_load(app, vec![std::path::PathBuf::from(folder)]);
            }
        }
        _ => {}
    }
}

fn compaction_for(index: usize) -> Compaction {
    match index {
        1 => Compaction::Portable {
            summary: SummarySource::SourceRecord,
            keep_from: KeepFrom::SourceBoundary,
        },
        2 => Compaction::Portable {
            summary: SummarySource::SourceModel(SummarizeOpts::default()),
            keep_from: KeepFrom::LastTurns(0),
        },
        _ => Compaction::None,
    }
}

/// Read the source from its location, write it into the destination's: two installations of one
/// harness keep their stores apart (`mukade::hop_into`).
fn execute_hop(hop: &HopState) -> Vec<String> {
    execute_hop_with(hop, write_details_for)
}

/// [`execute_hop`] with the destination's facts taken from `details`, so a test needs no
/// installation to ask.
fn execute_hop_with(
    hop: &HopState,
    details: impl Fn(&Location, Option<&str>) -> WriteDetails,
) -> Vec<String> {
    let Some(dest) = hop.destinations.get(hop.dest) else {
        return vec!["hop failed: no destination".to_string()];
    };
    let compaction = compaction_for(hop.compaction);
    let source = SessionRef::id(hop.source.harness, hop.source_id.clone());
    let cwd = match hop.source_cwd.clone() {
        Some(cwd) => Some(cwd),
        None => match mukade::export(&source, &hop.source.roots) {
            Ok(session) => session.meta.cwd,
            Err(error) => return vec![format!("hop failed: {error}")],
        },
    };
    let write = details(dest, cwd.as_deref());
    let request = HopRequest {
        cwd,
        compaction,
        resume: (!hop.prompt.trim().is_empty())
            .then(|| ResumePrompt::new(hop.prompt.trim().to_string())),
        write,
        children: hop.children && hop.subagents > 0,
        all_branches: hop.all_branches && hop.fields.contains(&HopField::AllBranches),
        ..HopRequest::new(source, dest.harness, hop.source.roots.clone())
    };
    match mukade::hop_into(&request, &dest.roots) {
        Ok(receipt) => receipt_lines(&receipt, dest),
        Err(HopError::ResumeFailed { reason, receipt }) => {
            let mut lines = vec![format!("resume failed: {reason}"), String::new()];
            lines.extend(receipt_lines(&receipt, dest));
            lines
        }
        Err(HopError::ChildWriteFailed { reason, receipt }) => {
            let mut lines = vec![
                format!("a subagent session was not written: {reason}"),
                "the session itself was written".to_string(),
                String::new(),
            ];
            lines.extend(receipt_lines(&receipt, dest));
            lines
        }
        Err(error) => vec![format!("hop failed: {error}")],
    }
}

fn receipt_lines(receipt: &mukade::Receipt, dest: &Location) -> Vec<String> {
    let manifest = save_manifest(receipt, dest.harness);
    receipt_lines_with(receipt, dest, manifest)
}

fn receipt_lines_with(
    receipt: &mukade::Receipt,
    dest: &Location,
    manifest: Option<String>,
) -> Vec<String> {
    let info = SessionInfo {
        id: receipt.dest_id.clone(),
        path: receipt.location.clone(),
        ..Default::default()
    };
    // A harness with no interface to open a session in says so, with what continues it instead.
    let interactive = match mukade::harnesses::get(dest.harness).resume_refusal(&info) {
        Some(refusal) => format!("none: {refusal}"),
        None => resume_argv(dest.harness, &dest.engine(), &info).join(" "),
    };
    let mut lines = vec![
        format!("destination  {} ({})", receipt.dest_id, dest.label),
        format!("location     {}", receipt.location),
        format!("headless     {}", receipt.resume_command.join(" ")),
        format!("interactive  {interactive}"),
    ];
    if let Some(path) = manifest {
        lines.push(format!("manifest     {path}"));
    } else {
        lines.push("manifest     (not saved)".to_string());
    }
    let losses = receipt
        .manifest
        .get("losses")
        .and_then(serde_json::Value::as_array);
    match losses {
        Some(losses) if losses.is_empty() => lines.push("losses       none".to_string()),
        Some(losses) => {
            lines.push(format!("losses       {}", losses.len()));
            for loss in losses {
                let text = loss
                    .as_str()
                    .map(str::to_string)
                    .unwrap_or_else(|| serde_json::to_string(loss).unwrap_or_default());
                lines.push(format!("  · {text}"));
            }
        }
        None => lines.push("losses       (not in manifest)".to_string()),
    }
    lines.extend(branch_line(&receipt.manifest));
    lines.extend(child_lines(&receipt.manifest));
    if let Some(reply) = &receipt.resume_output {
        lines.push(String::new());
        lines.push("resume reply".to_string());
        lines.extend(reply.lines().map(str::to_string));
    }
    lines
}

/// How many branches of the source the hop wrote and left out (the manifest's `counts`). Nothing
/// when the source carried none.
fn branch_line(manifest: &serde_json::Value) -> Option<String> {
    let count = |key: &str| {
        manifest
            .pointer(&format!("/counts/{key}"))
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0)
    };
    let (kept, dropped) = (count("branches_kept"), count("branches_dropped"));
    (kept + dropped > 0).then(|| format!("branches     {kept} kept, {dropped} left out"))
}

/// What became of each subagent session a hop carried (the manifest's `children`): where it went
/// and whether the destination linked it to its parent. Nothing when none was carried.
fn child_lines(manifest: &serde_json::Value) -> Vec<String> {
    let children = manifest
        .get("children")
        .and_then(serde_json::Value::as_array)
        .filter(|children| !children.is_empty());
    let Some(children) = children else {
        return Vec::new();
    };
    let mut lines = vec![format!("children     {}", children.len())];
    for child in children {
        let id = |key: &str| child.get(key).and_then(serde_json::Value::as_str);
        let link = match id("link") {
            Some("native") => "linked to its parent",
            _ => "not linked: the destination has no link",
        };
        lines.push(format!(
            "  · {} -> {} ({link})",
            id("source_id").unwrap_or("?"),
            id("dest_id").unwrap_or("?"),
        ));
    }
    lines
}

fn save_manifest(receipt: &mukade::Receipt, dest: HarnessId) -> Option<String> {
    let dir = data_dir()?.join("manifests");
    std::fs::create_dir_all(&dir).ok()?;
    let name = format!("{}-{}.json", dest.as_str(), receipt.dest_id);
    let path = dir.join(name.replace(['/', '\\'], "_"));
    let bytes = serde_json::to_vec_pretty(&receipt.manifest).ok()?;
    std::fs::write(&path, bytes).ok()?;
    Some(path.display().to_string())
}

/// The command that opens a session in its harness's own terminal UI, with `exe`.
fn resume_argv(harness: HarnessId, exe: &str, info: &SessionInfo) -> Vec<String> {
    catalog::interactive_resume(
        harness,
        exe,
        &info.id,
        Some(std::path::Path::new(&info.path)),
    )
}

fn resume_selected(terminal: &mut Tui, app: &mut App) -> std::io::Result<()> {
    if refuse_unreadable(app) {
        return Ok(());
    }
    let (location, info) = {
        let Some((location, info)) = app.selected_pair() else {
            app.status = "no session selected".to_string();
            return Ok(());
        };
        (location.clone(), info.clone())
    };
    let harness = location.harness;
    if let Some(refusal) = mukade::harnesses::get(harness).resume_refusal(&info) {
        app.status = refusal;
        return Ok(());
    }
    let argv = resume_argv(harness, &location.engine(), &info);
    let cwd = info.cwd.clone().or_else(|| {
        mukade::export(&SessionRef::id(harness, info.id.clone()), &location.roots)
            .ok()
            .and_then(|session| session.meta.cwd)
    });

    app.status = format!("resume → {}", argv.join(" "));
    terminal.draw(|frame| draw(frame, app))?;

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    let mut command = Command::new(&argv[0]);
    command.args(&argv[1..]);
    if let Some(cwd) = &cwd {
        command.current_dir(cwd);
    }
    location.roots.apply_to(harness, &mut command);
    let status = command.status();

    enable_raw_mode()?;
    execute!(terminal.backend_mut(), EnterAlternateScreen)?;
    terminal.clear()?;
    app.status = match status {
        Ok(status) => format!("resume exited: {status}"),
        Err(error) => format!("resume failed: {error}"),
    };
    Ok(())
}

fn draw(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(3),
            Constraint::Length(2),
        ])
        .split(area);

    draw_header(frame, chunks[0], app);
    draw_body(frame, chunks[1], app);
    draw_footer(frame, chunks[2], app);

    match &mut app.mode {
        Mode::Hop(hop) => draw_hop(frame, area, hop),
        Mode::Result(view) => draw_result(frame, area, view),
        Mode::Open(path) => draw_open(frame, area, path),
        Mode::Import(state) => draw_import(frame, area, state),
        Mode::Normal => {}
    }
}

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width.saturating_sub(2)).max(20);
    let height = height.min(area.height.saturating_sub(2)).max(6);
    Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    }
}

fn draw_hop(frame: &mut Frame, area: Rect, hop: &HopState) {
    let rect = centered(area, 78, if hop.subagents > 0 { 19 } else { 17 });
    frame.render_widget(Clear, rect);
    let mut lines = vec![
        Line::from(vec![
            Span::styled("source  ", Style::default().fg(Color::DarkGray)),
            Span::raw(hop.source_title.clone()),
        ]),
        Line::from(""),
    ];
    for (index, destination) in hop.destinations.iter().enumerate() {
        let selected = index == hop.dest;
        let focus = hop.focused(HopField::Destination) && selected;
        lines.push(Line::from(vec![
            Span::styled(
                if focus { "▶ " } else { "  " },
                Style::default().fg(Color::Green),
            ),
            Span::styled(
                format!("{:<28}", destination.label),
                if selected {
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                },
            ),
            Span::styled(
                format!("destination {}", index + 1),
                Style::default().fg(Color::DarkGray),
            ),
        ]));
    }
    lines.push(Line::from(""));
    let compaction = ["none", "portable", "summary"][hop.compaction];
    lines.push(Line::from(vec![
        Span::styled(
            if hop.focused(HopField::Compaction) {
                "▶ "
            } else {
                "  "
            },
            Style::default().fg(Color::Green),
        ),
        Span::styled("compaction  ", Style::default().fg(Color::DarkGray)),
        Span::styled(compaction, Style::default().fg(Color::Yellow)),
    ]));
    if hop.subagents > 0 {
        lines.push(Line::from(vec![
            Span::styled(
                if hop.focused(HopField::Children) {
                    "▶ "
                } else {
                    "  "
                },
                Style::default().fg(Color::Green),
            ),
            Span::styled("subagents   ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                if hop.children {
                    "carry"
                } else {
                    "leave behind"
                },
                Style::default().fg(Color::Yellow),
            ),
            Span::styled(
                format!(
                    "  ({} in the source, written as sessions of their own)",
                    hop.subagents
                ),
                Style::default().fg(Color::DarkGray),
            ),
        ]));
    }
    if hop.fields.contains(&HopField::AllBranches) {
        lines.push(Line::from(vec![
            Span::styled(
                if hop.focused(HopField::AllBranches) {
                    "▶ "
                } else {
                    "  "
                },
                Style::default().fg(Color::Green),
            ),
            Span::styled("branches    ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                if hop.all_branches {
                    "every branch"
                } else {
                    "the resumed one"
                },
                Style::default().fg(Color::Yellow),
            ),
            Span::styled(
                "  (abandoned ones too, where the destination keeps them)",
                Style::default().fg(Color::DarkGray),
            ),
        ]));
    }
    lines.push(Line::from(vec![
        Span::styled(
            if hop.focused(HopField::Prompt) {
                "▶ "
            } else {
                "  "
            },
            Style::default().fg(Color::Green),
        ),
        Span::styled("prompt      ", Style::default().fg(Color::DarkGray)),
        Span::raw(if hop.prompt.is_empty() {
            "(empty: write only)".to_string()
        } else {
            format!("{}▌", hop.prompt)
        }),
    ]));
    lines.push(Line::from(Span::styled(
        format!(
            "cwd         {}",
            hop.source_cwd.as_deref().unwrap_or("session's own cwd")
        ),
        Style::default().fg(Color::DarkGray),
    )));
    if hop.compaction == 2 {
        lines.push(Line::from(Span::styled(
            "summary asks the source model and may take minutes",
            Style::default().fg(Color::Yellow),
        )));
        if hop.all_branches {
            lines.push(Line::from(Span::styled(
                "summary stands for the resumed branch: not with every branch",
                Style::default().fg(Color::Yellow),
            )));
        }
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "tab field · ↑/↓ change (space toggles) · enter next/run · esc cancel",
        Style::default().fg(Color::DarkGray),
    )));
    let block = Block::default().borders(Borders::ALL).title(" hop ");
    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false }),
        rect,
    );
}

/// The import dialog: document, destination, prompt; one field at a time is active.
fn draw_import(frame: &mut Frame, area: Rect, state: &ImportState) {
    let rect = centered(area, 78, 12 + state.destinations.len() as u16);
    frame.render_widget(Clear, rect);
    let marker = |field: usize| if state.field == field { "▶ " } else { "  " };
    let green = Style::default().fg(Color::Green);
    let gray = Style::default().fg(Color::DarkGray);
    let mut lines = vec![
        Line::from(vec![
            Span::styled(marker(0), green),
            Span::styled("1 document  ", gray),
            Span::raw(if state.path.is_empty() {
                "(type the path of a .json document)".to_string()
            } else if state.field == 0 {
                format!("{}▌", state.path)
            } else {
                state.path.clone()
            }),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled(marker(1), green),
            Span::styled("2 into", gray),
        ]),
    ];
    for (index, destination) in state.destinations.iter().enumerate() {
        let selected = index == state.dest;
        lines.push(Line::from(vec![
            Span::raw("    "),
            Span::styled(if selected { "● " } else { "○ " }, green),
            Span::styled(
                destination.label.clone(),
                if selected {
                    green.add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                },
            ),
        ]));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled(marker(2), green),
        Span::styled("3 prompt    ", gray),
        Span::raw(if state.prompt.is_empty() {
            "(empty: write only)".to_string()
        } else {
            format!("{}▌", state.prompt)
        }),
    ]));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "tab field · ↑/↓ installation · enter next/run · esc cancel",
        gray,
    )));
    let block = Block::default().borders(Borders::ALL).title(" import ");
    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false }),
        rect,
    );
}

/// The folder prompt: a path, scanned for installations on Enter.
fn draw_open(frame: &mut Frame, area: Rect, path: &str) {
    let rect = centered(area, 78, 8);
    frame.render_widget(Clear, rect);
    let lines = vec![
        Line::from(Span::styled(
            "folder to scan for harness installations (portable ones included)",
            Style::default().fg(Color::DarkGray),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("▶ ", Style::default().fg(Color::Green)),
            Span::raw(format!("{path}▌")),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "enter scan · esc cancel",
            Style::default().fg(Color::DarkGray),
        )),
    ];
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" open folder ");
    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false }),
        rect,
    );
}

fn draw_result(frame: &mut Frame, area: Rect, view: &mut ResultView) {
    let rect = centered(area, 100, 24);
    frame.render_widget(Clear, rect);
    let mut lines: Vec<Line> = view
        .lines
        .iter()
        .map(|line| Line::from(line.clone()))
        .collect();
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "j/k scroll · esc closes",
        Style::default().fg(Color::DarkGray),
    )));
    let block = Block::default().borders(Borders::ALL).title(view.title);
    let paragraph = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false });
    let wrapped = paragraph.line_count(rect.width.saturating_sub(2).max(1));
    let max = wrapped.saturating_sub(usize::from(rect.height.saturating_sub(2).max(1)));
    view.scroll = view.scroll.min(u16::try_from(max).unwrap_or(u16::MAX));
    frame.render_widget(paragraph.scroll((view.scroll, 0)), rect);
}

fn draw_header(frame: &mut Frame, area: Rect, app: &App) {
    let total: usize = app.rows.iter().map(|r| r.sessions.len()).sum();
    let line = Line::from(vec![
        Span::styled(
            " mukade ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!(
            "  {total} sessions · {} harnesses · ",
            app.rows.len()
        )),
        Span::styled(app.status.clone(), Style::default().fg(Color::DarkGray)),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

fn draw_body(frame: &mut Frame, area: Rect, app: &mut App) {
    if area.width < 120 && app.transcript.is_some() {
        draw_detail(frame, area, app);
        return;
    }
    if area.width < 70 {
        match app.focus {
            Focus::Harnesses => draw_harnesses(frame, area, app),
            Focus::Directories => draw_directories(frame, area, app),
            Focus::Sessions => draw_sessions(frame, area, app),
            Focus::Transcript => draw_detail(frame, area, app),
        }
        return;
    }
    let wide = area.width >= 120;
    let constraints = if wide {
        vec![
            Constraint::Length(22),
            Constraint::Length(28),
            Constraint::Length(34),
            Constraint::Min(30),
        ]
    } else {
        vec![
            Constraint::Length(18),
            Constraint::Length(28),
            Constraint::Min(24),
        ]
    };
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(area);

    draw_harnesses(frame, chunks[0], app);
    draw_directories(frame, chunks[1], app);
    draw_sessions(frame, chunks[2], app);
    if wide {
        draw_detail(frame, chunks[3], app);
    }
}

fn draw_directories(frame: &mut Frame, area: Rect, app: &App) {
    let mut items = Vec::new();
    if let Some(row) = app.selected_row() {
        items.push(ListItem::new(format!(
            "All directories  {}",
            row.sessions.len()
        )));
        for (index, group) in row.directories.groups().iter().enumerate() {
            let count = row.directories.sessions(index + 1).len();
            let label = group
                .cwd
                .as_deref()
                .map_or("Unknown directory", directory::name);
            let count_text = format!("  {count}");
            let label_width = usize::from(area.width.saturating_sub(2))
                .saturating_sub(Span::raw(&count_text).width());
            let mut lines = vec![Line::from(vec![
                Span::raw(truncate_cells(label, label_width)),
                Span::styled(count_text, Style::default().fg(Color::DarkGray)),
            ])];
            if let Some(cwd) = &group.cwd {
                let width = usize::from(area.width.saturating_sub(2)).max(1);
                let mut path_lines = if app.directory == index + 1 {
                    wrap_cells(cwd, width)
                } else {
                    vec![truncate_cells(&group.display_path, width)]
                };
                // Ratatui hides an entire list item if it is taller than the viewport.
                let max_lines = usize::from(area.height.saturating_sub(3));
                if path_lines.len() > max_lines {
                    path_lines.truncate(max_lines);
                    if let Some(last) = path_lines.last_mut() {
                        let mut start = cwd.len();
                        let mut used = 1;
                        for (index, ch) in cwd.char_indices().rev() {
                            let cells = Span::raw(ch.to_string()).width();
                            if used + cells > width {
                                break;
                            }
                            used += cells;
                            start = index;
                        }
                        *last = format!("…{}", &cwd[start..]);
                    }
                }
                lines.extend(path_lines.into_iter().map(|line| {
                    Line::from(Span::styled(line, Style::default().fg(Color::DarkGray)))
                }));
            }
            items.push(ListItem::new(lines));
        }
    }
    let mut state = ListState::default();
    if !items.is_empty() {
        state.select(Some(app.directory));
    }
    let highlight = if app.focus == Focus::Directories {
        Style::default().bg(Color::DarkGray).fg(Color::White)
    } else {
        Style::default().fg(Color::White)
    };
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" directories "),
        )
        .highlight_style(highlight);
    frame.render_stateful_widget(list, area, &mut state);
}

fn draw_harnesses(frame: &mut Frame, area: Rect, app: &App) {
    let items: Vec<ListItem> = app
        .rows
        .iter()
        .map(|row| {
            let (mark, color) = if row.error.is_some() {
                ("!", Color::Red)
            } else {
                ("●", Color::Green)
            };
            ListItem::new(Line::from(vec![
                Span::styled(format!("{mark} "), Style::default().fg(color)),
                Span::styled(
                    row.location.label.clone(),
                    Style::default().fg(Color::White),
                ),
                Span::raw(format!("  {}", row.sessions.len())),
            ]))
        })
        .collect();
    let mut state = ListState::default();
    state.select(Some(app.harness));
    let highlight = if app.focus == Focus::Harnesses {
        Style::default().bg(Color::DarkGray).fg(Color::White)
    } else {
        Style::default().fg(Color::White)
    };
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(" harnesses "))
        .highlight_style(highlight);
    frame.render_stateful_widget(list, area, &mut state);
}

fn draw_sessions(frame: &mut Frame, area: Rect, app: &App) {
    let Some(row) = app.selected_row() else {
        let block = Block::default().borders(Borders::ALL).title(" sessions ");
        frame.render_widget(block, area);
        return;
    };
    let visible = app.visible_sessions();
    let depths = row.directories.depths(app.directory);
    let title = format!(" sessions · {} ", visible.len());
    let items: Vec<ListItem> = visible
        .iter()
        .zip(depths)
        .map(|(&index, &depth)| (&row.sessions[index], depth))
        .map(|(info, depth)| {
            let title = session_label(info);
            let when = info.updated.as_deref().unwrap_or("—");
            let archived = if info.archived { " [archived]" } else { "" };
            let unreadable = if info.unreadable.is_some() {
                " [unreadable]"
            } else {
                ""
            };
            let live = if info.live == Some(true) {
                " [live]"
            } else {
                ""
            };
            let lead = tree::prefix(info, depth);
            ListItem::new(Line::from(vec![
                Span::styled(format!("{lead}{title}"), Style::default().fg(Color::White)),
                Span::styled(
                    format!("{live}{unreadable}{archived}  {when}"),
                    Style::default().fg(Color::DarkGray),
                ),
            ]))
        })
        .collect();
    let mut state = ListState::default();
    if !visible.is_empty() {
        state.select(Some(app.session));
    }
    let highlight = if app.focus == Focus::Sessions {
        Style::default().bg(Color::DarkGray).fg(Color::White)
    } else {
        Style::default().fg(Color::White)
    };
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(title))
        .highlight_style(highlight);
    frame.render_stateful_widget(list, area, &mut state);
}

fn draw_detail(frame: &mut Frame, area: Rect, app: &mut App) {
    app.viewport_width = area.width.saturating_sub(2).max(1);
    app.viewport_height = area.height.saturating_sub(2).max(1);
    if let Some(transcript) = &app.transcript {
        let title = if transcript.hidden_turns > 0 {
            format!(
                " transcript · {} · last {} turns ",
                transcript.title, app.turn_limit
            )
        } else {
            format!(" transcript · {} ", transcript.title)
        };
        let paragraph = Paragraph::new(transcript.lines.clone())
            .block(Block::default().borders(Borders::ALL).title(title))
            .wrap(Wrap { trim: false })
            .scroll((app.scroll, 0));
        frame.render_widget(paragraph, area);
        return;
    }

    let mut lines: Vec<Line> = Vec::new();
    let Some(row) = app.selected_row() else {
        lines.push(Line::from(Span::styled(
            app.status.clone(),
            Style::default().fg(Color::DarkGray),
        )));
        let paragraph = Paragraph::new(lines)
            .block(Block::default().borders(Borders::ALL).title(" session "))
            .wrap(Wrap { trim: true });
        frame.render_widget(paragraph, area);
        return;
    };
    if let Some(error) = &row.error {
        lines.push(Line::from(Span::styled(
            error.clone(),
            Style::default().fg(Color::Red),
        )));
    } else if let Some(info) = app.selected_session() {
        lines.push(Line::from(Span::styled(
            session_label(info),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(""));
        field(&mut lines, "harness", row.location.harness.as_str());
        field(&mut lines, "id", &info.id);
        if let Some(parent) = &info.parent {
            let listed = row.sessions.iter().any(|other| &other.id == parent);
            field(
                &mut lines,
                "parent",
                &if listed {
                    parent.clone()
                } else {
                    format!("{parent} (not listed)")
                },
            );
        }
        if let Some(kind) = tree::kind_text(info) {
            field(&mut lines, "kind", &kind);
        }
        if let Some(children) = tree::children_of(&row.sessions, &info.id).text() {
            field(&mut lines, "children", &children);
        }
        if let Some(cwd) = &info.cwd {
            field(&mut lines, "cwd", cwd);
        }
        if let Some(updated) = &info.updated {
            field(&mut lines, "updated", updated);
        }
        field(
            &mut lines,
            "archived",
            if info.archived { "yes" } else { "no" },
        );
        if let Some(live) = info.live {
            field(&mut lines, "live", if live { "yes" } else { "no" });
        }
        if let Some(why) = &info.unreadable {
            field(&mut lines, "unreadable", why);
        }
        field(&mut lines, "path", &info.path);
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Enter opens the transcript · r reloads · q quits",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        lines.push(Line::from(Span::styled(
            "no session selected",
            Style::default().fg(Color::DarkGray),
        )));
    }

    if let Some(store) = &row.location.store {
        lines.push(Line::from(""));
        field(&mut lines, "store", &store.display().to_string());
        field(
            &mut lines,
            "engine",
            row.location.exe.as_deref().unwrap_or("standard resolution"),
        );
    }

    let paragraph = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title(" session "))
        .wrap(Wrap { trim: true });
    frame.render_widget(paragraph, area);
}

fn draw_footer(frame: &mut Frame, area: Rect, app: &App) {
    let focus = match app.focus {
        Focus::Harnesses => "harnesses",
        Focus::Directories => "directories",
        Focus::Sessions => "sessions",
        Focus::Transcript => "transcript",
    };
    let navigation = Line::from(vec![
        Span::styled(
            format!(" {focus} "),
            Style::default().fg(Color::Black).bg(Color::Gray),
        ),
        Span::raw("  j/k move · tab/←/→ column · enter open · esc back · r reload · q quit"),
    ]);
    let actions = Line::from(" t history · h hop · e export · i import · R resume · o scan folder");
    frame.render_widget(Paragraph::new(vec![navigation, actions]), area);
}

fn field(lines: &mut Vec<Line>, label: &str, value: &str) {
    lines.push(Line::from(vec![
        Span::styled(format!("{label:<9}"), Style::default().fg(Color::DarkGray)),
        Span::raw(value.to_string()),
    ]));
}

/// Stop an action that needs the selected session's transcript when the listing already knows it
/// cannot be read: say why in the status line. `true` when the action is refused.
fn refuse_unreadable(app: &mut App) -> bool {
    let Some(why) = app
        .selected_session()
        .and_then(|info| info.unreadable.clone())
    else {
        return false;
    };
    app.status = format!("cannot read this session: {why}");
    true
}

fn session_label(info: &SessionInfo) -> String {
    let title = info.title.as_deref().unwrap_or("").trim();
    if !title.is_empty() {
        return truncate(title, 48);
    }
    truncate(&info.id, 48)
}

fn truncate(text: &str, max: usize) -> String {
    let one_line = text.split('\n').next().unwrap_or("");
    if one_line.chars().count() <= max {
        one_line.to_string()
    } else {
        let mut out: String = one_line.chars().take(max.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}

fn truncate_cells(text: &str, max: usize) -> String {
    let text = text.split('\n').next().unwrap_or("");
    if Span::raw(text).width() <= max {
        return text.to_string();
    }
    if max == 0 {
        return String::new();
    }
    let mut out = String::new();
    let mut width = 0;
    for ch in text.chars() {
        let ch_width = Span::raw(ch.to_string()).width();
        if width + ch_width >= max {
            break;
        }
        width += ch_width;
        out.push(ch);
    }
    out.push('…');
    out
}

fn wrap_cells(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    let mut used = 0;
    for ch in text.chars() {
        let cells = Span::raw(ch.to_string()).width();
        if ch == '\n' || (used + cells > width && !line.is_empty()) {
            lines.push(std::mem::take(&mut line));
            used = 0;
        }
        if ch != '\n' {
            line.push(ch);
            used += cells;
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

/// The stores this process's variables name, each missing one at the catalog's default location.
fn resolve_roots() -> StoreRoots {
    let mut roots = StoreRoots::from_env();
    for harness in mukade::harnesses::all() {
        let root = harness.store_root_mut(&mut roots);
        if root.is_none() {
            *root = catalog::store_default(harness.id());
        }
    }
    roots
}

/// Every location's sessions, through the harness's own reader.
fn load_rows(locations: &[Location]) -> Vec<HarnessRow> {
    locations
        .iter()
        .map(|location| {
            let listed = reader::for_harness(location.harness)
                .and_then(|reader| reader.list_all(&location.roots));
            match listed {
                Ok(sessions) => HarnessRow::new(location.clone(), sessions, None),
                Err(error) => {
                    HarnessRow::new(location.clone(), Vec::new(), Some(error.to_string()))
                }
            }
        })
        .collect()
}

fn transcript_lines(session: &Session, turn_limit: usize) -> (Vec<Line<'static>>, usize) {
    let entries: Vec<&mukade::model::Entry> = session.conversation().collect();
    let user_turns: Vec<usize> = entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| matches!(entry.kind, Kind::User { .. }))
        .map(|(index, _)| index)
        .collect();
    let (start, hidden) = if turn_limit > 0 && user_turns.len() > turn_limit {
        let start = user_turns[user_turns.len() - turn_limit];
        (start, user_turns.len() - turn_limit)
    } else {
        (0, 0)
    };
    let mut out = Vec::new();
    if hidden > 0 {
        out.push(Line::from(Span::styled(
            format!("… {hidden} earlier turns hidden · t shows all"),
            Style::default().fg(Color::DarkGray),
        )));
    }
    for entry in &entries[start..] {
        match &entry.kind {
            Kind::User { text, attachments } => {
                push_text(&mut out, "USER", Color::Cyan, text);
                for attachment in attachments {
                    let path = attachment.path.as_deref().unwrap_or("");
                    push_text(
                        &mut out,
                        "  file",
                        Color::DarkGray,
                        &format!("{} {path}", attachment.kind),
                    );
                }
            }
            Kind::Step {
                reasoning,
                text,
                calls,
                ..
            } => {
                for block in reasoning {
                    if !block.text.trim().is_empty() {
                        push_text(&mut out, "THINK", Color::DarkGray, &block.text);
                    }
                }
                for block in text {
                    if !block.trim().is_empty() {
                        push_text(&mut out, "ASSISTANT", Color::White, block);
                    }
                }
                for call in calls {
                    let input = serde_json::to_string(&call.input).unwrap_or_default();
                    push_text(
                        &mut out,
                        "  tool",
                        Color::Yellow,
                        &format!("{} {}", call.source_name, truncate(&input, 240)),
                    );
                }
            }
            Kind::Results { results } => {
                for result in results {
                    let text = result.content.text();
                    let text = if result.is_error {
                        format!("error: {text}")
                    } else {
                        text
                    };
                    push_text(
                        &mut out,
                        "  result",
                        if result.is_error {
                            Color::Red
                        } else {
                            Color::Green
                        },
                        &text,
                    );
                }
            }
            Kind::Compaction { summary, .. } => {
                push_text(&mut out, "COMPACTION", Color::Magenta, summary);
            }
            Kind::Meta { .. } => {}
        }
    }
    if out.is_empty() {
        out.push(Line::from(Span::styled(
            "this session has no conversation entries",
            Style::default().fg(Color::DarkGray),
        )));
    }
    (out, hidden)
}

fn push_text(out: &mut Vec<Line<'static>>, label: &str, color: Color, text: &str) {
    for (index, part) in text.split('\n').enumerate() {
        if index == 0 {
            out.push(Line::from(vec![
                Span::styled(
                    format!("{label} "),
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                ),
                Span::raw(part.to_string()),
            ]));
        } else {
            out.push(Line::from(Span::raw(part.to_string())));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app_with_directories() -> App {
        let roots = StoreRoots::default();
        let locations = location::standard_locations(&roots);
        let sessions = [
            ("beta-first", Some("/projects/beta")),
            ("alpha-first", Some("/projects/alpha")),
            ("unknown", None),
            ("alpha-second", Some("/projects/alpha")),
            ("beta-second", Some("/projects/beta")),
        ]
        .into_iter()
        .map(|(id, cwd)| SessionInfo {
            id: id.to_string(),
            cwd: cwd.map(String::from),
            path: format!("{id}.jsonl"),
            ..Default::default()
        })
        .collect();
        let rows = vec![
            HarnessRow::new(locations[0].clone(), sessions, None),
            HarnessRow::new(locations[1].clone(), Vec::new(), None),
        ];
        App::new(locations, roots, rows, 50)
    }

    fn screen(app: &mut App, width: u16, height: u16) -> String {
        let mut terminal =
            Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
        terminal.draw(|frame| draw(frame, app)).unwrap();
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect()
    }

    /// A listing as `list_all` hands it back: a child before its parent, a fork, a subagent of a
    /// session that is not listed, in two folders.
    fn app_with_a_tree() -> App {
        use mukade::model::ParentKind;
        let roots = StoreRoots::default();
        let locations = location::standard_locations(&roots);
        let session = |id: &str, cwd: &str, parent: Option<(&str, ParentKind)>| SessionInfo {
            id: id.to_string(),
            title: Some(id.to_string()),
            cwd: Some(cwd.to_string()),
            path: format!("{id}.jsonl"),
            parent: parent.as_ref().map(|(parent, _)| parent.to_string()),
            parent_kind: parent.map(|(_, kind)| kind),
            ..Default::default()
        };
        let spawn = ParentKind::Spawn {
            call_id: None,
            agent: Some("explore".to_string()),
            inherits_context: false,
        };
        let sessions = vec![
            session("helper", "/projects/a", Some(("main", spawn.clone()))),
            session("main", "/projects/a", None),
            session("branch", "/projects/a", Some(("main", ParentKind::fork()))),
            session("stray", "/projects/b", Some(("gone", spawn))),
            session(
                "far-fork",
                "/projects/b",
                Some(("main", ParentKind::fork())),
            ),
        ];
        let rows = locations
            .iter()
            .enumerate()
            .map(|(index, location)| {
                let sessions = if index == 0 {
                    sessions.clone()
                } else {
                    Vec::new()
                };
                HarnessRow::new(location.clone(), sessions, None)
            })
            .collect();
        App::new(locations, roots, rows, 50)
    }

    #[test]
    fn the_session_list_nests_children_and_forks_under_their_parent() {
        let mut app = app_with_a_tree();
        app.focus = Focus::Sessions;
        let ids: Vec<_> = app
            .visible_sessions()
            .iter()
            .map(|&index| app.rows[0].sessions[index].id.clone())
            .collect();
        assert_eq!(ids, ["main", "helper", "branch", "far-fork", "stray"]);
        // The selection walks the displayed order and lands on the session, not on a position of
        // the unarranged listing.
        app.move_down();
        assert_eq!(app.selected_session().unwrap().id, "helper");
        let text = screen(&mut app, 120, 20);
        assert!(text.contains("↳ agent explore"), "{text}");
        assert!(text.contains("⑂ fork"), "{text}");
    }

    #[test]
    fn a_child_is_indented_under_its_parent_and_an_orphan_is_not() {
        let app = app_with_a_tree();
        let row = &app.rows[0];
        let lines: Vec<String> = row
            .directories
            .sessions(0)
            .iter()
            .zip(row.directories.depths(0))
            .map(|(&index, &depth)| {
                let info = &row.sessions[index];
                format!("{}{}", tree::prefix(info, depth), session_label(info))
            })
            .collect();
        assert_eq!(
            lines,
            [
                "main",
                "  ↳ agent explore helper",
                "  ⑂ fork branch",
                "  ⑂ fork far-fork",
                "↳ agent explore stray",
            ]
        );
    }

    #[test]
    fn a_folder_filter_still_finds_a_child_whose_parent_is_in_another_folder() {
        let mut app = app_with_a_tree();
        app.focus = Focus::Directories;
        app.move_down();
        app.move_down();
        app.focus = Focus::Sessions;
        let ids: Vec<_> = app
            .visible_sessions()
            .iter()
            .map(|&index| app.rows[0].sessions[index].id.clone())
            .collect();
        assert_eq!(ids, ["stray", "far-fork"], "/projects/b");
        assert_eq!(app.rows[0].directories.depths(2), [0, 0]);
        assert_eq!(app.selected_session().unwrap().id, "stray");
    }

    #[test]
    fn the_detail_pane_names_the_parent_the_kind_and_the_children() {
        let mut app = app_with_a_tree();
        app.focus = Focus::Sessions;
        let parent = screen(&mut app, 120, 30);
        assert!(parent.contains("3 (1 subagent, 2 forks)"), "{parent}");
        assert!(!parent.contains("parent  "), "a root has no parent line");
        app.move_down();
        let child = screen(&mut app, 120, 30);
        assert!(child.contains("parent"), "{child}");
        assert!(child.contains("subagent · explore"), "{child}");
        assert!(!child.contains("(not listed)"), "{child}");
        // The orphan's parent is named, and said not to be listed.
        app.move_down();
        app.move_down();
        app.move_down();
        assert_eq!(app.selected_session().unwrap().id, "stray");
        let orphan = screen(&mut app, 120, 30);
        assert!(orphan.contains("gone (not listed)"), "{orphan}");
    }

    fn select(app: &mut App, id: &str) {
        app.focus = Focus::Sessions;
        app.session = app
            .visible_sessions()
            .iter()
            .position(|&index| app.rows[0].sessions[index].id == id)
            .expect("the session is listed");
    }

    fn hop_of(app: &App) -> &HopState {
        let Mode::Hop(hop) = &app.mode else {
            panic!("the hop dialog is open");
        };
        hop
    }

    #[test]
    fn a_hop_offers_subagents_only_for_a_source_that_has_them_and_leaves_them_off() {
        let mut app = app_with_a_tree();
        select(&mut app, "branch");
        open_hop(&mut app);
        assert_eq!(
            hop_of(&app).fields,
            [
                HopField::Destination,
                HopField::Compaction,
                HopField::AllBranches,
                HopField::Prompt
            ],
            "a fork's source has no subagents"
        );
        assert!(!hop_of(&app).children);
        let text = screen(&mut app, 120, 30);
        assert!(!text.contains("subagents"), "{text}");
        app.mode = Mode::Normal;

        select(&mut app, "main");
        open_hop(&mut app);
        assert_eq!(
            hop_of(&app).fields,
            [
                HopField::Destination,
                HopField::Compaction,
                HopField::Children,
                HopField::AllBranches,
                HopField::Prompt
            ]
        );
        assert!(!hop_of(&app).children, "the default is to leave them");
        let text = screen(&mut app, 120, 30);
        assert!(text.contains("subagents"), "{text}");
        assert!(text.contains("leave behind"), "{text}");
    }

    #[test]
    fn the_subagent_field_toggles_with_the_arrows_and_the_space_bar_and_enter_walks_past_it() {
        let mut app = app_with_a_tree();
        select(&mut app, "main");
        open_hop(&mut app);
        handle_hop_key(&mut app, KeyCode::Enter);
        handle_hop_key(&mut app, KeyCode::Enter);
        assert_eq!(hop_of(&app).focus(), HopField::Children);
        handle_hop_key(&mut app, KeyCode::Right);
        assert!(hop_of(&app).children);
        let text = screen(&mut app, 120, 30);
        assert!(text.contains("carry"), "{text}");
        handle_hop_key(&mut app, KeyCode::Char(' '));
        assert!(!hop_of(&app).children);
        handle_hop_key(&mut app, KeyCode::Char(' '));
        assert!(hop_of(&app).children);
        // The branches field comes next; the space bar is text in the prompt, a toggle elsewhere.
        handle_hop_key(&mut app, KeyCode::Enter);
        assert_eq!(hop_of(&app).focus(), HopField::AllBranches);
        handle_hop_key(&mut app, KeyCode::Enter);
        assert_eq!(hop_of(&app).focus(), HopField::Prompt);
        handle_hop_key(&mut app, KeyCode::Char(' '));
        assert!(hop_of(&app).children);
        assert!(!hop_of(&app).all_branches);
        assert_eq!(hop_of(&app).prompt, " ");
        // Shift+tab goes back through the same fields.
        handle_hop_key(&mut app, KeyCode::BackTab);
        assert_eq!(hop_of(&app).focus(), HopField::AllBranches);
        handle_hop_key(&mut app, KeyCode::BackTab);
        assert_eq!(hop_of(&app).focus(), HopField::Children);
        handle_hop_key(&mut app, KeyCode::Tab);
        handle_hop_key(&mut app, KeyCode::Tab);
        handle_hop_key(&mut app, KeyCode::Tab);
        assert_eq!(hop_of(&app).focus(), HopField::Destination, "tab wraps");
    }

    /// The branches field: offered for a source whose branches the library reads, off until asked,
    /// toggled by the arrows and the space bar, and named on the dialog.
    #[test]
    fn the_branches_field_is_offered_where_the_library_reads_branches_and_toggles() {
        let mut app = app_with_a_tree();
        select(&mut app, "branch");
        open_hop(&mut app);
        assert_eq!(
            hop_of(&app).fields.contains(&HopField::AllBranches),
            mukade::hop::reads_all_branches(hop_of(&app).source.harness)
        );
        assert!(
            !hop_of(&app).all_branches,
            "the default is the resumed branch"
        );
        let text = screen(&mut app, 120, 30);
        assert!(text.contains("the resumed one"), "{text}");
        handle_hop_key(&mut app, KeyCode::Tab);
        handle_hop_key(&mut app, KeyCode::Tab);
        assert_eq!(hop_of(&app).focus(), HopField::AllBranches);
        handle_hop_key(&mut app, KeyCode::Right);
        assert!(hop_of(&app).all_branches);
        let text = screen(&mut app, 120, 30);
        assert!(text.contains("every branch"), "{text}");
        handle_hop_key(&mut app, KeyCode::Char(' '));
        assert!(!hop_of(&app).all_branches);
    }

    #[test]
    fn a_hops_branches_come_back_as_a_line_of_the_result() {
        let manifest = serde_json::json!({"counts": {"branches_kept": 2, "branches_dropped": 1}});
        assert_eq!(
            branch_line(&manifest).as_deref(),
            Some("branches     2 kept, 1 left out")
        );
        let none = serde_json::json!({"counts": {"branches_kept": 0, "branches_dropped": 0}});
        assert_eq!(branch_line(&none), None);
    }

    #[test]
    fn a_hops_children_come_back_as_lines_of_the_result() {
        let manifest = serde_json::json!({
            "losses": [],
            "children": [
                {"source_id": "a1", "dest_id": "d1", "link": "native"},
                {"source_id": "a2", "dest_id": "d2", "link": "none"},
            ],
        });
        assert_eq!(
            child_lines(&manifest),
            [
                "children     2",
                "  · a1 -> d1 (linked to its parent)",
                "  · a2 -> d2 (not linked: the destination has no link)",
            ]
        );
        assert!(child_lines(&serde_json::json!({"children": []})).is_empty());
        assert!(child_lines(&serde_json::json!({"losses": []})).is_empty());
    }

    fn copy_tree(from: &std::path::Path, to: &std::path::Path) {
        std::fs::create_dir_all(to).unwrap();
        for entry in std::fs::read_dir(from).unwrap().flatten() {
            let target = to.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                copy_tree(&entry.path(), &target);
            } else {
                std::fs::copy(entry.path(), &target).unwrap();
            }
        }
    }

    /// The Claude Code fixture whose session has subagents, listed by the explorer's own loader
    /// (`list_all`), hopped into a scratch Pi store with and without `HopRequest::children`.
    #[test]
    fn a_hop_carries_the_subagent_sessions_only_when_asked() {
        let dir = scratch("hop-children");
        let claude = dir.join("claude");
        copy_tree(
            std::path::Path::new("../fixtures/claude_code/subagents/projects"),
            &claude.join("projects"),
        );
        let roots = StoreRoots {
            claude_config_dir: Some(claude),
            pi_agent_dir: Some(dir.join("pi")),
            ..Default::default()
        };
        let locations = location::standard_locations(&roots);
        let rows = load_rows(&locations);
        let mut app = App::new(locations, roots, rows, 50);
        app.harness = app
            .rows
            .iter()
            .position(|row| row.location.harness == HarnessId::ClaudeCode)
            .unwrap();
        let parent = "5ab1c0de-0000-4000-8000-000000000001";
        let nested: Vec<_> = app
            .visible_sessions()
            .iter()
            .zip(app.rows[app.harness].directories.depths(0))
            .map(|(&index, &depth)| (app.rows[app.harness].sessions[index].id.clone(), depth))
            .collect();
        let at = nested.iter().position(|(id, _)| id == parent).unwrap();
        assert!(
            nested[at + 1..]
                .iter()
                .take_while(|(_, depth)| *depth > 0)
                .count()
                >= 4,
            "the listing nests the subagents under their parent: {nested:?}"
        );

        let details = |_: &Location, _: Option<&str>| WriteDetails {
            dest_version: Some("1.4.0".into()),
            dest_protocol: mukade::policy::destination_protocol(HarnessId::Pi),
            ..Default::default()
        };
        let hop_with = |app: &mut App, children: bool| {
            app.focus = Focus::Sessions;
            app.session = at;
            open_hop(app);
            let Mode::Hop(hop) = &mut app.mode else {
                panic!("the hop dialog opens");
            };
            hop.dest = hop
                .destinations
                .iter()
                .position(|l| l.harness == HarnessId::Pi)
                .unwrap();
            assert!(hop.fields.contains(&HopField::Children));
            hop.children = children;
            let lines = execute_hop_with(hop, details);
            app.mode = Mode::Normal;
            lines
        };

        let alone = hop_with(&mut app, false);
        assert!(
            alone.iter().any(|line| line.starts_with("destination")),
            "{alone:?}"
        );
        assert!(
            alone.iter().all(|line| !line.starts_with("children")),
            "no children unless asked: {alone:?}"
        );

        let together = hop_with(&mut app, true);
        assert!(
            together
                .iter()
                .any(|line| line.starts_with("children     4")),
            "{together:?}"
        );
        assert!(
            together
                .iter()
                .any(|line| line.contains("a1111111111111111 ->")),
            "{together:?}"
        );
    }

    /// A Claude Code session with an abandoned attempt at its second prompt, hopped into a
    /// scratch Pi store with and without `HopRequest::all_branches`.
    #[test]
    fn a_hop_carries_the_abandoned_branches_only_when_asked() {
        let dir = scratch("hop-branches");
        let claude = dir.join("claude");
        let row = |kind: &str, uuid: &str, parent: Option<&str>, text: &str| {
            let message = if kind == "user" {
                serde_json::json!({"role": "user", "content": text})
            } else {
                serde_json::json!({"id": format!("msg_{uuid}"), "role": "assistant",
                    "model": "claude-x", "stop_reason": "end_turn",
                    "content": [{"type": "text", "text": text}]})
            };
            serde_json::json!({"type": kind, "uuid": uuid, "parentUuid": parent,
                "sessionId": "b0000000-0000-4000-8000-000000000001", "cwd": "/work",
                "isSidechain": false, "timestamp": "2026-11-12T10:00:00.000Z",
                "message": message})
            .to_string()
        };
        let file = claude.join("projects/-work/b0000000-0000-4000-8000-000000000001.jsonl");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        let lines = [
            row("user", "u1", None, "q1"),
            row("assistant", "a1", Some("u1"), "r1"),
            row("user", "u2b", Some("a1"), "q2 first try"),
            row("assistant", "a2b", Some("u2b"), "r2 first try"),
            row("user", "u2", Some("a1"), "q2"),
            row("assistant", "a2", Some("u2"), "r2"),
        ];
        std::fs::write(&file, lines.join("\n") + "\n").unwrap();
        let roots = StoreRoots {
            claude_config_dir: Some(claude),
            pi_agent_dir: Some(dir.join("pi")),
            ..Default::default()
        };
        let locations = location::standard_locations(&roots);
        let rows = load_rows(&locations);
        let mut app = App::new(locations, roots, rows, 50);
        app.harness = app
            .rows
            .iter()
            .position(|row| row.location.harness == HarnessId::ClaudeCode)
            .unwrap();
        let details = |_: &Location, _: Option<&str>| WriteDetails {
            dest_version: Some("1.4.0".into()),
            dest_protocol: mukade::policy::destination_protocol(HarnessId::Pi),
            ..Default::default()
        };
        let hop_with = |app: &mut App, all_branches: bool| {
            app.focus = Focus::Sessions;
            app.session = 0;
            open_hop(app);
            let Mode::Hop(hop) = &mut app.mode else {
                panic!("the hop dialog opens");
            };
            hop.dest = hop
                .destinations
                .iter()
                .position(|l| l.harness == HarnessId::Pi)
                .unwrap();
            assert!(hop.fields.contains(&HopField::AllBranches));
            hop.all_branches = all_branches;
            let lines = execute_hop_with(hop, details);
            app.mode = Mode::Normal;
            lines
        };

        let active = hop_with(&mut app, false);
        assert!(
            active.iter().any(|line| line.starts_with("destination")),
            "{active:?}"
        );
        assert!(
            active.iter().all(|line| !line.starts_with("branches")),
            "no branches unless asked: {active:?}"
        );
        let every = hop_with(&mut app, true);
        assert!(
            every
                .iter()
                .any(|line| line == "branches     1 kept, 0 left out"),
            "{every:?}"
        );
    }

    #[test]
    fn a_directory_filter_keeps_the_original_session_as_the_hop_and_resume_target() {
        let mut app = app_with_directories();
        app.focus = Focus::Directories;
        app.move_down(); // /projects/alpha, whose sessions are interleaved in the store.
        app.next_focus();
        app.move_down();
        assert_eq!(app.selected_session().unwrap().id, "alpha-second");
        let (location, info) = app.selected_pair().unwrap();
        assert_eq!(location.harness, HarnessId::ClaudeCode);
        assert_eq!(
            resume_argv(location.harness, "claude", info),
            ["claude", "--resume", "alpha-second"]
        );
        let rendered = screen(&mut app, 120, 24);
        assert!(rendered.contains("alpha-first"));
        assert!(rendered.contains("alpha-second"));
        assert!(!rendered.contains("beta-first"));
        open_hop(&mut app);
        let Mode::Hop(hop) = &app.mode else {
            panic!("hop opens")
        };
        assert_eq!(hop.source_id, "alpha-second");
        assert_eq!(hop.source_cwd.as_deref(), Some("/projects/alpha"));
    }

    #[test]
    fn all_directories_and_unknown_directory_keep_every_session_reachable() {
        let mut app = app_with_directories();
        assert_eq!(app.visible_sessions().len(), 5);
        app.focus = Focus::Directories;
        for _ in 0..10 {
            app.move_down();
        }
        assert_eq!(app.visible_sessions().len(), 1);
        assert_eq!(app.selected_session().unwrap().id, "unknown");
        app.focus = Focus::Sessions;
        app.move_down();
        assert_eq!(app.selected_session().unwrap().id, "unknown");
        app.focus = Focus::Directories;
        for _ in 0..10 {
            app.move_up();
        }
        assert_eq!(app.visible_sessions().len(), 5);
    }

    #[test]
    fn changing_a_directory_or_harness_clears_the_previous_transcript() {
        let mut app = app_with_directories();
        let fixture = std::path::Path::new(
            "../fixtures/claude_code/source/c45db7bc-542c-4201-89de-b590c6fdb797.jsonl",
        );
        app.transcript = Some(Transcript {
            title: "previous".into(),
            session: mukade::reader::claude_code::read_file(fixture).unwrap(),
            lines: Vec::new(),
            hidden_turns: 0,
        });
        app.session = 3;
        app.scroll = 10;
        app.focus = Focus::Directories;
        app.move_down();
        assert!(app.transcript.is_none());
        assert_eq!(app.scroll, 0);
        assert_eq!(app.selected_session().unwrap().id, "alpha-first");
        app.focus = Focus::Harnesses;
        app.move_down();
        assert_eq!(app.directory, 0);
        assert!(app.selected_session().is_none());
        app.move_up();
        assert_eq!(app.visible_sessions().len(), 5);
    }

    #[test]
    fn reloading_after_the_filtered_directory_disappears_resets_the_filter() {
        let mut app = app_with_directories();
        app.focus = Focus::Directories;
        app.move_down();
        let (sender, receiver) = mpsc::channel();
        sender
            .send(JobResult::Loaded {
                locations: app.locations.clone(),
                rows: vec![HarnessRow::new(
                    app.locations[0].clone(),
                    vec![SessionInfo {
                        id: "remaining".into(),
                        cwd: Some("/projects/beta".into()),
                        ..Default::default()
                    }],
                    None,
                )],
                status: "ready".into(),
            })
            .unwrap();
        app.job = Some(receiver);
        poll_job(&mut app);
        assert_eq!(app.directory, 0);
        assert_eq!(app.selected_session().unwrap().id, "remaining");
        assert!(matches!(app.focus, Focus::Harnesses));
    }

    #[test]
    fn keyboard_focus_visits_directories_in_both_directions() {
        let mut app = app_with_directories();
        app.next_focus();
        assert!(matches!(app.focus, Focus::Directories));
        app.next_focus();
        assert!(matches!(app.focus, Focus::Sessions));
        app.previous_focus();
        assert!(matches!(app.focus, Focus::Directories));
        app.back();
        assert!(matches!(app.focus, Focus::Harnesses));
    }

    #[test]
    fn directory_filters_render_in_wide_narrow_and_single_panel_terminals() {
        let mut app = app_with_directories();
        for (width, height) in [(120, 40), (80, 24), (40, 12)] {
            app.focus = Focus::Directories;
            let rendered = screen(&mut app, width, height);
            assert!(rendered.contains("directories"));
            assert!(rendered.contains("alpha"));
            if width >= 70 {
                assert!(rendered.contains("sessions"));
            }
        }
    }

    #[test]
    fn same_named_projects_remain_distinguishable_without_the_metadata_panel() {
        let mut app = app_with_directories();
        let sessions = [
            "/projects/long-shared-parent/one/app",
            "/projects/long-shared-parent/two/app",
        ]
        .into_iter()
        .enumerate()
        .map(|(index, cwd)| SessionInfo {
            id: index.to_string(),
            cwd: Some(cwd.into()),
            ..Default::default()
        })
        .collect();
        app.rows[0] = HarnessRow::new(app.locations[0].clone(), sessions, None);
        app.focus = Focus::Directories;
        let rendered = screen(&mut app, 80, 24);
        assert!(rendered.contains("one/app"));
        assert!(rendered.contains("two/app"));
    }

    #[test]
    fn a_selected_directory_shows_the_complete_path_even_with_long_identical_prefixes() {
        let mut app = app_with_directories();
        let cwd = "/projects/long-shared-parent/frontend-experiment-branch-one/app";
        let sessions = [
            cwd,
            "/projects/long-shared-parent/frontend-experiment-branch-two/app",
        ]
        .into_iter()
        .map(|cwd| SessionInfo {
            cwd: Some(cwd.into()),
            ..Default::default()
        })
        .collect();
        app.rows[0] = HarnessRow::new(app.locations[0].clone(), sessions, None);
        app.directory = 1;
        app.focus = Focus::Directories;
        let mut terminal = Terminal::new(ratatui::backend::TestBackend::new(28, 24)).unwrap();
        terminal
            .draw(|frame| draw_directories(frame, frame.area(), &app))
            .unwrap();
        let buffer = terminal.backend().buffer();
        let mut content = String::new();
        for y in 1..23 {
            let line: String = (1..27)
                .map(|x| buffer.cell((x, y)).unwrap().symbol())
                .collect();
            content.push_str(line.trim_end());
        }
        assert!(
            content.contains(cwd),
            "selected path was clipped: {content}"
        );
    }

    #[test]
    fn a_selected_directory_stays_visible_when_its_path_exceeds_the_panel_height() {
        let mut app = app_with_directories();
        app.rows[0] = HarnessRow::new(app.locations[0].clone(), vec![SessionInfo {
            id: "long-path".into(),
            cwd: Some(r"C:\Users\alfio\Documents\GitHub\company-frontend-platform\frontend-experiment-branch-one\apps\example-app".into()),
            ..Default::default()
        }], None);
        app.directory = 1;
        app.focus = Focus::Directories;
        assert!(screen(&mut app, 80, 10).contains("example-app"));
    }

    #[test]
    fn the_explorer_has_a_directory_filter_column() {
        let mut app = App::new(Vec::new(), StoreRoots::default(), Vec::new(), 50);
        let mut terminal = Terminal::new(ratatui::backend::TestBackend::new(120, 40)).unwrap();
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        let screen: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(
            screen.contains("directories"),
            "directory filter is missing"
        );
    }

    #[test]
    fn a_fixture_becomes_readable_lines() {
        let path = std::path::Path::new(
            "../fixtures/claude_code/source/c45db7bc-542c-4201-89de-b590c6fdb797.jsonl",
        );
        let session = mukade::reader::claude_code::read_file(path).expect("fixture");
        let (lines, hidden) = transcript_lines(&session, 50);
        assert_eq!(hidden, 0);
        assert!(!lines.is_empty());
        assert!(lines.iter().any(|line| {
            line.spans
                .iter()
                .any(|span| span.content.contains("USER") || span.content.contains("ASSISTANT"))
        }));
    }

    /// The interactive explorer draws its first frame before the background scan returns any
    /// harness, so every pane must render, and every key must be handled, over empty rows.
    #[test]
    fn the_first_frame_renders_before_the_scan_returns() {
        let mut app = App::new(Vec::new(), StoreRoots::default(), Vec::new(), 50);
        app.busy = true;
        app.status = "scanning stores…".to_string();
        let mut terminal = Terminal::new(ratatui::backend::TestBackend::new(120, 40)).unwrap();
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        let screen: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(screen.contains("scanning stores"));

        app.busy = false;
        for focus in [
            Focus::Harnesses,
            Focus::Directories,
            Focus::Sessions,
            Focus::Transcript,
        ] {
            app.focus = focus;
            app.move_down();
            app.move_up();
        }
        assert!(app.selected_session().is_none());
        open_hop(&mut app);
        assert!(matches!(app.mode, Mode::Normal));
    }

    #[test]
    fn the_scan_flag_is_repeatable() {
        let args: Vec<String> = [
            "mukade-tui",
            "--scan",
            "D:\\Apps",
            "--turns",
            "5",
            "--scan=E:\\",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        assert_eq!(
            parse_scan_folders(&args),
            vec![
                std::path::PathBuf::from("D:\\Apps"),
                std::path::PathBuf::from("E:\\")
            ]
        );
        assert!(parse_scan_folders(&["mukade-tui".to_string(), "--scan".to_string()]).is_empty());
    }

    /// A Claude Code session can hop into a portable JanuraCode the scan found, and an OpenCode
    /// session into it too: the hop offers every other location.
    #[test]
    fn a_hop_offers_the_installations_a_scan_found() {
        let standard = StoreRoots::default();
        let mut locations = location::standard_locations(&standard);
        location::add_installations(
            &mut locations,
            &[mukade::detect::Installation {
                harness: HarnessId::OpenCode,
                store: std::path::PathBuf::from("apps/JanuraCode/data/share/opencode/opencode.db"),
                exe: Some(std::path::PathBuf::from(
                    "apps/JanuraCode/resources/opencode-cli/opencode.exe",
                )),
                evidence: Vec::new(),
            }],
            &standard,
        );
        let rows = locations
            .iter()
            .map(|location| {
                HarnessRow::new(
                    location.clone(),
                    vec![SessionInfo {
                        id: "abc".to_string(),
                        path: "session.jsonl".to_string(),
                        ..Default::default()
                    }],
                    None,
                )
            })
            .collect();
        let mut app = App::new(locations, standard, rows, 50);
        open_hop(&mut app);
        let Mode::Hop(hop) = &app.mode else {
            panic!("the hop dialog opens");
        };
        assert_eq!(hop.source.harness, HarnessId::ClaudeCode);
        // One standard location per harness and the scanned installation, less the source: every
        // harness is a destination.
        assert_eq!(hop.destinations.len(), HarnessId::ALL.len());
        assert!(
            hop.destinations
                .iter()
                .any(|l| l.label.contains("JanuraCode"))
        );
    }

    #[test]
    fn the_open_prompt_collects_a_folder_and_esc_cancels_it() {
        let mut app = App::new(Vec::new(), StoreRoots::default(), Vec::new(), 50);
        app.mode = Mode::Open(String::new());
        for character in "D:\\Apps".chars() {
            handle_open_key(&mut app, KeyCode::Char(character));
        }
        handle_open_key(&mut app, KeyCode::Backspace);
        assert!(matches!(&app.mode, Mode::Open(path) if path == "D:\\App"));
        handle_open_key(&mut app, KeyCode::Esc);
        assert!(matches!(app.mode, Mode::Normal));
        // An empty folder starts nothing.
        app.mode = Mode::Open(String::new());
        handle_open_key(&mut app, KeyCode::Enter);
        assert!(!app.busy);
    }

    /// Claude Code with a session that can be read, one whose transcript is gone, and one whose
    /// process runs.
    fn app_with_unreadable_sessions() -> App {
        let roots = StoreRoots::default();
        let locations = location::standard_locations(&roots);
        let sessions = vec![
            SessionInfo {
                id: "kept".into(),
                cwd: Some("/projects/kept".into()),
                path: "kept.jsonl".into(),
                live: Some(false),
                ..Default::default()
            },
            SessionInfo {
                id: "lost".into(),
                cwd: Some("/projects/lost".into()),
                path: "history.jsonl".into(),
                unreadable: Some("its transcript is gone; only prompts remain".into()),
                live: Some(false),
                ..Default::default()
            },
            SessionInfo {
                id: "running".into(),
                cwd: Some("/projects/running".into()),
                path: "running.jsonl".into(),
                live: Some(true),
                ..Default::default()
            },
        ];
        let rows = vec![
            HarnessRow::new(locations[0].clone(), sessions, None),
            HarnessRow::new(locations[1].clone(), Vec::new(), None),
        ];
        App::new(locations, roots, rows, 50)
    }

    #[test]
    fn a_session_without_a_transcript_is_marked_and_is_not_opened_hopped_or_resumed() {
        let mut app = app_with_unreadable_sessions();
        let rendered = screen(&mut app, 140, 24);
        assert!(rendered.contains("[unreadable]"), "{rendered}");
        assert!(rendered.contains("[live]"), "{rendered}");

        app.session = 1;
        assert_eq!(app.selected_session().unwrap().id, "lost");
        let detail = screen(&mut app, 140, 24);
        assert!(detail.contains("its transcript is gone"), "{detail}");

        assert!(refuse_unreadable(&mut app));
        assert!(
            app.status.contains("its transcript is gone"),
            "{}",
            app.status
        );

        app.status.clear();
        open_hop(&mut app);
        assert!(matches!(app.mode, Mode::Normal), "no hop dialog for it");
        assert!(
            app.status.contains("its transcript is gone"),
            "{}",
            app.status
        );

        app.status.clear();
        start_read(&mut app);
        assert!(!app.busy, "nothing is read");
        assert!(
            app.status.contains("its transcript is gone"),
            "{}",
            app.status
        );

        app.session = 0;
        assert!(
            !refuse_unreadable(&mut app),
            "a session with a transcript is not refused"
        );
        open_hop(&mut app);
        assert!(matches!(app.mode, Mode::Hop(_)));
    }

    #[test]
    fn the_turns_flag_is_parameterised_with_a_safe_default() {
        assert_eq!(parse_turns(&[]), 50);
        assert_eq!(parse_turns(&["--turns".to_string(), "7".to_string()]), 7);
        assert_eq!(parse_turns(&["--turns=0".to_string()]), 0);
        assert_eq!(parse_turns(&["--turns=oops".to_string()]), 50);
    }

    #[test]
    fn the_transcript_can_keep_only_the_last_turns() {
        let path = std::path::Path::new(
            "../fixtures/claude_code/source/c45db7bc-542c-4201-89de-b590c6fdb797.jsonl",
        );
        let session = mukade::reader::claude_code::read_file(path).expect("fixture");
        let (lines, hidden) = transcript_lines(&session, 1);
        assert!(hidden > 0);
        assert!(lines.first().is_some_and(|line| {
            line.spans
                .iter()
                .any(|span| span.content.contains("earlier turns hidden"))
        }));
    }

    #[test]
    fn every_harness_has_the_exact_interactive_resume_argv() {
        let info = SessionInfo {
            id: "abc".to_string(),
            path: "/tmp/session.jsonl".to_string(),
            ..Default::default()
        };
        let tail = |harness| resume_argv(harness, "exe", &info)[1..].to_vec();
        assert_eq!(
            tail(HarnessId::ClaudeCode),
            vec!["--resume".to_string(), "abc".to_string()]
        );
        assert_eq!(
            tail(HarnessId::Codex),
            vec!["resume".to_string(), "abc".to_string()]
        );
        assert_eq!(
            tail(HarnessId::OpenCode),
            vec![
                "mini".to_string(),
                "--standalone".to_string(),
                "--session".to_string(),
                "abc".to_string()
            ]
        );
        assert_eq!(
            tail(HarnessId::Pi),
            vec!["--session".to_string(), "/tmp/session.jsonl".to_string()]
        );
        assert_eq!(
            tail(HarnessId::Hermes),
            vec!["--resume".to_string(), "abc".to_string()]
        );
        // dsh has no terminal UI: `R` shows the refusal; the catalog's headless resume is what a
        // hop's receipt carries.
        assert_eq!(
            tail(HarnessId::DeepSeekHarness),
            ["--profile", "headless", "--session-id", "abc"].map(String::from)
        );
        assert!(
            mukade::harnesses::get(HarnessId::DeepSeekHarness)
                .resume_refusal(&info)
                .is_some()
        );
    }

    #[test]
    fn compaction_options_map_to_the_cli_semantics() {
        assert_eq!(compaction_for(0).name(), "none");
        assert_eq!(compaction_for(1).name(), "portable");
        assert_eq!(compaction_for(2).name(), "summary");
    }

    #[test]
    fn receipt_lines_state_both_commands_and_every_loss() {
        let receipt = mukade::Receipt {
            dest_id: "new-id".to_string(),
            location: "target/rollout.jsonl".to_string(),
            resume_command: vec!["codex".into(), "exec".into(), "resume".into()],
            manifest: serde_json::json!({ "losses": ["one", "two"] }),
            resume_output: None,
        };
        let codex = location::standard_locations(&StoreRoots::default())
            .into_iter()
            .find(|location| location.harness == HarnessId::Codex)
            .unwrap();
        let lines = receipt_lines_with(&receipt, &codex, None);
        assert!(lines.iter().any(|line| line.contains("new-id")));
        assert!(lines.iter().any(|line| line.contains("headless")));
        assert!(lines.iter().any(|line| line.contains("interactive")));
        assert!(lines.iter().any(|line| line.contains("two")));
    }

    #[test]
    fn a_receipt_into_hermes_opens_it_and_one_into_dsh_says_it_has_no_terminal_ui() {
        let receipt = |id: &str, resume: &[&str]| mukade::Receipt {
            dest_id: id.to_string(),
            location: "store".to_string(),
            resume_command: resume.iter().map(|part| part.to_string()).collect(),
            manifest: serde_json::json!({ "losses": [] }),
            resume_output: None,
        };
        let location = |harness| {
            location::standard_locations(&StoreRoots::default())
                .into_iter()
                .find(|location| location.harness == harness)
                .unwrap()
        };
        let hermes = receipt_lines_with(
            &receipt("20261112_000000_abc123", &["hermes", "chat"]),
            &location(HarnessId::Hermes),
            None,
        );
        assert!(
            hermes.iter().any(|line| line.starts_with("interactive")
                && line.contains("--resume 20261112_000000_abc123")),
            "{hermes:?}"
        );
        let dsh = receipt_lines_with(
            &receipt("session-1", &["dsh", "--profile", "headless"]),
            &location(HarnessId::DeepSeekHarness),
            None,
        );
        assert!(
            dsh.iter().any(
                |line| line.starts_with("interactive  none") && line.contains("no terminal UI")
            ),
            "{dsh:?}"
        );
    }

    #[test]
    fn labels_prefer_the_title_and_truncate_the_id() {
        let info = SessionInfo {
            id: "abcdefghijklmnopqrstuvwxyz".to_string(),
            title: Some("A named session".to_string()),
            ..Default::default()
        };
        assert_eq!(session_label(&info), "A named session");
        let info = SessionInfo {
            id: "abcdefghijklmnopqrstuvwxyz".to_string(),
            ..Default::default()
        };
        assert!(session_label(&info).chars().count() <= 48);
    }

    fn scratch(tag: &str) -> crate::scratch::Scratch {
        crate::scratch::Scratch::new(tag)
    }

    #[test]
    fn an_export_file_is_named_by_harness_and_a_safe_id() {
        let dir = std::path::Path::new("exports");
        assert_eq!(
            export_file(dir, HarnessId::Codex, "01a17e07-93f7"),
            dir.join("codex-01a17e07-93f7.json")
        );
        assert_eq!(
            export_file(dir, HarnessId::Pi, "a/b\\c:d"),
            dir.join("pi-a_b_c_d.json")
        );
    }

    /// `e` then `i`: a Claude Code fixture is exported to a document and written into a scratch Pi
    /// store as a new session, with a receipt and the same messages.
    #[test]
    fn an_exported_session_imports_into_another_harness() {
        let dir = scratch("export-import");
        let id = "c45db7bc-542c-4201-89de-b590c6fdb797";
        let claude = dir.join("claude");
        let project = claude.join("projects").join("p");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::copy(
            format!("../fixtures/claude_code/source/{id}.jsonl"),
            project.join(format!("{id}.jsonl")),
        )
        .unwrap();
        let roots = StoreRoots {
            claude_config_dir: Some(claude.clone()),
            pi_agent_dir: Some(dir.join("pi")),
            ..Default::default()
        };
        let locations = location::standard_locations(&roots);
        let source = locations
            .iter()
            .find(|l| l.harness == HarnessId::ClaudeCode)
            .unwrap();
        let exported = export_session(source, id, &dir.join("exports")).expect("export");
        assert_eq!(
            exported.path,
            dir.join("exports").join(format!("claude_code-{id}.json"))
        );
        assert!(exported.messages > 0);

        let (doc, session) = read_document_file(&exported.path).expect("the document reads back");
        let pi = locations
            .iter()
            .find(|l| l.harness == HarnessId::Pi)
            .unwrap();
        let write = WriteDetails {
            dest_version: Some("1.4.0".into()),
            dest_protocol: mukade::policy::destination_protocol(HarnessId::Pi),
            ..Default::default()
        };
        let receipt = import_read(pi, &doc, &session, write, None).expect("import");
        assert!(std::path::Path::new(&receipt.location).starts_with(dir.join("pi")));
        let written = mukade::export(
            &SessionRef::id(HarnessId::Pi, receipt.dest_id.clone()),
            &roots,
        )
        .expect("the written session is a Pi session");
        assert_eq!(
            written.conversation().count(),
            session.conversation().count(),
            "the same conversation"
        );
    }

    #[test]
    fn a_missing_or_broken_document_is_refused_with_its_path() {
        let dir = scratch("broken-document");
        let missing = dir.join("missing.json");
        assert!(
            read_document_file(&missing)
                .unwrap_err()
                .contains("missing.json")
        );
        let broken = dir.join("broken.json");
        std::fs::write(&broken, "{not json").unwrap();
        assert!(
            read_document_file(&broken)
                .unwrap_err()
                .contains("not a JSON document")
        );
        let location = location::standard_locations(&StoreRoots::default()).remove(0);
        let lines = execute_import(&missing, &location, None);
        assert!(lines[0].starts_with("import failed"), "{lines:?}");
    }

    #[test]
    fn the_import_dialog_starts_with_the_last_export_and_walks_its_fields() {
        let mut app = app_with_directories();
        app.last_export = Some("exports/claude_code-x.json".to_string());
        open_import(&mut app);
        let Mode::Import(state) = &app.mode else {
            panic!("the import dialog opens");
        };
        assert_eq!(state.path, "exports/claude_code-x.json");
        assert_eq!(state.field, 0);
        handle_import_key(&mut app, KeyCode::Enter);
        handle_import_key(&mut app, KeyCode::Down);
        let Mode::Import(state) = &app.mode else {
            panic!("still open");
        };
        assert_eq!((state.field, state.dest), (1, 1));
        let rendered = screen(&mut app, 120, 30);
        assert!(rendered.contains("import"), "{rendered}");
        handle_import_key(&mut app, KeyCode::Esc);
        assert!(matches!(app.mode, Mode::Normal));
    }
}
