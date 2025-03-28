use crate::cli;
use anyhow::Result;
use colored::*;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Client, Method, Request, Response, Server, StatusCode};
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Instant;

async fn proxy_request(
    req: Request<Body>,
    target_port: u16,
    port_forward_status: Arc<Mutex<bool>>,
    verbose: u8,
) -> Result<Response<Body>, hyper::Error> {
    let start = Instant::now();
    let method = req.method().clone();
    let path = req.uri().path().to_string();
    
    // Log the incoming request
    if verbose >= 1 {
        cli::print_request(method.as_str(), &path, None, None, verbose);
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
            cli::print_request(
                method.as_str(),
                &path,
                Some(StatusCode::SERVICE_UNAVAILABLE.as_u16()),
                Some(start.elapsed()),
                verbose,
            );
        }
        
        return Ok(response);
    }
    
    // Create a new request with the target URL
    let target_uri = format!(
        "http://localhost:{}{}", 
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
    
    // Forward the request
    let client = Client::new();
    let target_req = target_req.body(req.into_body()).unwrap();
    
    match client.request(target_req).await {
        Ok(response) => {
            let status = response.status();
            
            if verbose >= 1 {
                cli::print_request(
                    method.as_str(),
                    &path,
                    Some(status.as_u16()),
                    Some(start.elapsed()),
                    verbose,
                );
            }
            
            Ok(response)
        }
        Err(e) => {
            let error_msg = format!("Failed to forward request: {}", e);
            cli::print_error(&error_msg);
            
            let mut response = Response::new(Body::from(error_msg));
            *response.status_mut() = StatusCode::BAD_GATEWAY;
            
            if verbose >= 1 {
                cli::print_request(
                    method.as_str(),
                    &path,
                    Some(StatusCode::BAD_GATEWAY.as_u16()),
                    Some(start.elapsed()),
                    verbose,
                );
            }
            
            Ok(response)
        }
    }
}

pub async fn start_http_server(
    local_port: u16,
    target_port: u16,
    port_forward_status: Arc<Mutex<bool>>,
    verbose: u8,
) -> Result<(), hyper::Error> {
    let addr = SocketAddr::from(([127, 0, 0, 1], local_port));
    
    println!("{} HTTP proxy server listening on {}", "🌐".bright_green(), format!("http://localhost:{}", local_port).bright_blue());
    
    let port_forward_status_clone = port_forward_status.clone();
    
    let make_svc = make_service_fn(move |_conn| {
        let port_forward_status = port_forward_status_clone.clone();
        let verbose_level = verbose;
        let target = target_port;
        
        async move {
            Ok::<_, Infallible>(service_fn(move |req| {
                proxy_request(req, target, port_forward_status.clone(), verbose_level)
            }))
        }
    });
    
    let server = Server::bind(&addr).serve(make_svc);
    
    server.await
}
