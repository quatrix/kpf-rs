use anyhow::{anyhow, Context, Result};
use kube::{
    api::Api,
    Client,
};
use k8s_openapi::api::core::v1::{Pod, Service};
use std::process::Stdio;
use tokio::process::Command;

pub fn parse_resource(resource_str: &str) -> Result<(String, String, u16)> {
    // Format: type/name:port
    let parts: Vec<&str> = resource_str.split(':').collect();
    if parts.len() != 2 {
        return Err(anyhow!("Invalid resource format. Expected type/name:port"));
    }
    
    let resource_parts: Vec<&str> = parts[0].split('/').collect();
    if resource_parts.len() != 2 {
        return Err(anyhow!("Invalid resource format. Expected type/name:port"));
    }
    
    let resource_type = resource_parts[0].to_string();
    let resource_name = resource_parts[1].to_string();
    let port = parts[1].parse::<u16>().context("Invalid port number")?;
    
    Ok((resource_type, resource_name, port))
}

pub async fn validate_resource(
    resource_type: &str,
    resource_name: &str,
    namespace: &str,
) -> Result<()> {
    let client = Client::try_default().await.context("Failed to create Kubernetes client")?;
    
    match resource_type {
        "pod" => {
            let pods: Api<Pod> = Api::namespaced(client, namespace);
            pods.get(resource_name).await.context("Pod not found")?;
        }
        "service" | "svc" => {
            let services: Api<Service> = Api::namespaced(client, namespace);
            services.get(resource_name).await.context("Service not found")?;
        }
        _ => return Err(anyhow!("Unsupported resource type: {}", resource_type)),
    }
    
    Ok(())
}

pub async fn create_port_forward(
    resource_type: &str,
    resource_name: &str,
    resource_port: u16,
    local_port: u16,
    namespace: &str,
    child_handle: std::sync::Arc<tokio::sync::Mutex<Option<tokio::process::Child>>>,
) -> Result<impl futures::Future<Output = Result<()>>> {
    // Validate that the resource exists
    if let Err(e) = validate_resource(resource_type, resource_name, namespace).await {
        crate::logger::log_error(format!("Resource validation failed: {}", e));
        return Err(e);
    }
    
    // Use kubectl port-forward command
    let mut cmd = Command::new("kubectl");
    cmd.arg("port-forward")
        .arg(format!("{}/{}", resource_type, resource_name))
        .arg(format!("{}:{}", local_port, resource_port))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    
    let child = cmd.spawn().context("Failed to start kubectl port-forward")?;
    {
        let mut handle = child_handle.lock().await;
        *handle = Some(child);
    }
    
    Ok(async move {
        let child_opt = {
            let mut handle = child_handle.lock().await;
            handle.take()
        };
        if let Some(mut child) = child_opt {
            let status = child.wait().await.context("Failed to wait for kubectl process")?;
    
            if !status.success() {
                let mut stderr = String::new();
                if let Some(mut err) = child.stderr.take() {
                    use tokio::io::AsyncReadExt;
                    let _ = err.read_to_string(&mut stderr).await;
                }
    
                return Err(anyhow!("kubectl port-forward failed: {}", stderr));
            }
        }
    
        Ok(())
    })
}
