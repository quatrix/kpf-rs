use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Paragraph},
    Frame,
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
}

impl App {
    pub fn new(log_receiver: mpsc::Receiver<LogEntry>) -> Self {
        Self {
            logs: Vec::new(),
            log_receiver,
            should_quit: false,
            scroll: 0,
            auto_scroll: true,
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
        let inner_width = size.width.saturating_sub(2) as usize;
        let inner_height = size.height.saturating_sub(2) as usize;
        let wrapped_lines = get_wrapped_lines(app, inner_width);
        let max_scroll = if wrapped_lines.len() > inner_height {
            wrapped_lines.len().saturating_sub(inner_height)
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

fn ui(f: &mut Frame, app: &App) {
    let size = f.size();

    // Define inner area dimensions (accounting for borders)
    let inner_height = if size.height > 2 { size.height - 2 } else { size.height } as usize;
    let inner_width = if size.width > 2 { size.width - 2 } else { size.width } as usize;

    // Create a header with auto/manual scroll status
    let title = if app.auto_scroll {
        " Logs (Auto Scroll) "
    } else {
        " Logs (Manual Scroll) "
    };
    let logs_block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title_alignment(Alignment::Left);

    // Wrap log messages and build display lines
    let mut wrapped_lines = Vec::new();
    for log in &app.logs {
        let ts = log.timestamp.format("%H:%M:%S").to_string();
        let prefix = format!("[{}] ", ts);
        let prefix_width = prefix.chars().count();
        let indent = " ".repeat(prefix_width);
        let wrap_options = Options::new(inner_width)
            .initial_indent("")
            .subsequent_indent(&indent);
        let wrapped = textwrap::wrap(&log.message, wrap_options);
        let color = match log.level {
            LogLevel::Info => Color::Cyan,
            LogLevel::Success => Color::Green,
            LogLevel::Warning => Color::Yellow,
            LogLevel::Error => Color::Red,
        };
        if !wrapped.is_empty() {
            wrapped_lines.push(Line::from(vec![
                Span::styled(prefix.clone(), Style::default().fg(Color::DarkGray)),
                Span::styled(wrapped[0].clone(), Style::default().fg(color)),
            ]));
            for line in wrapped.iter().skip(1) {
                wrapped_lines.push(Line::from(vec![
                    Span::styled(" ".repeat(prefix_width), Style::default()),
                    Span::styled(line.clone(), Style::default().fg(color)),
                ]));
            }
        } else {
            wrapped_lines.push(Line::from(vec![
                Span::styled(prefix, Style::default().fg(Color::DarkGray)),
            ]));
        }
    }

    // Determine scroll offset based on auto-scroll or manual control
    let total_lines = wrapped_lines.len();
    let max_scroll = if total_lines > inner_height { total_lines - inner_height } else { 0 };
    let scroll_offset = if app.auto_scroll {
        max_scroll
    } else {
        app.scroll.min(max_scroll)
    };

    // Slice the wrapped lines to get visible portion
    let visible_lines: Vec<Line> = wrapped_lines
        .iter()
        .skip(scroll_offset)
        .take(inner_height)
        .cloned()
        .collect();

    // Create and render the Paragraph widget
    let paragraph = Paragraph::new(visible_lines)
        .block(logs_block);
    f.render_widget(paragraph, size);

    // Display scroll indicator if in manual scroll mode
    if !app.auto_scroll && total_lines > inner_height {
        let percent = if max_scroll > 0 { (scroll_offset * 100) / max_scroll } else { 100 };
        let indicator = format!(" {}% ", percent);
        let ind_rect = Rect {
            x: size.x + size.width - indicator.len() as u16 - 1,
            y: size.y,
            width: indicator.len() as u16,
            height: 1,
        };
        let ind_paragraph = Paragraph::new(indicator)
            .style(Style::default().bg(Color::Cyan).fg(Color::Black));
        f.render_widget(ind_paragraph, ind_rect);
    }
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

fn get_wrapped_lines(app: &App, inner_width: usize) -> Vec<Line> {
    let mut wrapped_lines = Vec::new();
    for log in &app.logs {
        let timestamp = log.timestamp.format("%H:%M:%S").to_string();
        let timestamp_prefix = format!("[{}] ", timestamp);
        let prefix_width = timestamp_prefix.chars().count();
        let indent = " ".repeat(prefix_width);
        let wrap_options = Options::new(inner_width)
            .initial_indent("")
            .subsequent_indent(&indent);
        let wrapped_message = textwrap::wrap(&log.message, wrap_options);
        let color = match log.level {
            LogLevel::Info => Color::Cyan,
            LogLevel::Success => Color::Green,
            LogLevel::Warning => Color::Yellow,
            LogLevel::Error => Color::Red,
        };
        if !wrapped_message.is_empty() {
            wrapped_lines.push(Line::from(vec![
                Span::styled(timestamp_prefix.clone(), Style::default().fg(Color::DarkGray)),
                Span::styled(wrapped_message[0].clone(), Style::default().fg(color)),
            ]));
            for line in wrapped_message.iter().skip(1) {
                wrapped_lines.push(Line::from(vec![
                    Span::styled(" ".repeat(prefix_width), Style::default()),
                    Span::styled(line.clone(), Style::default().fg(color)),
                ]));
            }
        } else {
            wrapped_lines.push(Line::from(vec![
                Span::styled(timestamp_prefix.clone(), Style::default().fg(Color::DarkGray)),
            ]));
        }
    }
    wrapped_lines
}
