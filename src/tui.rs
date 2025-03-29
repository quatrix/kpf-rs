use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};
use std::io;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

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
            self.scroll = self.logs.len().saturating_sub(1);
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
        // Calculate the maximum scroll position based on the current terminal size
        let max_scroll = {
            let size = terminal.size()?;
            app.logs.len().saturating_sub(size.height as usize - 2) // -2 for borders
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

    // Create a block for the logs area
    let title = if app.auto_scroll {
        " Logs (Auto-Scroll: ON) "
    } else {
        " Logs (Auto-Scroll: OFF) "
    };

    let logs_block = Block::default()
        .title(title)
        .title_alignment(Alignment::Left)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    // Calculate visible area
    let inner_height = size.height.saturating_sub(2) as usize; // -2 for borders

    // Create the log text
    let log_text = app
        .logs
        .iter()
        .map(|log| {
            let timestamp = log.timestamp.format("%H:%M:%S").to_string();
            let color = match log.level {
                LogLevel::Info => Color::Cyan,
                LogLevel::Success => Color::Green,
                LogLevel::Warning => Color::Yellow,
                LogLevel::Error => Color::Red,
            };
            Line::from(vec![
                Span::styled(
                    format!("[{}] ", timestamp),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(log.message.clone(), Style::default().fg(color)),
            ])
        })
        .collect::<Vec<Line>>();

    // Calculate scroll position
    let scroll_offset = if app.logs.len() > inner_height {
        let max_scroll = app.logs.len() - inner_height;
        app.scroll.min(max_scroll)
    } else {
        0
    };

    // Create a paragraph with the logs
    let logs = Paragraph::new(log_text)
        .block(logs_block)
        .wrap(Wrap { trim: false })
        .scroll((scroll_offset as u16, 0));

    // Render the logs
    f.render_widget(logs, size);

    // If we have logs and are not in auto-scroll mode, show a scroll indicator
    if !app.logs.is_empty() && !app.auto_scroll {
        let scroll_percent = if app.logs.len() <= inner_height {
            100
        } else {
            (scroll_offset * 100) / (app.logs.len() - inner_height)
        };

        let scroll_indicator = format!(" {scroll_percent}% ");
        let scroll_indicator_width = scroll_indicator.len() as u16;

        let scroll_indicator_layout = Rect {
            x: size.width - scroll_indicator_width - 1,
            y: 0,
            width: scroll_indicator_width,
            height: 1,
        };

        let scroll_indicator_paragraph = Paragraph::new(scroll_indicator)
            .style(Style::default().bg(Color::Cyan).fg(Color::Black));

        f.render_widget(scroll_indicator_paragraph, scroll_indicator_layout);
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
