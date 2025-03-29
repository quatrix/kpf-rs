use anyhow::Result;
use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyModifiers,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    prelude::*,
    text::{Line, Span},
    widgets::{
        Block, Borders, Clear, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap,
    },
    Frame,
};
use std::io;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};
use textwrap;

// New struct: ForwardStatus holds the state for a port-forward
#[derive(Clone)]
pub struct ForwardStatus {
    pub resource: String,
    pub local_port: u16,
    pub state: String,
    pub last_probe: Option<String>,
}

pub struct LogEntry {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub message: String,
    pub level: LogLevel,
}

#[derive(Clone, Copy, PartialEq)]
pub enum LogLevel {
    Info,
    Success,
    Warning,
    Error,
}

pub struct App {
    logs: Vec<LogEntry>,
    log_receiver: mpsc::Receiver<LogEntry>,
    should_quit: bool,
    scroll: usize,
    auto_scroll: bool,
    log_scroll_state: ScrollbarState,
    awaiting_verbosity_input: bool,
    pub forward_statuses: Vec<ForwardStatus>,
    // Search state
    search_mode: bool,
    search_query: String,
    search_results: Vec<usize>, // Stores indices of matching log lines
    current_search_result_index: Option<usize>, // Index into search_results
}

impl App {
    pub fn new(log_receiver: mpsc::Receiver<LogEntry>) -> Self {
        Self {
            logs: Vec::new(),
            log_receiver,
            should_quit: false,
            scroll: 0,
            auto_scroll: true,
            log_scroll_state: ScrollbarState::default(),
            awaiting_verbosity_input: false,
            forward_statuses: Vec::new(),
            // Search state init
            search_mode: false,
            search_query: String::new(),
            search_results: Vec::new(),
            current_search_result_index: None,
        }
    }

    pub fn on_tick(&mut self) {
        // Process any new log messages
        let mut received_logs = false;

        // Try to receive all pending log messages
        while let Ok(log) = self.log_receiver.try_recv() {
            self.logs.push(log);
            received_logs = true;
        }

        // Auto-scroll to bottom if enabled and we received new logs
        if received_logs && self.auto_scroll {
            self.scroll_to_bottom();
        }

        if let Ok(statuses) = crate::forwarder::FORWARD_STATUSES.lock() {
            self.forward_statuses = statuses.values().cloned().collect();
        }
    }

    pub fn quit(&mut self) {
        self.should_quit = true;
    }

    pub fn should_quit(&self) -> bool {
        self.should_quit
    }

    pub fn scroll_up(&mut self) {
        if self.scroll > 0 {
            self.scroll -= 1;
            self.auto_scroll = false;
        }
    }

    pub fn scroll_down(&mut self) {
        self.scroll += 1;
        self.auto_scroll = false;
    }

    pub fn page_up(&mut self, page_size: usize) {
        if self.scroll > page_size {
            self.scroll -= page_size;
        } else {
            self.scroll = 0;
        }
        self.auto_scroll = false;
    }

    pub fn page_down(&mut self, page_size: usize) {
        self.scroll += page_size;
        self.auto_scroll = false;
    }

    pub fn scroll_to_bottom(&mut self) {
        if !self.logs.is_empty() {
            // Set scroll to a very large value
            // This will be capped to the maximum valid scroll position in the UI rendering
            self.scroll = usize::MAX / 2; // Using a large value that will be capped
        } else {
            self.scroll = 0;
        }
        self.auto_scroll = true;
    }

    pub fn scroll_to_top(&mut self) {
        self.scroll = 0;
        self.auto_scroll = false;
    }

    pub fn toggle_auto_scroll(&mut self) {
        self.auto_scroll = !self.auto_scroll;
        if self.auto_scroll {
            self.scroll_to_bottom();
        }
    }

    // --- Search Methods ---

    fn enter_search_mode(&mut self) {
        self.search_mode = true;
        self.search_query.clear();
        self.search_results.clear();
        self.current_search_result_index = None;
    }

    fn exit_search_mode(&mut self) {
        self.search_mode = false;
        // Keep query and results for 'n'/'N' navigation
    }

    fn cancel_search(&mut self) {
        self.search_mode = false;
        self.search_query.clear();
        self.search_results.clear();
        self.current_search_result_index = None;
    }

    fn update_search_results(&mut self) {
        self.search_results.clear();
        self.current_search_result_index = None;
        if self.search_query.is_empty() {
            return;
        }
        for (index, log_entry) in self.logs.iter().enumerate() {
            // Simple case-sensitive search for now
            if log_entry.message.contains(&self.search_query) {
                self.search_results.push(index);
            }
        }
    }

    fn jump_to_result(&mut self, result_index: usize, viewport_height: usize) {
        if self.search_results.is_empty() {
            self.current_search_result_index = None;
            return;
        }

        let actual_result_index = result_index % self.search_results.len();
        self.current_search_result_index = Some(actual_result_index);

        if let Some(log_line_index) = self.search_results.get(actual_result_index) {
            // Try to center the result line in the viewport
            let target_scroll = log_line_index.saturating_sub(viewport_height / 2);
            self.scroll = target_scroll;
            self.auto_scroll = false; // Disable auto-scroll when jumping
        }
    }

    fn jump_to_next_result(&mut self, viewport_height: usize) {
        if self.search_results.is_empty() {
            return;
        }
        let next_index = match self.current_search_result_index {
            Some(current) => (current + 1) % self.search_results.len(),
            None => 0, // Start from the first result if none is selected
        };
        self.jump_to_result(next_index, viewport_height);
    }

    fn jump_to_previous_result(&mut self, viewport_height: usize) {
        if self.search_results.is_empty() {
            return;
        }
        let prev_index = match self.current_search_result_index {
            Some(current) => {
                if current == 0 {
                    self.search_results.len() - 1
                } else {
                    current - 1
                }
            }
            None => 0, // Start from the first result if none is selected
        };
        self.jump_to_result(prev_index, viewport_height);
    }
}

pub fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    tick_rate: Duration,
) -> Result<()> {
    let mut last_tick = Instant::now();

    loop {
        // Calculate viewport height for scrolling/jumping logic BEFORE drawing
        let size = terminal.size()?;
        // Assuming status panel (5), command panel (1), and borders (2) for logs panel
        let log_viewport_height = size.height.saturating_sub(5 + 1 + 2);

        terminal.draw(|f| ui(f, app))?;

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));

        if crossterm::event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                if app.search_mode {
                    // --- Search Mode Input Handling ---
                    match key.code {
                        KeyCode::Enter => {
                            app.exit_search_mode();
                            // Jump to the first result after confirming search
                            app.jump_to_result(0, log_viewport_height as usize);
                        }
                        KeyCode::Esc => {
                            app.cancel_search();
                        }
                        KeyCode::Backspace => {
                            app.search_query.pop();
                            app.update_search_results();
                        }
                        KeyCode::Char(c) => {
                            app.search_query.push(c);
                            app.update_search_results();
                        }
                        _ => {} // Ignore other keys in search mode for now
                    }
                } else if app.awaiting_verbosity_input {
                    // --- Verbosity Input Handling ---
                    match key.code {
                        KeyCode::Char(c) if c >= '0' && c <= '3' => {
                            let new_level = c.to_digit(10).unwrap() as u8;
                            app.awaiting_verbosity_input = false;
                            crate::http::set_verbose(new_level);
                            crate::logger::log_info(format!("Verbosity updated to {}", new_level));
                        }
                        KeyCode::Esc => {
                            // Allow Esc to cancel verbosity change
                            app.awaiting_verbosity_input = false;
                            crate::logger::log_info("Verbosity change cancelled".to_string());
                        }
                        _ => {
                            // Keep awaiting input on invalid key
                            crate::logger::log_warning(
                                "Invalid verbosity level. Enter 0-3 or Esc to cancel.".to_string(),
                            );
                        }
                    }
                } else {
                    // --- Normal Mode Input Handling ---
                    match key.code {
                        KeyCode::Char('q') => app.quit(),
                        KeyCode::Esc => {
                            // Esc can also quit in normal mode
                            app.quit();
                        }
                        KeyCode::Up | KeyCode::Char('k') => app.scroll_up(),
                        KeyCode::Down | KeyCode::Char('j') => app.scroll_down(),
                        KeyCode::PageUp => {
                            app.page_up(log_viewport_height as usize);
                        }
                        KeyCode::PageDown => {
                            app.page_down(log_viewport_height as usize);
                        }
                        KeyCode::Home => app.scroll_to_top(),
                        KeyCode::End => app.scroll_to_bottom(),
                        KeyCode::Char('a') => app.toggle_auto_scroll(),
                        KeyCode::Char('v') => {
                            app.awaiting_verbosity_input = true;
                            crate::logger::log_info(
                                "Enter new verbosity level (0-3) or Esc to cancel:".to_string(),
                            );
                        }
                        KeyCode::Char('/') => {
                            app.enter_search_mode();
                        }
                        KeyCode::Char('n') => {
                            // Check for Shift modifier for 'N'
                            if key.modifiers.contains(KeyModifiers::SHIFT) {
                                app.jump_to_previous_result(log_viewport_height as usize);
                            } else {
                                app.jump_to_next_result(log_viewport_height as usize);
                            }
                        }
                        // Explicitly handle Shift+N if needed, though 'n' with SHIFT modifier covers it
                        // KeyCode::Char('N') => { // This typically requires checking modifiers
                        //     app.jump_to_previous_result(log_viewport_height as usize);
                        // }
                        _ => {}
                    }
                }
            }
        }

        if last_tick.elapsed() >= tick_rate {
            app.on_tick();
            last_tick = Instant::now();
        }

        if app.should_quit() {
            return Ok(());
        }
    }
}

fn ui(f: &mut Frame, app: &mut App) {
    f.render_widget(Clear, f.area());
    let area = f.area();
    let chunks = Layout::vertical([
        Constraint::Length(5), // fixed height for Status panel
        Constraint::Min(0),    // remaining area for Logs
        Constraint::Length(1), // fixed height for Command panel
    ])
    .split(area);
    render_status_panel(f, app, chunks[0]);
    // Pass viewport height to render_logs_panel for highlighting logic if needed
    // (though jump logic now handles scroll calculation)
    let log_viewport_height = chunks[1].height.saturating_sub(2); // Account for borders
    render_logs_panel(f, app, chunks[1], log_viewport_height);
    render_command_panel(f, app, chunks[2]);
}

fn render_status_panel(f: &mut Frame, app: &mut App, area: Rect) {
    use ratatui::widgets::{Cell, Row, Table};
    let header = Row::new(vec![
        Cell::from("Resource"),
        Cell::from("Local Port"),
        Cell::from("Status"),
        Cell::from("Last Probe"),
    ])
    .style(Style::default().bg(Color::Blue).fg(Color::White))
    .bottom_margin(0);
    let rows: Vec<Row> = app
        .forward_statuses
        .iter()
        .map(|st| {
            Row::new(vec![
                Cell::from(st.resource.clone()),
                Cell::from(st.local_port.to_string()),
                Cell::from(st.state.clone()),
                Cell::from(st.last_probe.clone().unwrap_or_else(|| "N/A".to_string())),
            ])
        })
        .collect();
    let table = Table::new(
        rows,
        &[
            Constraint::Percentage(40),
            Constraint::Length(10),
            Constraint::Length(12),
            Constraint::Percentage(38),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .title("Status")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Magenta)),
    );
    f.render_widget(table, area);
}

fn render_logs_panel(f: &mut Frame, app: &mut App, area: Rect, _viewport_height: u16) {
    f.render_widget(Clear, area);
    // Build log lines with timestamp prefixes and colored messages
    let inner_width = if area.width > 2 {
        area.width - 2
    } else {
        area.width
    } as usize;

    let mut log_lines: Vec<Line> = Vec::new();
    let search_highlight_style = Style::default().bg(Color::Yellow).fg(Color::Black);
    let current_match_highlight_style = Style::default().bg(Color::Rgb(255, 165, 0)); // Orange background

    let current_match_log_index = app
        .current_search_result_index
        .and_then(|idx| app.search_results.get(idx).copied());

    for (log_index, log) in app.logs.iter().enumerate() {
        let time = log.timestamp.format("%H:%M:%S").to_string();
        let prefix = format!("[{}] ", time);
        let prefix_width = prefix.chars().count();

        // Determine base style and color
        let color = match log.level {
            LogLevel::Info => Color::Cyan,
            LogLevel::Success => Color::Green,
            LogLevel::Warning => Color::Yellow,
            LogLevel::Error => Color::Red,
        };
        let base_style = Style::default().fg(color);

        // Determine if this line is a search result
        let is_search_result = app.search_results.contains(&log_index);
        let is_current_match = current_match_log_index == Some(log_index);

        let line_style = if is_current_match {
            current_match_highlight_style
        } else if is_search_result {
            search_highlight_style
        } else {
            base_style // Base style determined by log level
        };

        // Function to create spans for a single line, highlighting search terms
        let create_line_spans = |line_content: &str, style: Style, highlight_style: Style| -> Vec<Span> {
            let mut spans = Vec::new();
            if app.search_query.is_empty() || !line_content.contains(&app.search_query) {
                // No search query or no match in this line, return single span
                spans.push(Span::styled(line_content.to_string(), style));
            } else {
                // Highlight matches
                let mut last_index = 0;
                for (start, part) in line_content.match_indices(&app.search_query) {
                    if start > last_index {
                        spans.push(Span::styled(
                            &line_content[last_index..start],
                            style,
                        ));
                    }
                    spans.push(Span::styled(part, highlight_style));
                    last_index = start + part.len();
                }
                if last_index < line_content.len() {
                    spans.push(Span::styled(&line_content[last_index..], style));
                }
            }
            spans
        };

        // Determine the highlight style to use for matches *on this specific log line*
        let match_highlight_style = if is_current_match {
            current_match_highlight_style
        } else {
            search_highlight_style
        };

        // Split message into lines and apply styling with search highlighting
        let message_lines: Vec<&str> = log.message.split('\n').collect();

        if message_lines.is_empty() || (message_lines.len() == 1 && message_lines[0].is_empty()) {
            // Handle potentially empty log messages
             log_lines.push(Line::from(vec![Span::styled(
                prefix.clone(),
                Style::default().fg(Color::DarkGray),
            )]));
        } else {
            // First line with timestamp
            let mut first_line_spans = vec![Span::styled(prefix.clone(), Style::default().fg(Color::DarkGray))];
            first_line_spans.extend(create_line_spans(
                message_lines[0],
                base_style,
                match_highlight_style,
            ));
            log_lines.push(Line::from(first_line_spans));

            // Subsequent lines indented
            let indent_str = " ".repeat(prefix_width); // Use spaces for indent
            for line_content in message_lines.iter().skip(1) {
                 let mut subsequent_line_spans = vec![Span::raw(indent_str.clone())];
                 subsequent_line_spans.extend(create_line_spans(
                    line_content,
                    base_style,
                    match_highlight_style,
                 ));
                 log_lines.push(Line::from(subsequent_line_spans));
            }
        }
    }

    let total_lines = log_lines.len();
    let available_height = if area.height > 2 {
        area.height - 2
    } else {
        area.height
    } as usize;
    let max_scroll = total_lines.saturating_sub(available_height);
    if app.scroll > max_scroll {
        app.scroll = max_scroll;
    }

    let block = Block::default()
        .title(if app.auto_scroll {
            "Logs (Auto Scroll)"
        } else {
            "Logs (Manual Scroll)"
        })
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let paragraph = Paragraph::new(log_lines)
        .block(block)
        .wrap(Wrap { trim: true })
        .scroll((app.scroll as u16, 0));
    f.render_widget(paragraph, area);

    app.log_scroll_state = app.log_scroll_state.content_length(total_lines);
    app.log_scroll_state = app.log_scroll_state.position(app.scroll);

    let scrollbar_area = Rect {
        x: area.x + area.width - 1,
        y: area.y,
        width: 1,
        height: area.height,
    };
    f.render_stateful_widget(
        Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(Some("↑"))
            .end_symbol(Some("↓")),
        scrollbar_area,
        &mut app.log_scroll_state,
    );
}

fn render_command_panel(f: &mut Frame, app: &mut App, area: Rect) {
    let command_text = if app.search_mode {
        // Display search prompt
        format!("/{}", app.search_query)
    } else if app.awaiting_verbosity_input {
        // Display verbosity prompt
        "Enter verbosity (0-3) or Esc:".to_string()
    } else if !app.search_query.is_empty() && !app.search_results.is_empty() {
        // Display search status if there are results
        let current_num = app.current_search_result_index.map_or(0, |i| i + 1);
        format!(
            "Search '{}': {}/{} | Quit: q | Verbosity: v | Auto-scroll: a | Search: / | Next: n | Prev: N",
            app.search_query,
            current_num,
            app.search_results.len()
        )
    } else if !app.search_query.is_empty() {
        // Display search status if query exists but no results
        format!(
            "Search '{}': Not found | Quit: q | Verbosity: v | Auto-scroll: a | Search: /",
            app.search_query
        )
    } else {
        // Default commands
        "Quit: q | Verbosity: v | Auto-scroll: a | Search: / | Scroll: ↑/↓/PgUp/PgDn/Home/End"
            .to_string()
    };

    let paragraph = Paragraph::new(Span::styled(
        command_text,
        Style::default().fg(Color::White).bg(Color::Blue),
    ))
    .alignment(Alignment::Left);

    // Render cursor in search mode
    if app.search_mode {
        f.set_cursor(
            area.x + 1 + app.search_query.chars().count() as u16, // +1 for the '/'
            area.y,
        )
    }
    f.render_widget(paragraph, area);
}

pub fn setup_terminal() -> Result<Terminal<CrosstermBackend<io::Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let terminal = Terminal::new(backend)?;
    Ok(terminal)
}

pub fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    Ok(())
}

pub fn create_log_channel() -> (mpsc::Sender<LogEntry>, mpsc::Receiver<LogEntry>) {
    mpsc::channel()
}

pub fn spawn_log_collector(_log_sender: mpsc::Sender<LogEntry>) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        // This thread will run until the program exits
        loop {
            // Sleep to avoid busy waiting
            thread::sleep(Duration::from_millis(100));
        }
    })
}
