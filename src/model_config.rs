use std::{fs::File, io::Read, path::{Path, PathBuf, absolute}};

use anyhow::{anyhow, Result};
use serde::Deserialize;

use crate::{ContainerMode, EndpointType};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelConfig {
    // The name of the model
    pub name: String,
    // The model artefact path. If provided, we upload this to S3
    // and pass the S3 path as ModelDataURI to the endpoint. SageMaker then makes this available
    // to the container at /opt/ml/model, boosting load times
    pub artefact: Option<String>,
    // Deployment configuration(s)
    pub container: Container,
    // Specify compute characterstics
    pub compute: Compute,
    // Override the default role and bucket names created by Sageturner as part of the deploy process.
    // Expects the bucket and role to already exist
    pub overrides: Option<Overrides>,
}

#[derive(Debug, Deserialize)]
pub struct Container {
    // Configuration for smart mode deploy
    pub generate_container: Option<GenerateContainerConfig>,
    // Configuration for a docker mode deploy
    pub provide_container: Option<ProvideContainerConfig>,
}

#[derive(Debug, Deserialize)]
pub struct GenerateContainerConfig {
    // A path to a directory containing a sageturner.py file. 
    // the sageturner.py file, and the rest of the contents of the directory,
    // will be copied into the container 
    pub code_dir: String,
    // Optional additional system packages to install to container 
    pub system_packages: Option<Vec<String>>,
    // Optional python packages to install to container 
    // (things that load() and predict() depend on, for instance). Don't need to provide FastAPI, that's in the
    // serve.py template
    pub python_packages: Option<Vec<String>>,
    // Whether to install CUDA. Note that we don't allow this if deploy mode is serverless,
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
pub struct ProvideContainerConfig {
    // If bringing your own Dockerfile, provide the directory where we can find the Dockerfile and artefacts to build.
    // We bundle everything in that directory to a TAR as part of the build process, so paths referenced in Docker COPY commands needs to work in that directory
    pub docker_dir: String,
}

#[derive(Debug, Deserialize)]
pub struct Compute {
    pub serverless: Option<ServerlessCompute>,
    pub server: Option<ServerCompute>,
}

#[derive(Debug, Deserialize)]
pub struct ServerlessCompute {
    // Memory required by servless instance
    pub memory: i32,
    // Provisioned servless instances at all times
    pub provisioned_concurrency: i32,
    // Max serverless instances to run at same time
    pub max_concurrency: i32, // Note: Sagemaker Servless endpoints don't support GPUs, so we're always using
}

#[derive(Debug, Deserialize)]
pub struct ServerCompute {
    // AWS EC2 instance type
    pub instance_type: String,
    pub initial_instance_count: i32,
}

#[derive(Debug, Deserialize)]
pub struct Overrides {
    pub bucket_name: Option<String>,
    pub role_arn: Option<String>,
}

pub fn parse_config(path: PathBuf) -> Result<ModelConfig> {
    println!("Parsing model config file");
    let mut file = File::open(path)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;

    match serde_yaml::from_str::<ModelConfig>(&contents) {
        Ok(mc) => Ok(mc),
        Err(err) => match err.location() {
            Some(l) => {
                let location = format!("line {} column {}", l.line(), l.column());
                Err(anyhow!("YAML parsing error at {}: {}", location, err))
            }
            None => Err(err.into()),
        },
    }
}

pub fn validate_config(
    mc: &ModelConfig,
    endpoint_type: &EndpointType,
    container_mode: &ContainerMode,
    config_dir: &Path
) -> Result<()> {
    println!("Validating config file");
    if mc.name.is_empty() {
        return Err(anyhow!(
            "Invalid sageturner config: model name can't be an empty string"
        ));
    }

    if mc.artefact.as_ref().is_some_and(|a| a.is_empty()) {
        return Err(anyhow!(
            "Invalid sageturner config: artefact can't be an empty string"
        ));
    }

    // Validate minimal config present for each deploy mode
    match container_mode {
        ContainerMode::Provide => {
            if mc.container.provide_container.is_none() {
                return Err(anyhow!("Invalid sageturner config: you're trying to deploy in provided container mode, but there's no container.provide_container in your YAML"));
            }
            if mc.container
                .provide_container
                .as_ref()
                .is_some_and(|d| d.docker_dir.is_empty())
            {
                return Err(anyhow!("Invalid sageturner config: you're trying to deploy in provided container mode, but your docker_dir is an empty string"));
            }
        }
        ContainerMode::Generate => {
            if mc.container.generate_container.is_none() {
                return Err(anyhow!("Invalid sageturner config: you're trying to deploy in generate container mode, but there's no container.generate_container field in your YAML"));
            }
            if mc
                .container
                .generate_container
                .as_ref()
                .is_some_and(|s| s.code_dir.is_empty())
            {
                return Err(anyhow!("Invalid sageturner config: you're trying to deploy in generate container mode, but your code field is an empty string. Needs to be path to code with load() and predict()"));
            }
            if let Some(c) = mc.container.generate_container.as_ref() {
                // check that codedir is a directory, and contains a sageturner.py file at minimum
                let config_path = config_dir.join(&c.code_dir);
                let abs_path = absolute(config_path)?;
                let abs_path = abs_path.as_path();
                if !abs_path.is_dir() {
                    return Err(anyhow!("Invalid sageturner config: your code_dir was not a valid directory: {}", abs_path.display()));
                }
                if !abs_path.join("sageturner.py").exists() {
                    return Err(anyhow!("Invalid sageturner config: your code_dir did not contain a sageturner.py file. Please add one with a load() and predict() method. Code dir: {}", abs_path.display()));
                }
            }
        }
    }

    // Validate minimal config present for each endpoint type
    match endpoint_type {
        EndpointType::Serverless => {
            if mc.compute.serverless.is_none() {
                return Err(anyhow!("Invalid sageturner config: you're trying to deploy to a serverless endpoint, without compute->serverless field"));
            }
        }
        EndpointType::Server => {
            if mc.compute.server.is_none() {
                return Err(anyhow!("Invalid sageturner config: you're trying to deploy to a server endpoint, without compute->server field"));
            }
        }
    }

    // Special case: GPUs not supported on serverless
    if *endpoint_type == EndpointType::Serverless
        && *container_mode == ContainerMode::Generate
        && mc
            .container
            .generate_container
            .as_ref()
            .is_some_and(|s| s.install_cuda)
    {
        return Err(anyhow!("Invalid sageturner config: you're trying to generate a container with CUDA installed, but using a Serverless endpoint. 
        Serverless endpoints don't support GPU, set install_cuda to false or deploy to a Server endpoint."));
    }

    Ok(())
}
