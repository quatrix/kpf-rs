use colored::*;
use std::sync::mpsc;
use tracing_subscriber::{fmt, EnvFilter};

// Global log sender
static mut LOG_SENDER: Option<mpsc::Sender<crate::tui::LogEntry>> = None;

pub fn init(verbose: u8) {
    // Use a fixed log level for the application
    let filter = match verbose {
        0 => "warn",
        1 => "info",
        2 => "debug",
        _ => "trace",
    };
    
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(filter));
    
    // Configure tracing to use a no-op subscriber when in TUI mode
    // This prevents tracing logs from interfering with our custom logging
    let is_tui_mode = unsafe { LOG_SENDER.is_some() };
    
    if !is_tui_mode {
        fmt::fmt()
            .with_env_filter(env_filter)
            .with_target(false)
            .init();
    }
    
    log_info(format!("📝 Log level set to {}", filter));
}

pub fn set_log_sender(sender: mpsc::Sender<crate::tui::LogEntry>) {
    unsafe {
        LOG_SENDER = Some(sender);
    }
}

pub fn log_info(message: String) {
    unsafe {
        if let Some(sender) = &LOG_SENDER {
            let _ = sender.send(crate::tui::LogEntry {
                timestamp: chrono::Utc::now(),
                message,
                level: crate::tui::LogLevel::Info,
            });
        } else {
            println!("{} {}", "ℹ️".bright_cyan(), message);
        }
    }
}

pub fn log_success(message: String) {
    unsafe {
        if let Some(sender) = &LOG_SENDER {
            let _ = sender.send(crate::tui::LogEntry {
                timestamp: chrono::Utc::now(),
                message,
                level: crate::tui::LogLevel::Success,
            });
        } else {
            println!("{} {}", "✅".bright_green(), message);
        }
    }
}

pub fn log_warning(message: String) {
    unsafe {
        if let Some(sender) = &LOG_SENDER {
            let _ = sender.send(crate::tui::LogEntry {
                timestamp: chrono::Utc::now(),
                message,
                level: crate::tui::LogLevel::Warning,
            });
        } else {
            println!("{} {}", "⚠️".bright_yellow(), message);
        }
    }
}

pub fn log_error(message: String) {
    unsafe {
        if let Some(sender) = &LOG_SENDER {
            let _ = sender.send(crate::tui::LogEntry {
                timestamp: chrono::Utc::now(),
                message,
                level: crate::tui::LogLevel::Error,
            });
        } else {
            eprintln!("{} {}", "❌".bright_red(), message);
        }
    }
}
