use crate::logger;

pub fn print_startup_banner() {
    let banner = format!(
        "{}\n{} {}                                  {}\n{}",
        "╔════════════════════════════════════════════╗",
        "║",
        "🚀 K8s Port Forward",
        "║",
        "╚════════════════════════════════════════════╝"
    );
    
    logger::log_info(banner);
}

pub fn print_forwarding_status(resource: &str, local_port: u16, remote_port: u16, alive: bool) {
    let status = if alive {
        "✅ CONNECTED"
    } else {
        "❌ DISCONNECTED"
    };

    let message = format!(
        "{} Port forward {} → {} {} ({})",
        "🔄",
        local_port.to_string(),
        remote_port.to_string(),
        resource,
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
        "🔄",
        attempt.to_string(),
        max_attempts.to_string()
    );
    
    logger::log_warning(message);
}
