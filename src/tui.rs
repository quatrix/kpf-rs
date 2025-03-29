use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Paragraph, Wrap, Scrollbar, ScrollbarOrientation, ScrollbarState},
    Frame,
    text::{Span, Line},
};
use std::io;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};
use textwrap::{self, Options};

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

    pub fn scroll_down(&mut self, max_scroll: usize) {
        if self.scroll < max_scroll {
            self.scroll += 1;
            // If we've scrolled to the bottom, re-enable auto-scroll
            if self.scroll >= max_scroll {
                self.auto_scroll = true;
            }
        }
    }

    pub fn page_up(&mut self, page_size: usize) {
        if self.scroll > page_size {
            self.scroll -= page_size;
        } else {
            self.scroll = 0;
        }
        self.auto_scroll = false;
    }

    pub fn page_down(&mut self, page_size: usize, max_scroll: usize) {
        if self.scroll + page_size < max_scroll {
            self.scroll += page_size;
        } else {
            self.scroll = max_scroll;
            self.auto_scroll = true;
        }
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
}

pub fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    tick_rate: Duration,
) -> Result<()> {
    let mut last_tick = Instant::now();

    loop {
        let size = terminal.size()?;
        let available_height = if size.height > 2 { size.height - 2 } else { size.height } as usize;
        let max_scroll = if app.logs.len() > available_height {
            app.logs.len() - available_height
        } else {
            0
        };

        terminal.draw(|f| ui(f, app))?;

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));

        if crossterm::event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') => {
                            app.quit();
                        }
                        KeyCode::Esc => {
                            app.quit();
                        }
                        KeyCode::Up => {
                            app.scroll_up();
                        }
                        KeyCode::Down => {
                            app.scroll_down(max_scroll);
                        }
                        KeyCode::PageUp => {
                            let page_size = terminal.size()?.height as usize / 2;
                            app.page_up(page_size);
                        }
                        KeyCode::PageDown => {
                            let page_size = terminal.size()?.height as usize / 2;
                            app.page_down(page_size, max_scroll);
                        }
                        KeyCode::Home => {
                            app.scroll_to_top();
                        }
                        KeyCode::End => {
                            app.scroll_to_bottom();
                        }
                        KeyCode::Char('a') => {
                            app.toggle_auto_scroll();
                        }
                        KeyCode::Char('j') => {
                            app.scroll_down(max_scroll);
                        }
                        KeyCode::Char('k') => {
                            app.scroll_up();
                        }
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
    let area = f.size();
    render_logs_panel(f, app, area);
}

fn render_logs_panel(f: &mut Frame, app: &mut App, area: Rect) {
    // Build log lines with timestamp prefixes and colored messages, wrapping long messages into multiple lines
    let inner_width = if area.width > 2 { area.width - 2 } else { area.width } as usize;
    let mut log_lines: Vec<Line> = Vec::new();
    for log in &app.logs {
        let time = log.timestamp.format("%H:%M:%S").to_string();
        let prefix = format!("[{}] ", time);
        let prefix_width = prefix.chars().count();
        let color = match log.level {
            LogLevel::Info => Color::Cyan,
            LogLevel::Success => Color::Green,
            LogLevel::Warning => Color::Yellow,
            LogLevel::Error => Color::Red,
        };
        let wrapped = textwrap::wrap(&log.message, inner_width.saturating_sub(prefix_width));
        if wrapped.is_empty() {
            log_lines.push(Line::from(vec![
                Span::styled(prefix.clone(), Style::default().fg(Color::DarkGray))
            ]));
        } else {
            log_lines.push(Line::from(vec![
                Span::styled(prefix.clone(), Style::default().fg(Color::DarkGray)),
                Span::styled(wrapped[0].to_string(), Style::default().fg(color)),
            ]));
            let indent = " ".repeat(prefix_width);
            for line in wrapped.iter().skip(1) {
                log_lines.push(Line::from(vec![
                    Span::raw(indent.clone()),
                    Span::styled(line.to_string(), Style::default().fg(color)),
                ]));
            }
        }
    }

    // Determine available height and compute scroll offset
    let available_height = if area.height > 2 { area.height - 2 } else { area.height } as usize;
    let total_lines = log_lines.len();
    let scroll = if app.auto_scroll {
        total_lines.saturating_sub(available_height)
    } else {
        app.scroll.min(total_lines.saturating_sub(available_height))
    };

    let visible_lines: Vec<Line> = log_lines.iter()
        .skip(scroll)
        .take(available_height)
        .cloned()
        .collect();

    let block = Block::default()
        .title(if app.auto_scroll { "Logs (Auto Scroll)" } else { "Logs (Manual Scroll)" })
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let paragraph = Paragraph::new(visible_lines)
        .block(block)
        .wrap(Wrap { trim: true });
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

