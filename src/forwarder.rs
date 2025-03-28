use crate::cli;
use crate::config::Config;
use crate::http::start_http_server;
use crate::k8s::{create_port_forward, parse_resource};
use anyhow::{Context, Result};
use colored::*;
use futures::future::join_all;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::sleep;

const MAX_RETRY_ATTEMPTS: u32 = 5;
const RETRY_DELAY_MS: u64 = 1000;

use std::net::TcpListener;

fn find_available_port() -> Result<u16> {
    // Bind to port 0 to get an available port from the OS
    let listener = TcpListener::bind("127.0.0.1:0")
        .context("Failed to bind to random port")?;
    let port = listener.local_addr()
        .context("Failed to get local address")?
        .port();
    
    Ok(port)
}

pub async fn start_single(
    resource_type: String,
    resource_name: String,
    resource_port: u16,
    local_port: u16,
    verbose: u8,
    timeout: Option<u64>,
    liveness_probe: Option<String>,
) -> Result<()> {
    let (tx, mut rx) = mpsc::channel::<bool>(10);
    let port_forward_status = Arc::new(Mutex::new(false));
    let child_handle = std::sync::Arc::new(std::sync::Mutex::new(None));
    let port_forward_status_clone = port_forward_status.clone();

    // Find an available port for the internal port-forward
    let internal_port = find_available_port()?;
    println!("{} Using internal port {} for port-forward", "🔌".bright_cyan(), internal_port);

    // Start HTTP server on the user-specified port
    let http_handle = tokio::spawn(async move {
        start_http_server(local_port, internal_port, port_forward_status_clone, verbose).await
    });

    // Start port-forward manager
    let k8s_handle = tokio::spawn(async move {
        let mut attempt = 0;
        
        loop {
            attempt += 1;
            if attempt > 1 {
                cli::print_retry(attempt, MAX_RETRY_ATTEMPTS);
                sleep(Duration::from_millis(RETRY_DELAY_MS)).await;
            }
            
            match create_port_forward(&resource_type, &resource_name, resource_port, internal_port, child_handle.clone()).await {
                Ok(pf) => {
                    {
                        let mut status = port_forward_status.lock().unwrap();
                        *status = true;
                        println!("{} Port-forward status set to ACTIVE (PID: {})", 
                            "🔄".bright_cyan(), 
                            std::process::id());
                    }
                    
                    cli::print_forwarding_status(
                        &format!("{}/{}", resource_type, resource_name),
                        internal_port,
                        resource_port,
                        true,
                    );
                    
                    println!("{} HTTP proxy listening on port {} and forwarding to internal port {}", 
                        "🔄".bright_blue(), 
                        local_port.to_string().bright_green(),
                        internal_port.to_string().bright_yellow());
                    
                    println!("{} Port-forward active, waiting for connection to establish...", "🔄".bright_cyan());
                    
                    // Add a small delay to ensure the port-forward is fully established
                    sleep(Duration::from_millis(500)).await;
                    
                    println!("{} Port-forward ready to accept connections", "✅".bright_green());
                    
                    let pf_future = pf;
                    let result = if let Some(probe_path) = liveness_probe.clone() {
                        use hyper::{Client, Request, Body, StatusCode};
                        let liveness_future = async {
                            loop {
                                sleep(Duration::from_secs(5)).await;
                                let client = Client::new();
                                let url = format!("http://127.0.0.1:{}{}", local_port, probe_path);
                                let req = Request::get(url).body(Body::empty()).unwrap();
                                let timeout_duration = std::time::Duration::from_secs(timeout.unwrap_or(3));
                                let res = tokio::time::timeout(timeout_duration, client.request(req)).await;
                                match res {
                                    Ok(Ok(response)) => {
                                        if response.status() != StatusCode::OK {
                                            break Err(anyhow::anyhow!("Liveness probe returned non-OK status: {}", response.status()));
                                        }
                                    }
                                    _ => break Err(anyhow::anyhow!("Liveness probe request failed or timed out")),
                                }
                            }
                        };
                        tokio::select! {
                            res = pf_future => res,
                            liveness_err = liveness_future => {
                                if let Some(mut child) = child_handle.lock().unwrap().take() {
                                    let _ = child.kill().await;
                                }
                                liveness_err
                            }
                        }
                    } else {
                        pf_future.await
                    };
                    
                    {
                        let mut status = port_forward_status.lock().unwrap();
                        *status = false;
                        println!("{} Port-forward status set to INACTIVE (PID: {})", 
                            "🔄".bright_red(), 
                            std::process::id());
                    }
                    
                    cli::print_forwarding_status(
                        &format!("{}/{}", resource_type, resource_name),
                        internal_port,
                        resource_port,
                        false,
                    );
                    
                    if let Err(e) = result {
                        cli::print_error(&format!("Port-forward failed: {}", e));
                    }
                    
                    // Reset attempt counter on successful connection
                    attempt = 0;
                }
                Err(e) => {
                    cli::print_error(&format!("Failed to create port-forward: {}", e));
                    
                    if attempt >= MAX_RETRY_ATTEMPTS {
                        cli::print_error(&format!("Max retry attempts ({}) reached, giving up", MAX_RETRY_ATTEMPTS));
                        break;
                    }
                }
            }
        }
        
        // Signal HTTP server to shut down
        let _ = tx.send(true).await;
    });

    // Wait for shutdown signal
    if let Some(_) = rx.recv().await {
        println!("{} Shutting down...", "🛑".bright_red());
    }

    // Wait for tasks to complete
    let _ = tokio::join!(http_handle, k8s_handle);
    
    Ok(())
}

pub async fn start_from_config(config: Config) -> Result<()> {
    let verbose = config.verbose.unwrap_or(1);
    let mut handles = Vec::new();
    
    println!("{} Starting {} port-forwards from config", 
        "📋".bright_cyan(), 
        config.forwards.len().to_string().bright_yellow());
    
    for forward in config.forwards {
        let (resource_type, resource_name, resource_port) = parse_resource(&forward.resource)
            .context(format!("Failed to parse resource: {}", forward.resource))?;
        
        let local_port = forward.local_port.unwrap_or(resource_port);
        
        let handle = tokio::spawn(async move {
            if let Err(e) = start_single(
                resource_type,
                resource_name,
                resource_port,
                local_port,
                verbose,
            ).await {
                cli::print_error(&format!("Forward failed: {}", e));
            }
        });
        
        handles.push(handle);
    }
    
    // Wait for all forwards to complete
    join_all(handles).await;
    
    Ok(())
}
