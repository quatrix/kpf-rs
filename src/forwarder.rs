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

pub async fn start_single(
    resource_type: String,
    resource_name: String,
    resource_port: u16,
    local_port: u16,
    verbose: u8,
) -> Result<()> {
    let (tx, mut rx) = mpsc::channel::<bool>(10);
    let port_forward_status = Arc::new(Mutex::new(false));
    let port_forward_status_clone = port_forward_status.clone();

    // Start HTTP server
    let http_handle = tokio::spawn(async move {
        start_http_server(local_port, resource_port, port_forward_status_clone, verbose).await
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
            
            match create_port_forward(&resource_type, &resource_name, resource_port, local_port).await {
                Ok(mut pf) => {
                    {
                        let mut status = port_forward_status.lock().unwrap();
                        *status = true;
                    }
                    
                    cli::print_forwarding_status(
                        &format!("{}/{}", resource_type, resource_name),
                        local_port,
                        resource_port,
                        true,
                    );
                    
                    // Wait for port-forward to complete or fail
                    let result = pf.await;
                    
                    {
                        let mut status = port_forward_status.lock().unwrap();
                        *status = false;
                    }
                    
                    cli::print_forwarding_status(
                        &format!("{}/{}", resource_type, resource_name),
                        local_port,
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
