use colored::*;
use std::time::Duration;

pub fn print_startup_banner() {
    println!("{}", "╔════════════════════════════════════════════╗".bright_blue());
    println!("{} {}                                  {}", "║".bright_blue(), "🚀 K8s Port Forward".bright_green(), "║".bright_blue());
    println!("{}", "╚════════════════════════════════════════════╝".bright_blue());
}

pub fn print_request(
    method: &str,
    path: &str,
    status: Option<u16>,
    duration: Option<Duration>,
    _verbose: u8,
) {
    let method_colored = match method {
        "GET" => method.bright_green(),
        "POST" => method.bright_yellow(),
        "PUT" => method.bright_blue(),
        "DELETE" => method.bright_red(),
        _ => method.bright_cyan(),
    };

    let status_str = if let Some(status) = status {
        let status_colored = match status {
            200..=299 => status.to_string().bright_green(),
            300..=399 => status.to_string().bright_cyan(),
            400..=499 => status.to_string().bright_yellow(),
            _ => status.to_string().bright_red(),
        };
        format!(" {} ", status_colored)
    } else {
        " ".to_string()
    };

    let duration_str = if let Some(duration) = duration {
        let ms = duration.as_millis();
        let duration_colored = match ms {
            0..=100 => format!("{}ms", ms).bright_green(),
            101..=300 => format!("{}ms", ms).bright_yellow(),
            _ => format!("{}ms", ms).bright_red(),
        };
        format!(" {} ", duration_colored)
    } else {
        " ".to_string()
    };

    println!(
        "{} {} {} {}{}",
        "→".bright_blue(),
        method_colored,
        path,
        status_str,
        duration_str
    );
}

pub fn print_forwarding_status(resource: &str, local_port: u16, remote_port: u16, alive: bool) {
    let status = if alive {
        "✅ CONNECTED".bright_green()
    } else {
        "❌ DISCONNECTED".bright_red()
    };
    
    println!(
        "{} Port forward {} → {} {} ({})",
        "🔄".cyan(),
        local_port.to_string().bright_green(),
        remote_port.to_string().bright_yellow(),
        resource.bright_blue(),
        status
    );
}

pub fn print_error(message: &str) {
    eprintln!("{} {}", "❌".bright_red(), message.bright_red());
}

pub fn print_retry(attempt: u32, max_attempts: u32) {
    println!(
        "{} Retrying connection ({}/{})",
        "🔄".yellow(),
        attempt.to_string().bright_yellow(),
        max_attempts.to_string()
    );
}
