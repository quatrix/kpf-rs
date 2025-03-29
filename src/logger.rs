use colored::*;
use std::sync::{mpsc, Mutex, Once};
use tracing_subscriber::{fmt, EnvFilter};

// Global log sender
static mut LOG_SENDER: Option<mpsc::Sender<crate::tui::LogEntry>> = None;
// Mutex to synchronize log output
static mut LOG_MUTEX: Option<Mutex<()>> = None;
// Ensure initialization happens only once
static INIT: Once = Once::new();

pub fn init(verbose: u8) {
    // Initialize the mutex only once
    INIT.call_once(|| {
        unsafe {
            LOG_MUTEX = Some(Mutex::new(()));
        }
    });

    // Use a fixed log level for the application
    let filter = match verbose {
        0 => "warn",
        1 => "info",
        2 => "debug",
        _ => "trace",
    };

    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(filter));

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
        // Acquire the lock to ensure synchronized logging
        let _guard = if let Some(mutex) = &LOG_MUTEX {
            Some(mutex.lock().unwrap_or_else(|e| {
                eprintln!("Failed to acquire log mutex: {}", e);
                e.into_inner()
            }))
        } else {
            None
        };

        if let Some(sender) = &LOG_SENDER {
            match sender.send(crate::tui::LogEntry {
                timestamp: chrono::Utc::now(),
                message: message.clone(),
                level: crate::tui::LogLevel::Info,
            }) {
                Ok(_) => {},
                Err(e) => eprintln!("Failed to send log to TUI: {}", e),
            }
        } else {
            println!("{} {}", "ℹ️".bright_cyan(), message);
        }
    }
}

pub fn log_success(message: String) {
    unsafe {
        // Acquire the lock to ensure synchronized logging
        let _guard = if let Some(mutex) = &LOG_MUTEX {
            Some(mutex.lock().unwrap_or_else(|e| {
                eprintln!("Failed to acquire log mutex: {}", e);
                e.into_inner()
            }))
        } else {
            None
        };

        if let Some(sender) = &LOG_SENDER {
            match sender.send(crate::tui::LogEntry {
                timestamp: chrono::Utc::now(),
                message: message.clone(),
                level: crate::tui::LogLevel::Success,
            }) {
                Ok(_) => {},
                Err(e) => eprintln!("Failed to send log to TUI: {}", e),
            }
        } else {
            println!("{} {}", "✅".bright_green(), message);
        }
    }
}

pub fn log_warning(message: String) {
    unsafe {
        // Acquire the lock to ensure synchronized logging
        let _guard = if let Some(mutex) = &LOG_MUTEX {
            Some(mutex.lock().unwrap_or_else(|e| {
                eprintln!("Failed to acquire log mutex: {}", e);
                e.into_inner()
            }))
        } else {
            None
        };

        if let Some(sender) = &LOG_SENDER {
            match sender.send(crate::tui::LogEntry {
                timestamp: chrono::Utc::now(),
                message: message.clone(),
                level: crate::tui::LogLevel::Warning,
            }) {
                Ok(_) => {},
                Err(e) => eprintln!("Failed to send log to TUI: {}", e),
            }
        } else {
            println!("{} {}", "⚠️".bright_yellow(), message);
        }
    }
}

pub fn log_error(message: String) {
    unsafe {
        // Acquire the lock to ensure synchronized logging
        let _guard = if let Some(mutex) = &LOG_MUTEX {
            Some(mutex.lock().unwrap_or_else(|e| {
                eprintln!("Failed to acquire log mutex: {}", e);
                e.into_inner()
            }))
        } else {
            None
        };

        if let Some(sender) = &LOG_SENDER {
            match sender.send(crate::tui::LogEntry {
                timestamp: chrono::Utc::now(),
                message: message.clone(),
                level: crate::tui::LogLevel::Error,
            }) {
                Ok(_) => {},
                Err(e) => eprintln!("Failed to send log to TUI: {}", e),
            }
        } else {
            eprintln!("{} {}", "❌".bright_red(), message);
        }
    }
}
