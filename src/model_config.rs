use std::{fs::File, io::Read, path::PathBuf};

use anyhow::{Result, anyhow};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct ModelConfig {
    // The name of the model 
    pub name: String,
    // The model artefact. If provided, we upload this to (compressing if uncompressed) to S3 
    // and pass the S3 path as ModelDataURI to the endpoint. Sagemaker then makes this available
    // to the container at /opt/ml/model
    pub artefact: Option<String>,
    // A Python file containing a load() and predict() function. We call these functions from a
    // template serve.py file. This option is for Smart (Dockerless) deploys only, and is mutually
    // exclusive with docker_dir
    pub code: Option<String>,
    // Optional additional system packages to install to container when running a smart deploy
    pub system_packages: Option<String>,
    // Optional additional python packages to install to container when running a smart deploy
    // (things that load() and predict() depend on). Don't need to provide FastAPI, that's in the 
    // serve.py template
    pub python_packages: Option<String>,
    // Whether to install CUDA for a smart deploy. Note that we don't allow this if deploy mode is serverless,
    // as there's no point since Serverless endpoints can't use GPUs
    pub enable_cuda: Option<bool>,
    // The python version for a smart deploy. 
    pub python_version: Option<String>,
    // If bringing your own Dockerfile, provide the directory where we can find the Dockerfile to build.
    // We bundle everything in that directory to a TAR as part of the build process
    pub docker_dir: Option<String>,
    // Specify compute characterstics 
    pub compute: Compute,
    // Override the default role and bucket names created by SimpleSage as part of the deploy process. 
    // Creates the role/bucket if they don't exist, but won't error if they do
    pub sagemaker_overrides: Option<Overrides>,
}

#[derive(Debug, Deserialize)]
struct Compute {
    pub serverless: Option<ServerlessCompute>,
    pub server_compute: Option<ServerCompute>
}

#[derive(Debug, Deserialize)]
struct ServerlessCompute {
    // Memory required by servless instance
    pub memory: i32,
    // Provisioned servless instances at all times 
    pub provisioned_concurrency: i32,
    // Max serverless instances to run at same time
    pub max_concurrency: i32
    // Note: Sagemaker Servless endpoints don't support GPUs, so we're always using 
}

#[derive(Debug, Deserialize)]
struct ServerCompute {
    // AWS EC2 instance type 
    pub instance: String,
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

fn validate_config(mc: &ModelConfig, endpoint_type: &str) -> Result<()> {
    println!("Validating config file");
    // predict-code and docker-dir/python/system packages cannot both be present 
    // validate that the user is either bringing own dockerfile, or using SimpleSage dockerfree deploys
    println!("Validating own dockerfile vs SimpleSage dockerfree deploy")
    if mc.docker_dir.is_some() && (mc.code.is_some() || mc.system_packages.is_some() || mc.python_packages.is_some() || mc.enable_cuda.is_some() || mc.python_version.is_some()) {
        return Err(anyhow!("code/system_packages/python_packages/enabled_cuda are for smart (dockerfile free) deploy mode, 
        and mutually exclusive with docker_dir (which is for bringing your own Dockerfile). 
        If you're bringing your own Dockerfile, 
        you're responsible for copying the server code, installing packages, and CUDA"));
    }

    let smart_deploy = mc.docker_dir.is_none();
    if smart_deploy {
        // validate we have minimal fields for a smart deploy
        if mc.code.is_none() {
            return Err(anyhow!("ERROR: You're attempting a smart deploy, but you've provided no code file that tells us how to load model and run inference"));
        }

        // warn users about blank fields. these aren't required and we do sensible things as default, 
        // but we should let users know they're missing and what the consequences are
        if mc.enable_cuda.is_none() {
            println!("WARNING: You didn't set enable_cuda. Defaulting to false for dockerfree deploy");
        }

        if mc.python_packages.is_none() {
            println!("WARNING: You didn't set any additional python packages for dockerfree deploy. The generated container won't have any PIP packages except FastAPI");
        }

        if mc.system_packages.is_none() {
            println!("WARNING: You didn't set any additional system packages for dockerfree deploy. The generated container won't apt-get install any additional packages");
        }

        if mc.python_version.is_none() {
            println!("WARNING: You didn't set python version for a dockerfree deploy. We'll default to 3.12");
        }

        // Disallow cuda-enabled if the endpoint type is serverless. The user might expect a GPU 
        if mc.enable_cuda.is_some_and(|x| x) && endpoint_type == "serverless" {
            return(Err(anyhow!("ERROR: You're attempting a dockerfree serverless deployment, but you've got CUDA enabled. Sagemaker doesn't
             support GPUs for serverless inference. You need to bring your own Dockerfile, and install CUDA toolkit yourself (thoughts and prayers)")));
        }
    } else {
        // what are the conditions for a bring your own dockerfile deploy

    }

    Ok(())
}