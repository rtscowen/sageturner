use std::{fs::File, io::Read, path::PathBuf};

use anyhow::{Result, anyhow};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct ModelConfig {
    pub name: String,
    pub artefact: Option<String>,
    pub code: Option<String>,
    pub system_packages: Option<String>,
    pub python_packages: Option<String>,
    pub docker_dir: Option<String>,
    pub compute: Compute,
    pub sagemaker_overrides: Option<Overrides>,
}

#[derive(Debug, Deserialize)]
struct Compute {
    pub gpu: bool,
    pub memory: i32,
    pub cpu: i32,
    pub max_concurrency: i32
}

#[derive(Debug, Deserialize)]
struct Overrides {
    pub bucket: Option<String>,
    pub role: Option<String>
}

pub fn parse_config(path: PathBuf) -> Result<ModelConfig> {
    println!("Parsing model config file");
    let mut file = File::open(path)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;

    let config: ModelConfig = serde_yaml::from_str(&contents)?;

    validate_config(&config)?;

    Ok(config)
}

fn validate_config(mc: &ModelConfig) -> Result<()> {
    println!("Validating config file");
    // predict-code and docker-dir cannot both be present 
    if mc.docker_dir.is_some() && (mc.code.is_some() || mc.system_packages.is_some() || mc.python_packages.is_some()) {
        return Err(anyhow!("code/system_packages/python_packages are for smart deploy mode, and mutually exclusive with docker_dir. If you're bringing your own Dockerfile, you're responsible for copying the server code and installing packages in the Dockerfile"));
    }
    Ok(())
}