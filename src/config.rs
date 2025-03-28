use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

#[derive(Debug, Serialize, Deserialize)]
pub struct ForwardConfig {
    pub resource: String,
    pub local_port: Option<u16>,
    pub timeout: Option<u64>,
    pub liveness_probe: Option<String>,
    pub namespace: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    pub forwards: Vec<ForwardConfig>,
    pub verbose: Option<u8>,
}

pub fn load_config<P: AsRef<Path>>(path: P) -> Result<Config> {
    let file = File::open(path).context("Failed to open config file")?;
    let reader = BufReader::new(file);
    let config: Config = serde_json::from_reader(reader).context("Failed to parse config file")?;
    Ok(config)
}
