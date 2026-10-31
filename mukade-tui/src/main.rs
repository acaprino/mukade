//! A deliberately small native terminal browser for Mukade sessions.
//!
//! Left: the four harnesses' standard stores, then every installation a scan found (portable and
//! custom ones included), with their session counts. Middle: the sessions of the selected row.
//! Right: the selected session's metadata, or its transcript after `Enter`.
//! `R` resumes in the session's own installation; `h` hops it to another and shows the manifest;
//! `o` scans a folder for more installations (`--scan <dir>` does it at start).

mod location;

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
    Compaction, HopError, HopRequest, KeepFrom, ResumePrompt, SessionRef, SummarySource,
    WriteDetails,
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
    Sessions,
    Transcript,
}

/// One location's sessions, as the left and middle columns show them.
struct HarnessRow {
    location: Location,
    sessions: Vec<SessionInfo>,
    error: Option<String>,
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
}

struct ResultView {
    lines: Vec<String>,
    scroll: u16,
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
    prompt: String,
    field: usize,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum JobKind {
    Scan,
    Read,
    Hop,
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
}

struct App {
    /// Where sessions are listed from: the standard stores, then scanned installations.
    locations: Vec<Location>,
    /// The standard stores, which a scanned installation keeps for the other harnesses.
    standard: StoreRoots,
    rows: Vec<HarnessRow>,
    harness: usize,
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
        }
    }

    /// `None` until the first scan returns: the interactive explorer draws before it has rows.
    fn selected_row(&self) -> Option<&HarnessRow> {
        self.rows
            .get(self.harness.min(self.rows.len().saturating_sub(1)))
    }

    fn selected_session(&self) -> Option<&SessionInfo> {
        self.selected_row()?.sessions.get(self.session)
    }

    /// The selected session together with the location that holds it.
    fn selected_pair(&self) -> Option<(&Location, &SessionInfo)> {
        let row = self.selected_row()?;
        Some((&row.location, row.sessions.get(self.session)?))
    }

    fn move_down(&mut self) {
        match self.focus {
            Focus::Harnesses => {
                self.harness = (self.harness + 1).min(self.rows.len().saturating_sub(1));
                self.session = 0;
                self.transcript = None;
            }
            Focus::Sessions => {
                let len = self.selected_row().map_or(0, |row| row.sessions.len());
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
                self.session = 0;
                self.transcript = None;
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
            if key.code == KeyCode::Char('q') && app.job_kind != JobKind::Hop {
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
            KeyCode::Char('R') => resume_selected(terminal, app)?,
            KeyCode::Char('o') => app.mode = Mode::Open(String::new()),
            KeyCode::Tab => {
                app.focus = match app.focus {
                    Focus::Harnesses => Focus::Sessions,
                    Focus::Sessions => Focus::Harnesses,
                    Focus::Transcript => Focus::Transcript,
                };
            }
            KeyCode::Esc => {
                if app.transcript.is_some() {
                    app.transcript = None;
                    app.scroll = 0;
                    app.focus = Focus::Sessions;
                } else {
                    app.focus = Focus::Harnesses;
                }
            }
            KeyCode::Enter => match app.focus {
                Focus::Harnesses => app.focus = Focus::Sessions,
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
            app.session = 0;
            app.transcript = None;
            app.scroll = 0;
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
            app.mode = Mode::Result(ResultView { lines, scroll: 0 });
        }
    }
}

fn open_hop(app: &mut App) {
    let Some((source, info)) = app.selected_pair() else {
        app.status = "no session selected".to_string();
        return;
    };
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
        prompt: String::new(),
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
        match code {
            KeyCode::Esc => app.mode = Mode::Normal,
            KeyCode::Tab => hop.field = (hop.field + 1) % 3,
            KeyCode::Up | KeyCode::Down | KeyCode::Left | KeyCode::Right => match hop.field {
                0 => {
                    hop.dest = if matches!(code, KeyCode::Up | KeyCode::Left) {
                        (hop.dest + count - 1) % count
                    } else {
                        (hop.dest + 1) % count
                    };
                }
                1 => {
                    hop.compaction = if matches!(code, KeyCode::Up | KeyCode::Left) {
                        (hop.compaction + 2) % 3
                    } else {
                        (hop.compaction + 1) % 3
                    };
                }
                _ => {}
            },
            KeyCode::Enter if hop.field < 2 => hop.field += 1,
            KeyCode::Enter => run = true,
            KeyCode::Backspace if hop.field == 2 => {
                hop.prompt.pop();
            }
            KeyCode::Char(character) if hop.field == 2 => hop.prompt.push(character),
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
    let exe = dest.engine();
    let configured = dest::configured_model(dest.harness);
    let project_id = if dest.harness == HarnessId::OpenCode {
        cwd.as_deref().and_then(|cwd| {
            mukade::reader::opencode::project_id_for(
                &exe,
                cwd,
                &dest.roots,
                mukade::cache::dir().as_deref(),
            )
        })
    } else {
        None
    };
    let request = HopRequest {
        source,
        dest: dest.harness,
        cwd,
        compaction,
        resume: (!hop.prompt.trim().is_empty())
            .then(|| ResumePrompt::new(hop.prompt.trim().to_string())),
        reasoning: ReasoningPolicy::default(),
        roots: hop.source.roots.clone(),
        write: WriteDetails {
            dest_version: exe::version_of(&exe),
            dest_protocol: mukade::policy::destination_protocol(dest.harness),
            model_provider: configured.as_ref().map(|(provider, _)| provider.clone()),
            model: configured.and_then(|(_, model)| model),
            project_id,
            exe: Some(exe),
            ..Default::default()
        },
    };
    match mukade::hop_into(&request, &dest.roots) {
        Ok(receipt) => receipt_lines(&receipt, dest),
        Err(HopError::ResumeFailed { reason, receipt }) => {
            let mut lines = vec![format!("resume failed: {reason}"), String::new()];
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
    let interactive = resume_argv(dest.harness, &dest.engine(), &info);
    let mut lines = vec![
        format!("destination  {} ({})", receipt.dest_id, dest.label),
        format!("location     {}", receipt.location),
        format!("headless     {}", receipt.resume_command.join(" ")),
        format!("interactive  {}", interactive.join(" ")),
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
    if let Some(reply) = &receipt.resume_output {
        lines.push(String::new());
        lines.push("resume reply".to_string());
        lines.extend(reply.lines().map(str::to_string));
    }
    lines
}

fn save_manifest(receipt: &mukade::Receipt, dest: HarnessId) -> Option<String> {
    let data = mukade::cache::dir()?.parent()?.to_path_buf();
    let dir = data.join("manifests");
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
    let (location, info) = {
        let Some((location, info)) = app.selected_pair() else {
            app.status = "no session selected".to_string();
            return Ok(());
        };
        (location.clone(), info.clone())
    };
    let harness = location.harness;
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
    let rect = centered(area, 78, 17);
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
        let focus = hop.field == 0 && selected;
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
            if hop.field == 1 { "▶ " } else { "  " },
            Style::default().fg(Color::Green),
        ),
        Span::styled("compaction  ", Style::default().fg(Color::DarkGray)),
        Span::styled(compaction, Style::default().fg(Color::Yellow)),
    ]));
    lines.push(Line::from(vec![
        Span::styled(
            if hop.field == 2 { "▶ " } else { "  " },
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
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "tab field · ↑/↓ change · enter next/run · esc cancel",
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
    let block = Block::default().borders(Borders::ALL).title(" hop result ");
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
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(26),
            Constraint::Length(40),
            Constraint::Min(30),
        ])
        .split(area);

    draw_harnesses(frame, chunks[0], app);
    draw_sessions(frame, chunks[1], app);
    draw_detail(frame, chunks[2], app);
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
    let title = format!(" {} · {} ", row.location.label, row.sessions.len());
    let items: Vec<ListItem> = row
        .sessions
        .iter()
        .map(|info| {
            let title = session_label(info);
            let when = info.updated.as_deref().unwrap_or("—");
            let archived = if info.archived { " [archived]" } else { "" };
            let fork = if info.parent.is_some() { " ↳" } else { "" };
            ListItem::new(Line::from(vec![
                Span::styled(format!("{title}{fork}"), Style::default().fg(Color::White)),
                Span::styled(
                    format!("{archived}  {when}"),
                    Style::default().fg(Color::DarkGray),
                ),
            ]))
        })
        .collect();
    let mut state = ListState::default();
    if !row.sessions.is_empty() {
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
            field(&mut lines, "parent", parent);
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
        Focus::Sessions => "sessions",
        Focus::Transcript => "transcript",
    };
    let line = Line::from(vec![
        Span::styled(
            format!(" {focus} "),
            Style::default().fg(Color::Black).bg(Color::Gray),
        ),
        Span::raw(
            "  j/k move · enter open · t history · h hop · R resume · o open folder · tab focus · esc back · r reload · q quit",
        ),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

fn field(lines: &mut Vec<Line>, label: &str, value: &str) {
    lines.push(Line::from(vec![
        Span::styled(format!("{label:<9}"), Style::default().fg(Color::DarkGray)),
        Span::raw(value.to_string()),
    ]));
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

fn resolve_roots() -> StoreRoots {
    let mut roots = StoreRoots::from_env();
    if roots.claude_config_dir.is_none() {
        roots.claude_config_dir = catalog::store_default(HarnessId::ClaudeCode);
    }
    if roots.codex_home.is_none() {
        roots.codex_home = catalog::store_default(HarnessId::Codex);
    }
    if roots.opencode_db.is_none() {
        roots.opencode_db = catalog::store_default(HarnessId::OpenCode);
    }
    if roots.pi_agent_dir.is_none() {
        roots.pi_agent_dir = catalog::store_default(HarnessId::Pi);
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
                Ok(sessions) => HarnessRow {
                    location: location.clone(),
                    sessions,
                    error: None,
                },
                Err(error) => HarnessRow {
                    location: location.clone(),
                    sessions: Vec::new(),
                    error: Some(error.to_string()),
                },
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
        for focus in [Focus::Harnesses, Focus::Sessions, Focus::Transcript] {
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
            .map(|location| HarnessRow {
                location: location.clone(),
                sessions: vec![SessionInfo {
                    id: "abc".to_string(),
                    path: "session.jsonl".to_string(),
                    ..Default::default()
                }],
                error: None,
            })
            .collect();
        let mut app = App::new(locations, standard, rows, 50);
        open_hop(&mut app);
        let Mode::Hop(hop) = &app.mode else {
            panic!("the hop dialog opens");
        };
        assert_eq!(hop.source.harness, HarnessId::ClaudeCode);
        assert_eq!(hop.destinations.len(), 4);
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
}
