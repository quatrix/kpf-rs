use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;
use std::time::Duration;

mod config;
mod forwarder;
mod http;
mod k8s;
mod logger;
mod tui;

#[derive(Parser, Debug, Clone)]
#[command(
    name = "k8s-port-forward",
    about = "Kubernetes port-forwarding with improved ergonomics",
    version,
    author
)]
struct Args {
    /// Kubernetes resource to port-forward (format: type/name:port)
    #[arg(help = "Example: pod/my-pod:8080 or service/my-service:80", group = "input")]
    resource: Option<String>,

    /// Local port to listen on. Only used when specifying a single resource.
    #[arg(long, short)]
    local_port: Option<u16>,

    /// Path to JSON config file with multiple port-forwards
    #[arg(long, short, group = "input")]
    config: Option<PathBuf>,

    /// Kubernetes namespace (default: default)
    #[arg(long, default_value = "default")]
    namespace: String,

    /// Verbosity level (0-3)
    #[arg(long, short, default_value = "1")]
    verbose: u8,
    /// Timeout in seconds for the port-forward connection
    #[arg(long)]
    timeout: Option<u64>,
    /// Liveness probe HTTP endpoint path (e.g., /ping)
    #[arg(long)]
    liveness_probe: Option<String>,
    /// Show liveness probe logs (disabled by default)
    #[arg(long, default_value_t = false)]
    show_liveness: bool,
    /// Path to log file for writing requests/responses
    #[arg(long)]
    requests_log_file: Option<PathBuf>,
    /// Verbosity level for requests log file (0-3)
    #[arg(long, default_value = "1")]
    requests_log_verbosity: u8,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    
    // Initialize logger with verbosity level
    logger::init(args.verbose);
    http::set_verbose(args.verbose);
    
    run_tui_mode(args).await
}


async fn run_tui_mode(args: Args) -> Result<()> {
    // Set up the terminal
    let mut terminal = tui::setup_terminal()?;
    
    // Create a channel for logging
    let (log_sender, log_receiver) = tui::create_log_channel();
    
    // Set the log sender in the logger module
    logger::set_log_sender(log_sender.clone());
    
    // Create the app state
    let mut app = tui::App::new(log_receiver);
    
    // Spawn a thread to handle the port forwarding
    let args_clone = args.clone();
    let log_sender_clone = log_sender.clone();
    let _port_forward_handle = tokio::spawn(async move {
        // Log startup information
        log_sender_clone.send(tui::LogEntry {
            timestamp: chrono::Utc::now(),
            message: "🚀 Kubernetes port-forward utility".to_string(),
            level: tui::LogLevel::Info,
        }).unwrap();
        
        log_sender_clone.send(tui::LogEntry {
            timestamp: chrono::Utc::now(),
            message: format!("🔊 Verbosity level: {}", args_clone.verbose),
            level: tui::LogLevel::Info,
        }).unwrap();
        
        // Start the port forwarding based on args
        if let Some(config_path) = args_clone.config {
            // Load config file and start multiple port-forwards
            match config::load_config(config_path) {
                Ok(mut config) => {
                    config.verbose = Some(args_clone.verbose);
                    
                    log_sender_clone.send(tui::LogEntry {
                        timestamp: chrono::Utc::now(),
                        message: format!("📋 Starting {} port-forwards from config", config.forwards.len()),
                        level: tui::LogLevel::Info,
                    }).unwrap();
                    
                    if let Err(e) = forwarder::start_from_config(
                        config, 
                        args_clone.show_liveness, 
                        args_clone.requests_log_file, 
                        args_clone.requests_log_verbosity
                    ).await {
                        log_sender_clone.send(tui::LogEntry {
                            timestamp: chrono::Utc::now(),
                            message: format!("❌ Error starting port-forwards: {}", e),
                            level: tui::LogLevel::Error,
                        }).unwrap();
                    }
                }
                Err(e) => {
                    log_sender_clone.send(tui::LogEntry {
                        timestamp: chrono::Utc::now(),
                        message: format!("❌ Failed to load config: {}", e),
                        level: tui::LogLevel::Error,
                    }).unwrap();
                }
            }
        } else if let Some(resource_str) = args_clone.resource {
            // Parse resource string and start single port-forward
            match k8s::parse_resource(&resource_str) {
                Ok((resource_type, resource_name, resource_port)) => {
                    let local_port = args_clone.local_port.unwrap_or(resource_port);
                    
                    log_sender_clone.send(tui::LogEntry {
                        timestamp: chrono::Utc::now(),
                        message: format!("📡 Forwarding {}/{} port {} via HTTP proxy on port {}", 
                            resource_type, resource_name, resource_port, local_port),
                        level: tui::LogLevel::Info,
                    }).unwrap();
                    
                    if let Err(e) = forwarder::start_single(
                        resource_type,
                        resource_name,
                        resource_port,
                        args_clone.namespace,
                        local_port,
                        args_clone.verbose,
                        args_clone.timeout,
                        args_clone.liveness_probe,
                        args_clone.show_liveness,
                        args_clone.requests_log_file,
                        args_clone.requests_log_verbosity,
                    ).await {
                        log_sender_clone.send(tui::LogEntry {
                            timestamp: chrono::Utc::now(),
                            message: format!("❌ Error starting port-forward: {}", e),
                            level: tui::LogLevel::Error,
                        }).unwrap();
                    }
                }
                Err(e) => {
                    log_sender_clone.send(tui::LogEntry {
                        timestamp: chrono::Utc::now(),
                        message: format!("❌ Failed to parse resource: {}", e),
                        level: tui::LogLevel::Error,
                    }).unwrap();
                }
            }
        } else {
            log_sender_clone.send(tui::LogEntry {
                timestamp: chrono::Utc::now(),
                message: "❌ No resource or config specified".to_string(),
                level: tui::LogLevel::Error,
            }).unwrap();
        }
    });
    
    // Run the app
    let tick_rate = Duration::from_millis(100);
    let res = tui::run_app(&mut terminal, &mut app, tick_rate);
    
    // Restore terminal
    tui::restore_terminal(&mut terminal)?;
    
    // Handle any errors from the app
    if let Err(err) = res {
        logger::log_error(format!("TUI error: {}", err));
    }
    
    Ok(())
}
