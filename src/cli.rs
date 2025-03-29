use colored::*;
use crate::logger;

pub fn print_startup_banner() {
    let banner = format!(
        "{}\n{} {}                                  {}\n{}",
        "╔════════════════════════════════════════════╗".bright_blue(),
        "║".bright_blue(),
        "🚀 K8s Port Forward".bright_green(),
        "║".bright_blue(),
        "╚════════════════════════════════════════════╝".bright_blue()
    );
    
    logger::log_info(banner);
}

pub fn print_forwarding_status(resource: &str, local_port: u16, remote_port: u16, alive: bool) {
    let status = if alive {
        "✅ CONNECTED".bright_green()
    } else {
        "❌ DISCONNECTED".bright_red()
    };

    let message = format!(
        "{} Port forward {} → {} {} ({})",
        "🔄".cyan(),
        local_port.to_string().bright_green(),
        remote_port.to_string().bright_yellow(),
        resource.bright_blue(),
        status
    );
    
    if alive {
        logger::log_success(message);
    } else {
        logger::log_error(message);
    }
}

pub fn print_error(message: &str) {
    logger::log_error(message.to_string());
}

pub fn print_retry(attempt: u32, max_attempts: u32) {
    let message = format!(
        "{} Retrying connection ({}/{})",
        "🔄".yellow(),
        attempt.to_string().bright_yellow(),
        max_attempts.to_string()
    );
    
    logger::log_warning(message);
}
