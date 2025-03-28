use colored::*;
use tracing_subscriber::{fmt, EnvFilter};

pub fn init(verbose: u8) {
    let filter = match verbose {
        1 => "info",
        2 => "debug",
        3 | 4 => "trace",
        _ => "info",
    };
    
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(filter));
    
    fmt::fmt()
        .with_env_filter(env_filter)
        .with_target(false)
        .init();
    
    println!("{} Log level set to {}", "📝".bright_cyan(), filter.bright_blue());
}
