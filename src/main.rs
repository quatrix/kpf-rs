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
    #[arg(help = "Example: pod/my-pod:8080 or service/my-service:80")]
    resource: String,

    /// Local port to listen on
    #[arg(long, short)]
    local_port: Option<u16>,

    /// Path to JSON config file with multiple port-forwards
    #[arg(long, short)]
    config: Option<PathBuf>,

    /// Verbosity level (1-4)
    #[arg(long, short, default_value = "1")]
    verbose: u8,
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
        let config = config::load_config(config_path)?;
        forwarder::start_from_config(config).await?;
    } else {
        // Parse resource string and start single port-forward
        let (resource_type, resource_name, resource_port) = k8s::parse_resource(&args.resource)?;
        let local_port = args.local_port.unwrap_or(resource_port);
        
        println!("{} Forwarding {} {}/{} port {} to local port {}", 
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
        ).await?;
    }
    
    Ok(())
}
