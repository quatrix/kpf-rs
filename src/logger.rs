use colored::*;
use tracing_subscriber::{fmt, EnvFilter};

pub fn init(_verbose: u8) {
    // Use a fixed log level for the application
    let filter = "info";
    
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(filter));
    
    fmt::fmt()
        .with_env_filter(env_filter)
        .with_target(false)
        .init();
    
    println!("{} Log level set to {}", "📝".bright_cyan(), filter.bright_blue());
}
