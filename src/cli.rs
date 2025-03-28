use colored::*;

pub fn print_startup_banner() {
    println!(
        "{}",
        "╔════════════════════════════════════════════╗".bright_blue()
    );
    println!(
        "{} {}                                  {}",
        "║".bright_blue(),
        "🚀 K8s Port Forward".bright_green(),
        "║".bright_blue()
    );
    println!(
        "{}",
        "╚════════════════════════════════════════════╝".bright_blue()
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
