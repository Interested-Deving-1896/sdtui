use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
    Terminal,
};
use regex::Regex;
use std::{
    collections::HashMap,
    error::Error,
    io,
    process::{Command, Stdio},
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::{Duration, Instant},
};

#[derive(PartialEq, Clone, Copy)]
enum ViewMode {
    User,
    System,
}

#[derive(PartialEq, Clone, Copy, Debug)]
enum UnitType {
    Service,
    Target,
    Socket,
    Slice,
    Timer,
    Mount,
}

impl UnitType {
    fn all_variants() -> Vec<UnitType> {
        vec![
            UnitType::Service,
            UnitType::Target,
            UnitType::Socket,
            UnitType::Slice,
            UnitType::Timer,
            UnitType::Mount,
        ]
    }

    fn extension(&self) -> &'static str {
        match self {
            UnitType::Service => ".service",
            UnitType::Target => ".target",
            UnitType::Socket => ".socket",
            UnitType::Slice => ".slice",
            UnitType::Timer => ".timer",
            UnitType::Mount => ".mount",
        }
    }

    fn label(&self) -> &'static str {
        match self {
            UnitType::Service => "Services",
            UnitType::Target => "Targets",
            UnitType::Socket => "Sockets",
            UnitType::Slice => "Slices",
            UnitType::Timer => "Timers",
            UnitType::Mount => "Mounts",
        }
    }
}

#[derive(Clone, Debug)]
struct ServiceUnit {
    name: String,
    load: String,
    active: String,
    sub: String,
    description: String,
}

enum WorkerMsg {
    RefreshRequest,
    ExecuteAction {
        action: String,
        unit_name: String,
        is_system: bool,
    },
    FetchLogs {
        unit_name: String,
        is_system: bool,
    },
}

enum MainMsg {
    UpdateUnits(Vec<ServiceUnit>, Vec<ServiceUnit>),
    UpdateLogs(String),
    ActionComplete,
}

struct App {
    view_mode: ViewMode,
    active_filters: Vec<UnitType>,
    user_services: Vec<ServiceUnit>,
    system_services: Vec<ServiceUnit>,
    user_list_state: ListState,
    system_list_state: ListState,
    selected_user_unit: Option<String>,
    selected_system_unit: Option<String>,
    show_options: bool,
    options_state: ListState,
    options: Vec<&'static str>,
    show_filters: bool,
    filter_list_state: ListState,
    show_help: bool,
    search_query: String,
    is_searching: bool,
    worker_tx: Sender<WorkerMsg>,
    main_rx: Receiver<MainMsg>,
    is_busy: bool,
    status_message: Option<String>,
    logs: String,
    last_log_request: Instant,
    show_full_logs: bool,
    log_highlight_query: String,
    is_entering_highlight: bool,
    log_scroll: u16,
    auto_scroll: bool,
    last_nav_time: Instant,
}

impl App {
    fn new(worker_tx: Sender<WorkerMsg>, main_rx: Receiver<MainMsg>) -> App {
        let mut app = App {
            view_mode: ViewMode::User,
            active_filters: vec![UnitType::Service],
            user_services: Vec::new(),
            system_services: Vec::new(),
            user_list_state: ListState::default(),
            system_list_state: ListState::default(),
            selected_user_unit: None,
            selected_system_unit: None,
            show_options: false,
            options_state: ListState::default(),
            options: vec!["Start", "Stop", "Enable", "Disable", "Edit"],
            show_filters: false,
            filter_list_state: ListState::default(),
            show_help: false,
            search_query: String::new(),
            is_searching: false,
            worker_tx,
            main_rx,
            is_busy: true,
            status_message: None,
            logs: String::from("No logs loaded"),
            last_log_request: Instant::now(),
            show_full_logs: false,
            log_highlight_query: String::new(),
            is_entering_highlight: false,
            log_scroll: 0,
            auto_scroll: true,
            last_nav_time: Instant::now(),
        };
        app.request_refresh();
        app
    }

    fn request_refresh(&mut self) {
        self.is_busy = true;
        let _ = self.worker_tx.send(WorkerMsg::RefreshRequest);
    }

    fn check_for_updates(&mut self) {
        while let Ok(msg) = self.main_rx.try_recv() {
            match msg {
                MainMsg::UpdateUnits(u, s) => {
                    self.user_services = u;
                    self.system_services = s;
                    self.is_busy = false;
                    self.status_message = None;
                    self.ensure_selection_in_bounds();
                    self.request_logs();
                }
                MainMsg::UpdateLogs(l) => {
                    self.logs = l;
                }
                MainMsg::ActionComplete => {
                    self.request_refresh();
                }
            }
        }
        
        if self.last_log_request.elapsed() > Duration::from_secs(2) {
            self.request_logs();
        }
    }

    fn request_logs(&mut self) {
        let filtered = self.get_filtered_services(self.view_mode);
        let state = if self.view_mode == ViewMode::User { &self.user_list_state } else { &self.system_list_state };
        
        if let Some(i) = state.selected() {
            if let Some(s) = filtered.get(i) {
                let _ = self.worker_tx.send(WorkerMsg::FetchLogs {
                    unit_name: s.name.clone(),
                    is_system: self.view_mode == ViewMode::System,
                });
            }
        }
        self.last_log_request = Instant::now();
    }

    fn ensure_selection_in_bounds(&mut self) {
        let user_filtered = self.get_filtered_services(ViewMode::User);
        if let Some(ref name) = self.selected_user_unit {
            if let Some(pos) = user_filtered.iter().position(|u| u.name == *name) {
                self.user_list_state.select(Some(pos));
            } else {
                self.user_list_state.select(if user_filtered.is_empty() { None } else { Some(0) });
            }
        } else if !user_filtered.is_empty() {
            self.user_list_state.select(Some(0));
        }

        let sys_filtered = self.get_filtered_services(ViewMode::System);
        if let Some(ref name) = self.selected_system_unit {
            if let Some(pos) = sys_filtered.iter().position(|u| u.name == *name) {
                self.system_list_state.select(Some(pos));
            } else {
                self.system_list_state.select(if sys_filtered.is_empty() { None } else { Some(0) });
            }
        } else if !sys_filtered.is_empty() {
            self.system_list_state.select(Some(0));
        }

        self.update_selected_names();
    }

    fn update_selected_names(&mut self) {
        let user_filtered = self.get_filtered_services(ViewMode::User);
        if let Some(i) = self.user_list_state.selected() {
            self.selected_user_unit = user_filtered.get(i).map(|u| u.name.clone());
        }

        let sys_filtered = self.get_filtered_services(ViewMode::System);
        if let Some(i) = self.system_list_state.selected() {
            self.selected_system_unit = sys_filtered.get(i).map(|u| u.name.clone());
        }
    }

    fn get_filtered_services_len(&self, mode: ViewMode) -> usize {
        let list = if mode == ViewMode::User { &self.user_services } else { &self.system_services };
        let query = self.search_query.to_lowercase();

        list.iter()
            .filter(|s| {
                let matches_query = query.is_empty() || s.name.to_lowercase().contains(&query) || s.description.to_lowercase().contains(&query);
                let matches_filter = if self.active_filters.is_empty() {
                    true
                } else {
                    self.active_filters.iter().any(|f| s.name.ends_with(f.extension()))
                };
                matches_query && matches_filter
            })
            .count()
    }

    fn get_filtered_services(&self, mode: ViewMode) -> Vec<&ServiceUnit> {
        let list = if mode == ViewMode::User { &self.user_services } else { &self.system_services };
        let query = self.search_query.to_lowercase();

        list.iter()
            .filter(|s| {
                let matches_query = query.is_empty() || s.name.to_lowercase().contains(&query) || s.description.to_lowercase().contains(&query);
                let matches_filter = if self.active_filters.is_empty() {
                    true
                } else {
                    self.active_filters.iter().any(|f| s.name.ends_with(f.extension()))
                };
                matches_query && matches_filter
            })
            .collect()
    }

    fn next(&mut self) {
        if self.show_options {
            let i = match self.options_state.selected() {
                Some(i) => if i >= self.options.len() - 1 { 0 } else { i + 1 },
                None => 0,
            };
            self.options_state.select(Some(i));
        } else if self.show_filters {
            let len = UnitType::all_variants().len();
            let i = match self.filter_list_state.selected() {
                Some(i) => if i >= len - 1 { 0 } else { i + 1 },
                None => 0,
            };
            self.filter_list_state.select(Some(i));
        } else if !self.show_full_logs {
            let filtered_len = self.get_filtered_services_len(self.view_mode);
            let state = if self.view_mode == ViewMode::User { &mut self.user_list_state } else { &mut self.system_list_state };
            let i = match state.selected() {
                Some(i) => if filtered_len == 0 { 0 } else if i >= filtered_len - 1 { 0 } else { i + 1 },
                None => 0,
            };
            state.select(Some(i));
            self.update_selected_names();
            self.auto_scroll = true;
            self.request_logs();
        }
    }

    fn previous(&mut self) {
        if self.show_options {
            let i = match self.options_state.selected() {
                Some(i) => if i == 0 { self.options.len() - 1 } else { i - 1 },
                None => 0,
            };
            self.options_state.select(Some(i));
        } else if self.show_filters {
            let len = UnitType::all_variants().len();
            let i = match self.filter_list_state.selected() {
                Some(i) => if i == 0 { len - 1 } else { i - 1 },
                None => 0,
            };
            self.filter_list_state.select(Some(i));
        } else if !self.show_full_logs {
            let filtered_len = self.get_filtered_services_len(self.view_mode);
            let state = if self.view_mode == ViewMode::User { &mut self.user_list_state } else { &mut self.system_list_state };
            let i = match state.selected() {
                Some(i) => if filtered_len == 0 { 0 } else if i == 0 { filtered_len - 1 } else { i - 1 },
                None => 0,
            };
            state.select(Some(i));
            self.update_selected_names();
            self.auto_scroll = true;
            self.request_logs();
        }
    }

    fn jump_next(&mut self) {
        let filtered_len = self.get_filtered_services_len(self.view_mode);
        if filtered_len == 0 { return; }
        let state = if self.view_mode == ViewMode::User { &mut self.user_list_state } else { &mut self.system_list_state };
        let i = match state.selected() {
            Some(i) => {
                let next = i + 10;
                if next >= filtered_len { filtered_len - 1 } else { next }
            },
            None => 0,
        };
        state.select(Some(i));
        self.update_selected_names();
        self.auto_scroll = true;
        self.request_logs();
    }

    fn jump_previous(&mut self) {
        let filtered_len = self.get_filtered_services_len(self.view_mode);
        if filtered_len == 0 { return; }
        let state = if self.view_mode == ViewMode::User { &mut self.user_list_state } else { &mut self.system_list_state };
        let i = match state.selected() {
            Some(i) => {
                if i < 10 { 0 } else { i - 10 }
            },
            None => 0,
        };
        state.select(Some(i));
        self.update_selected_names();
        self.auto_scroll = true;
        self.request_logs();
    }

    fn toggle_view(&mut self) {
        self.view_mode = match self.view_mode {
            ViewMode::User => ViewMode::System,
            ViewMode::System => ViewMode::User,
        };
        self.show_options = false;
        self.show_filters = false;
        self.show_help = false;
        self.show_full_logs = false;
        self.auto_scroll = true;
        self.request_logs();
    }

    fn toggle_filter(&mut self, unit_type: UnitType) {
        if let Some(pos) = self.active_filters.iter().position(|f| *f == unit_type) {
            self.active_filters.remove(pos);
        } else {
            self.active_filters.push(unit_type);
        }
        self.ensure_selection_in_bounds();
        self.request_logs();
    }
}

fn worker_loop(rx: Receiver<WorkerMsg>, tx: Sender<MainMsg>) {
    loop {
        match rx.recv() {
            Ok(WorkerMsg::RefreshRequest) => {
                let u = fetch_units(true);
                let s = fetch_units(false);
                let _ = tx.send(MainMsg::UpdateUnits(u, s));
            }
            Ok(WorkerMsg::ExecuteAction { action, unit_name, is_system }) => {
                let mut cmd = if is_system { vec!["sudo", "systemctl"] } else { vec!["systemctl", "--user"] };
                match action.as_str() {
                    "Start" => cmd.push("start"),
                    "Stop" => cmd.push("stop"),
                    "Enable" => cmd.push("enable"),
                    "Disable" => cmd.push("disable"),
                    "Reload" => { cmd.push("daemon-reload"); }
                    _ => continue,
                }
                if action != "Reload" { cmd.push(&unit_name); }
                let _ = Command::new(cmd[0]).args(&cmd[1..]).stdout(Stdio::null()).stderr(Stdio::null()).status();
                let _ = tx.send(MainMsg::ActionComplete);
            }
            Ok(WorkerMsg::FetchLogs { unit_name, is_system }) => {
                let mut cmd = Command::new("journalctl");
                if !is_system { cmd.arg("--user"); }
                cmd.args(["-u", &unit_name, "-n", "500", "--no-pager"]);
                if let Ok(output) = cmd.output() {
                    let logs = String::from_utf8_lossy(&output.stdout).to_string();
                    let _ = tx.send(MainMsg::UpdateLogs(if logs.trim().is_empty() { "No logs available".to_string() } else { logs }));
                }
            }
            Err(_) => break,
        }
    }
}

fn fetch_units(user_mode: bool) -> Vec<ServiceUnit> {
    let mut units_map: HashMap<String, ServiceUnit> = HashMap::new();
    let base_args = if user_mode { vec!["--user"] } else { vec![] };

    let mut list_files_args = base_args.clone();
    list_files_args.extend(["list-unit-files", "--all", "--plain", "--no-legend"]);
    if let Ok(output) = Command::new("systemctl").args(&list_files_args).output() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if !parts.is_empty() {
                let name = parts[0].trim_start_matches([' ', '●', '○', '▲', '▼', '*']).to_string();
                if name == "UNIT" || name.is_empty() { continue; }
                units_map.insert(name.clone(), ServiceUnit {
                    name,
                    load: "loaded".to_string(),
                    active: "inactive".to_string(),
                    sub: "dead".to_string(),
                    description: "".to_string(),
                });
            }
        }
    }

    let mut list_units_args = base_args.clone();
    list_units_args.extend(["list-units", "--all", "--plain", "--no-legend"]);
    if let Ok(output) = Command::new("systemctl").args(&list_units_args).output() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.is_empty() { continue; }
            let has_symbol = parts[0].starts_with(['●', '○', '▲', '▼', '*']);
            let offset = if has_symbol { 1 } else { 0 };
            if parts.len() >= 4 + offset {
                let name = parts[offset].to_string();
                if name == "UNIT" || name.is_empty() { continue; }
                units_map.insert(name.clone(), ServiceUnit {
                    name,
                    load: parts[1 + offset].to_string(),
                    active: parts[2 + offset].to_string(),
                    sub: parts[3 + offset].to_string(),
                    description: parts[(4 + offset)..].join(" "),
                });
            }
        }
    }

    let mut result: Vec<ServiceUnit> = units_map.into_values().collect();
    result.sort_by(|a, b| a.name.cmp(&b.name));
    result
}

fn handle_action<B: Backend + io::Write>(
    terminal: &mut Terminal<B>,
    app: &mut App,
    action: &str,
) -> Result<(), Box<dyn Error>> {
    let filtered_len = app.get_filtered_services_len(app.view_mode);
    if filtered_len == 0 { return Ok(()); }

    let (unit_name, is_system) = {
        let filtered = app.get_filtered_services(app.view_mode);
        let state = if app.view_mode == ViewMode::User { &app.user_list_state } else { &app.system_list_state };
        if let Some(i) = state.selected() {
            if let Some(s) = filtered.get(i) {
                (s.name.clone(), app.view_mode == ViewMode::System)
            } else { return Ok(()); }
        } else { return Ok(()); }
    };

    if action == "Edit" {
        disable_raw_mode()?;
        execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
        let mut cmd = if is_system { vec!["sudo", "systemctl", "edit"] } else { vec!["systemctl", "--user", "edit"] };
        cmd.push(&unit_name);
        println!("\x1b[2J\x1b[HOpening Editor for: {}\n", unit_name);
        let _ = Command::new(cmd[0]).args(&cmd[1..]).stdin(Stdio::inherit()).stdout(Stdio::inherit()).stderr(Stdio::inherit()).spawn()?.wait();
        enable_raw_mode()?;
        execute!(terminal.backend_mut(), EnterAlternateScreen, EnableMouseCapture)?;
        terminal.clear().map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
        app.request_refresh();
    } else {
        let verb = match action {
            "Start" => "Starting",
            "Stop" => "Stopping",
            "Enable" => "Enabling",
            "Disable" => "Disabling",
            "Reload" => "Reloading",
            _ => "Executing",
        };
        app.is_busy = true;
        app.status_message = Some(format!("{} {}...", verb, unit_name));
        let _ = app.worker_tx.send(WorkerMsg::ExecuteAction { action: action.to_string(), unit_name, is_system });
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let (worker_tx, worker_rx) = mpsc::channel();
    let (main_tx, main_rx) = mpsc::channel();
    thread::spawn(move || worker_loop(worker_rx, main_tx));
    let mut app = App::new(worker_tx, main_rx);
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let res = run_app(&mut terminal, &mut app);
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.show_cursor()?;
    if let Err(err) = res { println!("{:?}", err) }
    Ok(())
}

fn run_app<B: Backend + io::Write>(terminal: &mut Terminal<B>, app: &mut App) -> Result<(), Box<dyn Error>> {
    loop {
        app.check_for_updates();
        terminal.draw(|f| ui(f, app)).map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;

        if event::poll(Duration::from_millis(50))? {
            while event::poll(Duration::from_millis(0))? {
                if let Event::Key(key) = event::read()? {
                    handle_key_event(terminal, app, key)?;
                }
            }
        }
    }
}

fn handle_key_event<B: Backend + io::Write>(terminal: &mut Terminal<B>, app: &mut App, key: event::KeyEvent) -> Result<(), Box<dyn Error>> {
    let nav_keys = [KeyCode::Up, KeyCode::Down, KeyCode::Char('j'), KeyCode::Char('k'), KeyCode::PageUp, KeyCode::PageDown];
    let is_nav = nav_keys.contains(&key.code);
    
    // Throttle navigation events to 50ms (20 FPS)
    if is_nav && app.last_nav_time.elapsed() < Duration::from_millis(50) {
        return Ok(());
    }
    if is_nav {
        app.last_nav_time = Instant::now();
    }

    if app.is_searching {
        match key.code {
            KeyCode::Enter | KeyCode::Esc => { app.is_searching = false; app.ensure_selection_in_bounds(); }
            KeyCode::Char(c) => { app.search_query.push(c); app.ensure_selection_in_bounds(); }
            KeyCode::Backspace => { app.search_query.pop(); app.ensure_selection_in_bounds(); }
            _ => {}
        }
    } else if app.is_entering_highlight {
        match key.code {
            KeyCode::Enter | KeyCode::Esc => { app.is_entering_highlight = false; }
            KeyCode::Char(c) => { app.log_highlight_query.push(c); }
            KeyCode::Backspace => { app.log_highlight_query.pop(); }
            _ => {}
        }
    } else if app.show_help {
        match key.code {
            KeyCode::Char('?') | KeyCode::Esc | KeyCode::Enter => app.show_help = false,
            _ => {}
        }
    } else if app.show_options {
        match key.code {
            KeyCode::Esc => app.show_options = false,
            KeyCode::Down | KeyCode::Char('j') => app.next(),
            KeyCode::Up | KeyCode::Char('k') => app.previous(),
            KeyCode::Enter => { if let Some(i) = app.options_state.selected() { let action = app.options[i]; handle_action(terminal, app, action)?; app.show_options = false; } }
            _ => {}
        }
    } else if app.show_filters {
        match key.code {
            KeyCode::Esc | KeyCode::Char('f') => app.show_filters = false,
            KeyCode::Down | KeyCode::Char('j') => app.next(),
            KeyCode::Up | KeyCode::Char('k') => app.previous(),
            KeyCode::Enter | KeyCode::Char(' ') => { if let Some(i) = app.filter_list_state.selected() { let unit_type = UnitType::all_variants()[i]; app.toggle_filter(unit_type); } }
            _ => {}
        }
    } else if app.show_full_logs {
        match key.code {
            KeyCode::Char('l') | KeyCode::Esc => { app.show_full_logs = false; }
            KeyCode::Char('h') => { app.is_entering_highlight = true; }
            KeyCode::Char('c') => { app.log_highlight_query.clear(); }
            KeyCode::Down | KeyCode::Char('j') => { app.log_scroll = app.log_scroll.saturating_add(1); app.auto_scroll = false; }
            KeyCode::Up | KeyCode::Char('k') => { app.log_scroll = app.log_scroll.saturating_sub(1); app.auto_scroll = false; }
            KeyCode::PageDown => { app.log_scroll = app.log_scroll.saturating_add(15); app.auto_scroll = false; }
            KeyCode::PageUp => { app.log_scroll = app.log_scroll.saturating_sub(15); app.auto_scroll = false; }
            KeyCode::Char('g') => { app.log_scroll = 0; app.auto_scroll = false; }
            KeyCode::Char('G') => { app.auto_scroll = true; }
            _ => {}
        }
    } else {
        match key.code {
            KeyCode::Char('q') => { std::process::exit(0); }
            KeyCode::Char('?') => app.show_help = true,
            KeyCode::Char('l') => { app.show_full_logs = true; app.auto_scroll = true; app.request_logs(); }
            KeyCode::Esc => { if !app.search_query.is_empty() { app.search_query.clear(); app.ensure_selection_in_bounds(); app.request_logs(); } }
            KeyCode::Char('/') => app.is_searching = true,
            KeyCode::Char('f') => { app.show_filters = true; app.filter_list_state.select(Some(0)); }
            KeyCode::Tab => app.toggle_view(),
            KeyCode::Down | KeyCode::Char('j') => app.next(),
            KeyCode::Up | KeyCode::Char('k') => app.previous(),
            KeyCode::PageDown => app.jump_next(),
            KeyCode::PageUp => app.jump_previous(),
            KeyCode::Char('s') => {
                let action = {
                    let filtered = app.get_filtered_services(app.view_mode);
                    let state = if app.view_mode == ViewMode::User { &app.user_list_state } else { &app.system_list_state };
                    if let Some(i) = state.selected() { filtered.get(i).map(|u| if u.active == "active" { "Stop" } else { "Start" }) } else { None }
                };
                if let Some(act) = action { handle_action(terminal, app, act)?; }
            }
            KeyCode::Char('r') => { handle_action(terminal, app, "Reload")?; }
            KeyCode::Enter => {
                let filtered_len = app.get_filtered_services_len(app.view_mode);
                let state = if app.view_mode == ViewMode::User { &app.user_list_state } else { &app.system_list_state };
                if filtered_len > 0 && state.selected().is_some() { app.show_options = true; app.options_state.select(Some(0)); }
            }
            KeyCode::Char('e') => { handle_action(terminal, app, "Edit")?; }
            _ => {}
        }
    }
    Ok(())
}

fn highlight_text<'a>(text: &'a str, query: &str) -> Line<'a> {
    if query.is_empty() { return Line::from(text.to_string()); }
    if let Ok(re) = Regex::new(&format!("(?i){}", query)) {
        let mut spans = Vec::new();
        let mut last_pos = 0;
        for m in re.find_iter(text) {
            spans.push(Span::raw(text[last_pos..m.start()].to_string()));
            spans.push(Span::styled(text[m.start()..m.end()].to_string(), Style::default().bg(Color::Yellow).fg(Color::Black).add_modifier(Modifier::BOLD)));
            last_pos = m.end();
        }
        spans.push(Span::raw(text[last_pos..].to_string()));
        Line::from(spans)
    } else { Line::from(text.to_string()) }
}

fn ui(f: &mut ratatui::Frame, app: &mut App) {
    let chunks = Layout::default().direction(Direction::Vertical).constraints([Constraint::Min(0), if app.is_searching || !app.search_query.is_empty() { Constraint::Length(3) } else { Constraint::Length(0) }, Constraint::Length(3)].as_ref()).split(f.area());
    let main_chunks = Layout::default().direction(Direction::Horizontal).constraints([Constraint::Percentage(50), Constraint::Percentage(50)].as_ref()).split(chunks[0]);
    let right_chunks = Layout::default().direction(Direction::Vertical).constraints([Constraint::Percentage(30), Constraint::Percentage(70)].as_ref()).split(main_chunks[1]);

    let filtered_services = app.get_filtered_services(app.view_mode);
    let filter_label = if app.active_filters.is_empty() { "All".to_string() } else { app.active_filters.iter().map(|f| f.label()).collect::<Vec<_>>().join(", ") };
    let refreshing_tag = if app.is_busy { " [Busy...]" } else { "" };
    let title = format!("[{}] Units [{}]{}", match app.view_mode { ViewMode::User => "USER", ViewMode::System => "SYSTEM" }, filter_label, refreshing_tag);

    let items: Vec<ListItem> = filtered_services.iter().map(|s| {
        let style = match s.active.as_str() { "active" => Style::default().fg(Color::Green), "failed" => Style::default().fg(Color::Red), _ => Style::default().fg(Color::Gray), };
        ListItem::new(s.name.clone()).style(style)
    }).collect();

    let list = List::new(items).block(Block::default().borders(Borders::ALL).title(title)).highlight_style(Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD)).highlight_symbol(">> ");

    let details = {
        let state = if app.view_mode == ViewMode::User { &app.user_list_state } else { &app.system_list_state };
        if let Some(i) = state.selected() {
            if let Some(s) = filtered_services.get(i) {
                format!("Name: {}\nLoad: {}\nActive: {}\nSub: {}\nDescription: {}", s.name, s.load, s.active, s.sub, s.description)
            } else { "No unit selected".to_string() }
        } else { "No unit selected".to_string() }
    };

    match app.view_mode {
        ViewMode::User => f.render_stateful_widget(list, main_chunks[0], &mut app.user_list_state),
        ViewMode::System => f.render_stateful_widget(list, main_chunks[0], &mut app.system_list_state),
    };

    let details_widget = Paragraph::new(details).block(Block::default().borders(Borders::ALL).title("Details"));
    f.render_widget(details_widget, right_chunks[0]);

    let log_lines: Vec<Line> = app.logs.lines().map(|l| highlight_text(l, &app.log_highlight_query)).collect();
    let line_count = log_lines.len() as u16;
    let mini_area_height = right_chunks[1].height.saturating_sub(2);
    let mini_max_scroll = line_count.saturating_sub(mini_area_height);
    let mut mini_scroll = app.log_scroll;
    if app.auto_scroll { mini_scroll = mini_max_scroll; } else { mini_scroll = mini_scroll.clamp(0, mini_max_scroll); }

    let logs_widget = Paragraph::new(log_lines.clone()).block(Block::default().borders(Borders::ALL).title("Recent Logs")).wrap(Wrap { trim: true }).scroll((mini_scroll, 0));
    f.render_widget(logs_widget, right_chunks[1]);

    if app.is_searching || !app.search_query.is_empty() {
        let search_block = Block::default().borders(Borders::ALL).title("Search");
        let search_paragraph = Paragraph::new(format!(" {}", app.search_query)).block(search_block);
        f.render_widget(search_paragraph, chunks[1]);
    }

    let footer = Paragraph::new("[?] Help  [Tab] Mode  [/] Search  [l] Full Logs [q] Quit").block(Block::default().borders(Borders::ALL).title("Keybindings"));
    f.render_widget(footer, chunks[2]);

    if app.show_full_logs {
        let area = f.area();
        f.render_widget(Clear, area);
        let full_area_height = area.height.saturating_sub(2);
        let max_scroll = line_count.saturating_sub(full_area_height);
        if app.auto_scroll { app.log_scroll = max_scroll; } else { app.log_scroll = app.log_scroll.clamp(0, max_scroll); }
        let title = if app.auto_scroll { format!("Streaming Logs [TAIL] (Press [h] to Highlight, [j/k] to Scroll, [l/Esc] to Close)") } else { format!("Streaming Logs [PAUSED] (Press [G] to Tail, [h] to Highlight, [l/Esc] to Close)") };
        let logs_widget = Paragraph::new(log_lines).block(Block::default().borders(Borders::ALL).title(title)).wrap(Wrap { trim: true }).scroll((app.log_scroll, 0));
        f.render_widget(logs_widget, area);
        if app.is_entering_highlight {
            let popup_area = centered_rect(60, 10, f.area());
            f.render_widget(Clear, popup_area);
            let highlight_input = Paragraph::new(format!(" > {}", app.log_highlight_query)).block(Block::default().borders(Borders::ALL).title("Enter Regex to Highlight"));
            f.render_widget(highlight_input, popup_area);
        }
    }

    if app.show_options {
        let area = centered_rect(30, 30, f.area());
        f.render_widget(Clear, area);
        let items: Vec<ListItem> = app.options.iter().map(|o| ListItem::new(*o)).collect();
        let list = List::new(items).block(Block::default().borders(Borders::ALL).title("Options")).highlight_style(Style::default().bg(Color::Blue).add_modifier(Modifier::BOLD)).highlight_symbol("> ");
        f.render_stateful_widget(list, area, &mut app.options_state);
    }

    if app.show_filters {
        let area = centered_rect(30, 40, f.area());
        f.render_widget(Clear, area);
        let items: Vec<ListItem> = UnitType::all_variants().iter().map(|v| { let is_active = app.active_filters.contains(v); ListItem::new(if is_active { format!("[X] {}", v.label()) } else { format!("[ ] {}", v.label()) }) }).collect();
        let list = List::new(items).block(Block::default().borders(Borders::ALL).title("Toggle Filters")).highlight_style(Style::default().bg(Color::Magenta).add_modifier(Modifier::BOLD)).highlight_symbol("> ");
        f.render_stateful_widget(list, area, &mut app.filter_list_state);
    }

    if app.show_help {
        let area = centered_rect(50, 60, f.area());
        f.render_widget(Clear, area);
        let help_text = vec!["NAVIGATION","  j/k, Down/Up : Move selection","  PgUp/PgDn    : Jump scroll","  Tab          : Switch USER/SYSTEM mode","  /            : Start Search","  f            : Toggle Unit Filters","  l            : Open Full Log View","  ?            : Toggle Help","  q, Esc       : Quit","","LOG VIEW ACTIONS","  j/k, Down/Up : Scroll Logs (Pauses Tail)","  PgUp/PgDn    : Fast Scroll","  G            : Resume Tail (Bottom)","  g            : Go to Top","  h            : Enter Regex Highlight","  c            : Clear Highlight","  l, Esc       : Close Log View","","SERVICE ACTIONS","  Enter        : Open selection menu","  s            : Toggle Start/Stop","  r            : Reload daemon","  e            : Edit unit file",].join("\n");
        let help_widget = Paragraph::new(help_text).block(Block::default().borders(Borders::ALL).title("Help"));
        f.render_widget(help_widget, area);
    }

    if let Some(ref status) = app.status_message {
        let area = centered_rect(40, 10, f.area());
        f.render_widget(Clear, area);
        let status_widget = Paragraph::new(format!("\n  {}", status)).block(Block::default().borders(Borders::ALL).title("Status")).style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));
        f.render_widget(status_widget, area);
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default().direction(Direction::Vertical).constraints([Constraint::Percentage((100 - percent_y) / 2), Constraint::Percentage(percent_y), Constraint::Percentage((100 - percent_y) / 2),].as_ref()).split(r);
    Layout::default().direction(Direction::Horizontal).constraints([Constraint::Percentage((100 - percent_x) / 2), Constraint::Percentage(percent_x), Constraint::Percentage((100 - percent_x) / 2),].as_ref()).split(popup_layout[1])[1]
}
