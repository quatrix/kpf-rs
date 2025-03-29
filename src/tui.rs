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
}

impl App {
    pub fn new(log_receiver: mpsc::Receiver<LogEntry>) -> Self {
        Self {
            logs: Vec::new(),
            log_receiver,
            should_quit: false,
        }
    }

    pub fn on_tick(&mut self) {
        // Process any new log messages
        while let Ok(log) = self.log_receiver.try_recv() {
            self.logs.push(log);
        }
    }

    pub fn quit(&mut self) {
        self.should_quit = true;
    }

    pub fn should_quit(&self) -> bool {
        self.should_quit
    }
}

pub fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    tick_rate: Duration,
) -> Result<()> {
    let mut last_tick = Instant::now();
    
    loop {
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
    let logs_block = Block::default()
        .title(" Logs ")
        .title_alignment(Alignment::Left)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

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
                Span::styled(format!("[{}] ", timestamp), Style::default().fg(Color::DarkGray)),
                Span::styled(log.message.clone(), Style::default().fg(color)),
            ])
        })
        .collect::<Vec<Line>>();

    // Create a paragraph with the logs
    let logs = Paragraph::new(log_text)
        .block(logs_block)
        .wrap(Wrap { trim: false });

    // Render the logs
    f.render_widget(logs, size);
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
