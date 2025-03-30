use crate::config::Config;
use crate::http::start_http_server;
use crate::k8s::{create_port_forward, parse_resource};
use anyhow::{Context, Result};
use futures::future::join_all;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::sleep;

const MAX_RETRY_ATTEMPTS: u32 = 5;
const RETRY_DELAY_MS: u64 = 1000;

use std::net::TcpListener;

use std::collections::HashMap;
use std::sync::LazyLock;
pub static FORWARD_STATUSES: LazyLock<Mutex<HashMap<String, crate::tui::ForwardStatus>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

fn find_available_port() -> Result<u16> {
    // Bind to port 0 to get an available port from the OS
    let listener = TcpListener::bind("127.0.0.1:0").context("Failed to bind to random port")?;
    let port = listener
        .local_addr()
        .context("Failed to get local address")?
        .port();

    Ok(port)
}

pub async fn start_single(
    resource_type: String,
    resource_name: String,
    resource_port: u16,
    namespace: String,
    local_port: u16,
    _verbose: u8,
    timeout: Option<u64>,
    liveness_probe: Option<String>,
    show_liveness: bool,
    requests_log_file: Option<std::path::PathBuf>,
    requests_log_verbosity: u8,
) -> Result<()> {
    let (tx, mut rx) = mpsc::channel::<bool>(10);
    let port_forward_status = Arc::new(Mutex::new(false));
    let child_handle = std::sync::Arc::new(tokio::sync::Mutex::new(None));
    let port_forward_status_clone = port_forward_status.clone();

    // Find an available port for the internal port-forward
    let internal_port = find_available_port()?;
    crate::logger::log_info(format!(
        "{} Using internal port {} for port-forward",
        "🔌", internal_port
    ));

    // Start HTTP server on the user-specified port
    let resource_prefix = format!("{}/{}:{}", resource_type, resource_name, resource_port);
    let http_handle = tokio::spawn(async move {
        start_http_server(
            local_port,
            internal_port,
            port_forward_status_clone,
            show_liveness,
            resource_prefix,
            requests_log_file.clone(),
            requests_log_verbosity,
        )
        .await
    });

    // Start port-forward manager
    let k8s_handle = tokio::spawn(async move {
        let mut attempt = 0;

        loop {
            attempt += 1;
            if attempt > 1 {
                sleep(Duration::from_millis(RETRY_DELAY_MS)).await;
            }

            match create_port_forward(
                &resource_type,
                &resource_name,
                resource_port,
                internal_port,
                &namespace,
                child_handle.clone(),
            )
            .await
            {
                Ok(pf) => {
                    {
                        let mut status = port_forward_status.lock().unwrap();
                        *status = true;
                        crate::logger::log_info(format!(
                            "{} Port-forward status set to ACTIVE (PID: {})",
                            "🔄",
                            std::process::id()
                        ));
                        {
                            use crate::tui::ForwardStatus;
                            let mut statuses = FORWARD_STATUSES.lock().unwrap();
                            statuses.insert(format!("{}/{}", resource_type, resource_name), ForwardStatus {
                                resource: format!("{}/{}", resource_type, resource_name),
                                local_port,
                                state: "OPEN".to_string(),
                                last_probe: None,
                            });
                        }
                    }

                    crate::logger::log_info(format!(
                        "{} HTTP proxy listening on port {} and forwarding to internal port {}",
                        "🔄", local_port, internal_port
                    ));

                    crate::logger::log_info(format!(
                        "{} Port-forward active, waiting for first successful probe...",
                        "🔄"
                    ));
                    if let Some(probe_path) = liveness_probe.clone() {
                        use hyper::{Body, Client, Request, StatusCode};
                        let client = Client::new();
                        let per_request_timeout = std::time::Duration::from_secs(timeout.unwrap_or(1));
                        let overall_probe_timeout = std::time::Duration::from_secs(10);
                        let probe_success = match tokio::time::timeout(overall_probe_timeout, async {
                            let mut probe_fail_count = 0;
                            loop {
                                sleep(Duration::from_secs(2)).await;
                                let url = format!("http://127.0.0.1:{}{}", internal_port, probe_path);
                                let req = Request::get(url)
                                    .header("x-internal-probe", "true")
                                    .body(Body::empty())
                                    .unwrap();
                                let res = tokio::time::timeout(per_request_timeout, client.request(req)).await;
                                match res {
                                    Ok(Ok(response)) => {
                                        if response.status() == StatusCode::OK {
                                            crate::logger::log_info("Successful probe received.".to_string());
                                            {
                                                let mut statuses = FORWARD_STATUSES.lock().unwrap();
                                                let key = format!("{}/{}", resource_type, resource_name);
                                                statuses.entry(key).and_modify(|entry| {
                                                    entry.last_probe = Some(chrono::Utc::now().to_rfc3339());
                                                    entry.state = "ACTIVE".to_string();
                                                });
                                            }
                                            return true;
                                        } else if response.status() == StatusCode::SERVICE_UNAVAILABLE {
                                            crate::logger::log_warning("Received 503 from probe. Marking resource as UNAVAILABLE.".to_string());
                                            {
                                                let mut statuses = FORWARD_STATUSES.lock().unwrap();
                                                let key = format!("{}/{}", resource_type, resource_name);
                                                statuses.entry(key).and_modify(|entry| {
                                                    entry.state = "UNAVAILABLE".to_string();
                                                });
                                            }
                                            return false;
                                        } else {
                                            probe_fail_count += 1;
                                            crate::logger::log_warning(format!("Probe returned non-OK status: {}", response.status()));
                                        }
                                    },
                                    _ => {
                                        probe_fail_count += 1;
                                        crate::logger::log_warning("Probe failed or timed out.".to_string());
                                    }
                                }
                                if probe_fail_count > 2 {
                                    {
                                        let mut statuses = FORWARD_STATUSES.lock().unwrap();
                                        let key = format!("{}/{}", resource_type, resource_name);
                                        statuses.entry(key).and_modify(|entry| {
                                            entry.state = "UNAVAILABLE".to_string();
                                        });
                                    }
                                    crate::logger::log_error("Probe failed more than 2 times. Restarting port-forward.".to_string());
                                    break;
                                }
                            }
                            false
                        }).await {
                            Ok(success) => success,
                            Err(_) => {
                                {
                                    let mut statuses = FORWARD_STATUSES.lock().unwrap();
                                    let key = format!("{}/{}", resource_type, resource_name);
                                    statuses.entry(key).and_modify(|entry| {
                                        entry.state = "UNAVAILABLE".to_string();
                                    });
                                }
                                crate::logger::log_error("Probe overall timeout reached. Restarting port-forward.".to_string());
                                false
                            }
                        };
                        if !probe_success {
                            let _ = pf.await;
                            continue;
                        }
                    }
                    crate::logger::log_success(format!(
                        "{} Port-forward ready to accept connections",
                        "✅"
                    ));
                    let result = pf.await;

                    {
                        let mut status = port_forward_status.lock().unwrap();
                        *status = false;
                        crate::logger::log_warning(format!(
                            "{} Port-forward status set to INACTIVE (PID: {})",
                            "🔄",
                            std::process::id()
                        ));
                        {
                            let mut statuses = FORWARD_STATUSES.lock().unwrap();
                            let key = format!("{}/{}", resource_type, resource_name);
                            statuses.entry(key).and_modify(|entry| entry.state = "INACTIVE".to_string());
                        }
                    }

                    if let Err(e) = result {
                        crate::logger::log_error(format!("Port-forward failed: {}", e));
                    }

                    // Reset attempt counter on successful connection
                    attempt = 0;
                }
                Err(e) => {
                    crate::logger::log_error(format!("Failed to create port-forward: {}", e));

                    if attempt >= MAX_RETRY_ATTEMPTS {
                        crate::logger::log_error(format!(
                            "Max retry attempts ({}) reached, giving up",
                            MAX_RETRY_ATTEMPTS
                        ));
                        break;
                    }
                }
            }
        }

        // Signal HTTP server to shut down
        let _ = tx.send(true).await;
    });

    // Wait for shutdown signal
    if (rx.recv().await).is_some() {
        crate::logger::log_warning(format!("{} Shutting down...", "🛑"));
    }

    // Wait for tasks to complete
    let _ = tokio::join!(http_handle, k8s_handle);

    Ok(())
}

pub async fn start_from_config(
    config: Config,
    show_liveness: bool,
    requests_log_file: Option<std::path::PathBuf>,
    requests_log_verbosity: u8,
) -> Result<()> {
    let verbose = config.verbose.unwrap_or(1);
    let mut handles = Vec::new();

    crate::logger::log_info(format!(
        "{} Starting {} port-forwards from config",
        "📋",
        config.forwards.len()
    ));

    {
        use crate::tui::ForwardStatus;
        let mut statuses = FORWARD_STATUSES.lock().unwrap();
        for forward in &config.forwards {
            let (resource_type, resource_name, resource_port) = crate::k8s::parse_resource(&forward.resource)
                .expect(&format!("Failed to parse resource: {}", forward.resource));
            let local_port = forward.local_port.unwrap_or(resource_port);
            statuses.insert(
                format!("{}/{}", resource_type, resource_name),
                ForwardStatus {
                    resource: format!("{}/{}", resource_type, resource_name),
                    local_port,
                    state: "INITIALIZING".to_string(),
                    last_probe: None,
                },
            );
        }
    }
    let requests_log_file_arc = std::sync::Arc::new(requests_log_file.clone());

    for forward in config.forwards {
        let requests_log_file_clone = requests_log_file_arc.clone();
        let (resource_type, resource_name, resource_port) = parse_resource(&forward.resource)
            .context(format!("Failed to parse resource: {}", forward.resource))?;

        let ns = forward.namespace.unwrap_or_else(|| "default".to_string());
        let local_port = forward.local_port.unwrap_or(resource_port);

        let handle = tokio::spawn(async move {
            if let Err(e) = start_single(
                resource_type,
                resource_name,
                resource_port,
                ns,
                local_port,
                verbose,
                forward.timeout,
                forward.liveness_probe,
                show_liveness,
                (*requests_log_file_clone).clone(),
                requests_log_verbosity,
            )
            .await
            {
                crate::logger::log_error(format!("Forward failed: {}", e));
            }
        });

        handles.push(handle);
    }

    // Wait for all forwards to complete
    join_all(handles).await;

    Ok(())
}
