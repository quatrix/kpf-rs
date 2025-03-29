use crate::cli;
use anyhow::Result;
use colored::*;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Client, Request, Response, Server, StatusCode};
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Instant;

async fn proxy_request(
    req: Request<Body>,
    target_port: u16,
    port_forward_status: Arc<Mutex<bool>>,
    verbose: u8,
    _show_liveness: bool,
    resource: String,
    requests_log_file: Option<std::path::PathBuf>,
    requests_log_verbosity: u8,
) -> Result<Response<Body>, hyper::Error> {
    let start = Instant::now();
    let method = req.method().clone();
    let path = req.uri().path().to_string();
    
    // Check for internal endpoints
    if path == "/_internal/status" {
        return handle_internal_status(port_forward_status, verbose).await;
    }
    
    
    // Check if port-forward is active
    let is_active = {
        let status = port_forward_status.lock().unwrap();
        *status
    };
    
    if !is_active {
        let mut response = Response::new(Body::from("Service Unavailable: Port-forward is not active"));
        *response.status_mut() = StatusCode::SERVICE_UNAVAILABLE;
        
        if verbose >= 1 {
            crate::logger::log_error(format!("{} {} {} → {} ({})", 
                "✗".bright_red(),
                method.as_str(),
                path,
                "503 Service Unavailable".bright_red(),
                format!("{}ms", start.elapsed().as_millis()).bright_yellow()
            ));
        }
        
        return Ok(response);
    }
    
    // Create a new request with the target URL (using the internal port)
    let target_uri = format!(
        "http://127.0.0.1:{}{}", 
        target_port, 
        req.uri().path_and_query().map(|x| x.as_str()).unwrap_or("")
    );
    
    
    
    let mut target_req = Request::builder()
        .method(req.method().clone())
        .uri(target_uri);
    
    // Copy headers
    for (name, value) in req.headers() {
        if name != "host" {  // Skip the host header
            target_req = target_req.header(name, value);
        }
    }
    
    // Handle the request body
    let (req_body_content, req_body_for_logging) = if verbose >= 2 {
        // If we need to log the body, we need to read it fully
        let bytes = hyper::body::to_bytes(req.into_body()).await.unwrap_or_default();
        let bytes_clone = bytes.clone();
        
        // Try to parse as JSON for pretty printing
        let body_for_logging = if let Ok(json_str) = String::from_utf8(bytes_clone.to_vec()) {
            if let Ok(json_value) = serde_json::from_str::<serde_json::Value>(&json_str) {
                Some(serde_json::to_string_pretty(&json_value).unwrap_or(json_str))
            } else {
                Some(format!("Binary data: {} bytes", bytes_clone.len()))
            }
        } else {
            Some(format!("Binary data: {} bytes", bytes_clone.len()))
        };
        
        (Body::from(bytes), body_for_logging)
    } else {
        // If we don't need to log, just pass the body through
        (req.into_body(), None)
    };
    
    // Forward the request
    let client = Client::new();
    let target_req = target_req.body(req_body_content).unwrap();
    

    match client.request(target_req).await {
        Ok(response) => {
            let status = response.status();
            let elapsed = start.elapsed();
            if verbose >= 1 {
                let colored_method = match method {
                    hyper::Method::GET => "GET".bright_blue(),
                    hyper::Method::POST => "POST".bright_magenta(),
                    hyper::Method::PUT => "PUT".bright_yellow(),
                    hyper::Method::DELETE => "DELETE".bright_red(),
                    _ => method.as_str().normal(),
                };
                let status_colored = match status.as_u16() {
                    200..=299 => status.as_str().bright_green(),
                    300..=399 => status.as_str().bright_cyan(),
                    400..=499 => status.as_str().bright_yellow(),
                    _ => status.as_str().bright_red(),
                };
                let ms = elapsed.as_millis();
                let duration_colored = match ms {
                    0..=100 => format!("{}ms", ms).bright_green(),
                    101..=300 => format!("{}ms", ms).bright_yellow(),
                    _ => format!("{}ms", ms).bright_red(),
                };
                crate::logger::log_success(format!("{} {} - {} {} → {} ({})",
                    "✓".bright_green(),
                    resource,
                    colored_method,
                    path,
                    status_colored,
                    duration_colored
                ));
            }
            let (response, opt_resp_body) = if verbose >= 3 || (requests_log_file.is_some() && requests_log_verbosity >= 3) {
                let (parts, body) = response.into_parts();
                let bytes = hyper::body::to_bytes(body).await.unwrap_or_default();
                let body_clone = bytes.clone();
                let content_type_json = parts.headers.get("content-type")
                    .and_then(|ct| ct.to_str().ok())
                    .map(|ct| ct.contains("application/json"))
                    .unwrap_or(false);
                let computed_resp_body = if let Ok(json_str) = String::from_utf8(bytes.to_vec()) {
                    if let Ok(json_value) = serde_json::from_str::<serde_json::Value>(&json_str) {
                        if content_type_json {
                            if requests_log_file.is_some() {
                                serde_json::to_string(&json_value).unwrap_or(json_str)
                            } else {
                                colored_json::to_colored_json(&json_value).unwrap_or_else(|_| serde_json::to_string_pretty(&json_value).unwrap_or(json_str))
                            }
                        } else {
                            serde_json::to_string_pretty(&json_value).unwrap_or(json_str)
                        }
                    } else {
                        format!("Binary data: {} bytes", body_clone.len())
                    }
                } else {
                    format!("Binary data: {} bytes", body_clone.len())
                };
                (Response::from_parts(parts, Body::from(body_clone)), Some(computed_resp_body))
            } else {
                (response, None)
            };
            if let Some(ref log_path) = requests_log_file {
                use std::fs::OpenOptions;
                use std::io::Write;
                let timestamp = chrono::Utc::now().to_rfc3339();
                let log_line = if requests_log_verbosity >= 3 {
                    format!("{} {} - {} {} → {} ({}) [Payload: {}]\n", timestamp, resource, method.as_str(), path, status.to_string(), elapsed.as_millis(), opt_resp_body.as_deref().unwrap_or("N/A"))
                } else {
                    format!("{} {} - {} {} → {} ({})\n", timestamp, resource, method.as_str(), path, status.to_string(), elapsed.as_millis())
                };
                if let Ok(mut file) = OpenOptions::new().append(true).create(true).open(log_path) {
                    let _ = file.write_all(log_line.as_bytes());
                } else {
                    crate::logger::log_error(format!("Failed to write to log file: {}", log_path.display()));
                }
            }
            if verbose >= 2 && req_body_for_logging.is_some() && method != hyper::Method::GET {
                crate::logger::log_info(format!("{} Request body:\n{}", "📄".bright_blue(), req_body_for_logging.unwrap()));
            }
            if verbose >= 3 {
                if let Some(resp_body_str) = opt_resp_body {
                    crate::logger::log_info(format!("{} Response body:\n{}", "📄".bright_green(), resp_body_str));
                }
            }
            Ok(response)
        }
        Err(e) => {
            let error_msg = format!("Failed to forward request: {}", e);
            crate::logger::log_error(error_msg);
            
            let mut response = Response::new(Body::from(error_msg));
            *response.status_mut() = StatusCode::BAD_GATEWAY;
            
            if let Some(ref log_path) = requests_log_file {
                use std::fs::OpenOptions;
                use std::io::Write;
                let timestamp = chrono::Utc::now().to_rfc3339();
                let log_line = if requests_log_verbosity >= 3 {
                    format!("{} {} - {} {} → {} ({}) [Error Payload]\n", timestamp, resource, method, path, "502 Bad Gateway", start.elapsed().as_millis())
                } else {
                    format!("{} {} - {} {} → {} ({})\n", timestamp, resource, method, path, "502 Bad Gateway", start.elapsed().as_millis())
                };
                if let Ok(mut file) = OpenOptions::new().append(true).create(true).open(log_path) {
                    let _ = file.write_all(log_line.as_bytes());
                } else {
                    crate::logger::log_error(format!("Failed to write to log file: {}", log_path.display()));
                }
            }
            if verbose >= 1 {
                let colored_method = match method {
                    hyper::Method::GET => "GET".bright_blue(),
                    hyper::Method::POST => "POST".bright_magenta(),
                    hyper::Method::PUT => "PUT".bright_yellow(),
                    hyper::Method::DELETE => "DELETE".bright_red(),
                    _ => method.as_str().normal(),
                };
                crate::logger::log_error(format!("{} {} - {} {} → {} ({})", 
                    "✗".bright_red(),
                    resource,
                    colored_method,
                    path,
                    "502 Bad Gateway".bright_red(),
                    format!("{}ms", start.elapsed().as_millis()).bright_yellow()
                ));
            }
            
            Ok(response)
        }
    }
}


async fn handle_internal_status(
    port_forward_status: Arc<Mutex<bool>>,
    verbose: u8,
) -> Result<Response<Body>, hyper::Error> {
    // Get current status
    let is_active = {
        let status = port_forward_status.lock().unwrap();
        *status
    };
    
    // Create status response with health details
    let status_info = serde_json::json!({
        "health": {
            "active": is_active,
            "last_ping": chrono::Utc::now().to_rfc3339(),
            "latency": "unknown"
        },
        "status": {
            "verbose_level": verbose,
            "status_text": if is_active { "CONNECTED" } else { "DISCONNECTED" }
        },
        "version": env!("CARGO_PKG_VERSION"),
        "debug_info": {
            "process_id": std::process::id(),
            "system_time": format!("{:?}", std::time::SystemTime::now()),
            "uptime": format!("{:?}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap()),
            "memory_usage": format!("{} MB", std::process::Command::new("ps")
                .args(&["-o", "rss=", "-p", &std::process::id().to_string()])
                .output()
                .map(|output| String::from_utf8_lossy(&output.stdout).trim().parse::<u64>().unwrap_or(0) / 1024)
                .unwrap_or(0))
        },
        "help": {
            "endpoints": {
                "/_internal/status": "Shows port-forward status and health (last ping, latency, active)",
                "/<any-path>": "Proxied to the target service"
            }
        }
    });
    
    let status_json = serde_json::to_string_pretty(&status_info).unwrap();
    
    // Log the status request
    crate::logger::log_info(format!("{} Internal status request: {}", "🔍".bright_magenta(), if is_active { "ACTIVE".bright_green() } else { "INACTIVE".bright_red() }));
    
    // Return JSON response
    let mut response = Response::new(Body::from(status_json));
    response.headers_mut().insert(
        hyper::header::CONTENT_TYPE,
        hyper::header::HeaderValue::from_static("application/json"),
    );
    
    Ok(response)
}

pub async fn start_http_server(
    local_port: u16,
    target_port: u16,
    port_forward_status: Arc<Mutex<bool>>,
    verbose: u8,
    show_liveness: bool,
    resource: String,
    requests_log_file: Option<std::path::PathBuf>,
    requests_log_verbosity: u8,
) -> Result<(), hyper::Error> {
    let addr = SocketAddr::from(([127, 0, 0, 1], local_port));
    
    crate::logger::log_info(format!("{} HTTP proxy server listening on {}", "🌐".bright_green(), format!("http://localhost:{}", local_port).bright_blue()));
    crate::logger::log_info(format!("{} Verbosity level set to {}", "🔍".bright_yellow(), verbose));
    
    let port_forward_status_clone = port_forward_status.clone();
    
    let make_svc = make_service_fn(move |_conn| {
        let port_forward_status = port_forward_status_clone.clone();
        let verbose_level = verbose;
        let target = target_port;
        let show_liveness = show_liveness;
        let resource = resource.clone();
        let requests_log_file = requests_log_file.clone();
        let requests_log_verbosity = requests_log_verbosity;
        
        async move {
            Ok::<_, Infallible>(service_fn(move |req| {
                proxy_request(req, target, port_forward_status.clone(), verbose_level, show_liveness, resource.clone(), requests_log_file.clone(), requests_log_verbosity)
            }))
        }
    });
    
    let server = Server::bind(&addr).serve(make_svc);
    
    server.await
}
