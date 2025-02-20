use std::{fs::File, io::Read, path::PathBuf};

use anyhow::{Result, anyhow};
use serde::Deserialize;

use crate::{DeploymentMode, EndpointType};

#[derive(Debug, Deserialize)]
pub struct ModelConfig {
    // The name of the model 
    pub name: String,
    // The model artefact. If provided, we upload this to (compressing if uncompressed) to S3 
    // and pass the S3 path as ModelDataURI to the endpoint. SageMaker then makes this available
    // to the container at /opt/ml/model, boosting load times
    pub artefact: Option<String>,
    // Deployment configuration(s)
    pub deployment: Deployment,
    // Specify compute characterstics 
    pub compute: Compute,
    // Override the default role and bucket names created by Sageturner as part of the deploy process. 
    // Creates the role/bucket if they don't exist, but won't error if they do
    pub sagemaker_overrides: Option<Overrides>,
}

#[derive(Debug, Deserialize)]
struct Deployment {
    // Configuration for smart mode deploy 
    pub smart_deploy: Option<SmartDeployConfig>,
    // Configuration for a docker mode deploy
    pub docker_deploy: Option<DockerDeployConfig>,
}

#[derive(Debug, Deserialize)]
struct SmartDeployConfig {
    // A path to a Python script containing a load() and predict() function. We call these functions from a
    // template serve.py file. This option is for smart mode deploys only
    pub code: String,
    // Optional additional system packages to install to container when running a smart deploy
    pub system_packages: Option<Vec<String>>,
    // Optional python packages to install to container when running a smart deploy
    // (things that load() and predict() depend on, for instance). Don't need to provide FastAPI, that's in the 
    // serve.py template
    pub python_packages: Option<Vec<String>>,
    // Whether to install CUDA for a smart deploy. Note that we don't allow this if deploy mode is serverless,
    // as there's no point since Serverless endpoints can't use GPUs
    pub install_cuda: bool,
    // The python version for a smart deploy. 
    // defaults to 3.12
    #[serde(default = "default_python")] 
    pub python_version: String,
}

fn default_python() -> String {
    "3.12".to_string()
}

#[derive(Debug, Deserialize)]
struct DockerDeployConfig {
    // If bringing your own Dockerfile, provide the directory where we can find the Dockerfile and artefacts to build.
    // We bundle everything in that directory to a TAR as part of the build process, so paths referenced in Docker COPY commands needs to work in that directory 
    pub docker_dir: String,
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

    match serde_yaml::from_str::<ModelConfig>(&contents) {
        Ok(mc) => Ok(mc),
        Err(err) => {
            match err.location() {
                Some(l) => {
                    let location = format!("line {} column {}", l.line(), l.column());
                    Err(anyhow!("YAML parsing error at {}: {}", location, err))
                },
                None => Err(err.into()),
            }
        },
    }
}

pub fn validate_config(mc: &ModelConfig, endpoint_type: &EndpointType, deploy_mode: &DeploymentMode) -> Result<()> {
    println!("Validating config file");
    if mc.name.is_empty() {
        return Err(anyhow!("Invalid sageturner config: model name can't be an empty string"));
    }

    if mc.artefact.as_ref().is_some_and(|a| a.is_empty()) {
        return Err(anyhow!("Invalid sageturner config: artefact can't be an empty string"));
    }

    // Validate minimal config present for each deploy mode 
    match deploy_mode {
        DeploymentMode::Docker => {
            if mc.deployment.docker_deploy.is_none() {
                return Err(anyhow!("Invalid sageturner config: you're trying to deploy in docker mode, without field docker_deploy"));
            }
            if mc.deployment.docker_deploy.as_ref().is_some_and(|d| d.docker_dir.is_empty()) {
                return Err(anyhow!("Invalid sageturner config: you're trying to deploy in docker mode, but your docker_dir is an empty string"));
            }
        },
        DeploymentMode::Smart => {
            if mc.deployment.smart_deploy.is_none() {
                return Err(anyhow!("Invalid sageturner config: you're trying to deploy in smart mode, without field smart_deploy"));
            }
            if mc.deployment.smart_deploy.as_ref().is_some_and(|s| s.code.is_empty()) {
                return Err(anyhow!("Invalid sageturner config: you're trying to deploy in smart mode, but your code field is an empty string. needs to be path to code with load() and predict()"));
            }
        },
    }

    // Validate minimal config present for each endpoint type 
    match endpoint_type {
        EndpointType::Serverless => {
            if mc.compute.serverless.is_none() {
                return Err(anyhow!("Invalid sageturner config: you're trying to deploy to a serverless endpoint, without compute->serverless field"));
            }
        },
        EndpointType::Server => {
            if mc.compute.server_compute.is_none() {
                return Err(anyhow!("Invalid sageturner config: you're trying to deploy to a server endpoint, without compute->server field"));
            }
        },
    }

    // Special case: GPUs not supported on serverless
    if *endpoint_type == EndpointType::Serverless && 
    *deploy_mode == DeploymentMode::Smart && 
    mc.deployment.smart_deploy.as_ref().is_some_and(|s| s.install_cuda) {
        return Err(anyhow!("Invalid sageturner config: you're trying to do a smart deploy, to a serverless endpoint, with install_cuda as true. Serverless endpoints don't support GPU, so no point installing CUDA when sageturner generates your container"));
    }

    Ok(())
}