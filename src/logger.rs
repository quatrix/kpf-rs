use std::sync::{mpsc, Mutex};
use tracing_subscriber::{fmt, EnvFilter};

#[macro_use]
extern crate lazy_static;

lazy_static! {
    static ref LOG_SENDER: Mutex<Option<mpsc::Sender<crate::tui::LogEntry>>> = Mutex::new(None);
}

pub fn init(_verbose: u8) {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("error"));

    let is_tui_mode = LOG_SENDER.lock().unwrap().is_some();

    if !is_tui_mode {
        fmt::fmt()
            .with_env_filter(env_filter)
            .with_target(false)
            .init();
    }
}

pub fn set_log_sender(sender: mpsc::Sender<crate::tui::LogEntry>) {
    *LOG_SENDER.lock().unwrap() = Some(sender);
}

pub fn log_info(message: String) {
    if let Some(sender) = LOG_SENDER.lock().unwrap().clone() {
        if let Err(e) = sender.send(crate::tui::LogEntry {
            timestamp: chrono::Utc::now(),
            message: message.clone(),
            level: crate::tui::LogLevel::Info,
        }) {
            eprintln!("Failed to send log to TUI: {}", e);
        }
    } else {
        println!("{} {}", "ℹ️", message);
    }
}

pub fn log_success(message: String) {
    if let Some(sender) = LOG_SENDER.lock().unwrap().clone() {
        if let Err(e) = sender.send(crate::tui::LogEntry {
            timestamp: chrono::Utc::now(),
            message: message.clone(),
            level: crate::tui::LogLevel::Success,
        }) {
            eprintln!("Failed to send log to TUI: {}", e);
        }
    } else {
        println!("{} {}", "✅", message);
    }
}

pub fn log_warning(message: String) {
    if let Some(sender) = LOG_SENDER.lock().unwrap().clone() {
        if let Err(e) = sender.send(crate::tui::LogEntry {
            timestamp: chrono::Utc::now(),
            message: message.clone(),
            level: crate::tui::LogLevel::Warning,
        }) {
            eprintln!("Failed to send log to TUI: {}", e);
        }
    } else {
        println!("{} {}", "⚠️", message);
    }
}

pub fn log_error(message: String) {
    if let Some(sender) = LOG_SENDER.lock().unwrap().clone() {
        if let Err(e) = sender.send(crate::tui::LogEntry {
            timestamp: chrono::Utc::now(),
            message: message.clone(),
            level: crate::tui::LogLevel::Error,
        }) {
            eprintln!("Failed to send log to TUI: {}", e);
        }
    } else {
        eprintln!("{} {}", "❌", message);
    }
}
