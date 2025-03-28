use clap::Parser;
use colored::*;
use std::path::PathBuf;

mod cli;
mod config;
mod forwarder;
mod http;
mod k8s;
mod logger;

#[derive(Parser, Debug)]
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
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    
    // Initialize logger with verbosity level
    logger::init(args.verbose);
    
    // Print startup banner
    cli::print_startup_banner();
    
    println!("{} Kubernetes port-forward utility", "🚀".bright_green());
    println!("{} Verbosity level: {}", "🔊".bright_yellow(), args.verbose);
    
    if let Some(config_path) = args.config {
        // Load config file and start multiple port-forwards
        // Load config file and start multiple port-forwards
        let mut config = config::load_config(config_path)?;
        config.verbose = Some(args.verbose);
        forwarder::start_from_config(config, args.show_liveness).await?;
    } else if let Some(resource_str) = args.resource {
        // Parse resource string and start single port-forward
        let (resource_type, resource_name, resource_port) = k8s::parse_resource(&resource_str)?;
        // If local_port is not specified, try finding an available one.
        // If that fails, default to resource_port.
        // Note: find_available_port might not be defined yet, assuming it exists or will be added.
        // For now, let's keep the original logic of defaulting to resource_port if None.
        let local_port = args.local_port.unwrap_or(resource_port);

        println!("{} Forwarding {} {}/{} port {} via HTTP proxy on port {}",
            "📡".cyan(),
            resource_type.bright_blue(),
            resource_name.bright_yellow(),
            resource_port.to_string().bright_magenta(),
            resource_port.to_string().bright_magenta(),
            local_port.to_string().bright_green());
        
        forwarder::start_single(
            resource_type,
            resource_name,
            resource_port,
            local_port,
            args.verbose,
            args.timeout,
            args.liveness_probe,
            args.show_liveness,
        ).await?;
    }
    
    Ok(())
}
