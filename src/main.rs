use std::str::FromStr;

use argh::FromArgs;
use anyhow::{anyhow, Result};
use bollard::Docker;

mod docker;
mod aws;
mod model_config;

#[derive(Debug, FromArgs, PartialEq)]
#[argh(description="Sageturner deploys your models to Amazon SageMaker in one command.")]
struct SageturnerCLI {
    #[argh(subcommand)]
    nested: SageturnerSubCommands
}

#[derive(Debug, FromArgs, PartialEq)]
#[argh(subcommand)]
enum SageturnerSubCommands {
    Deploy(Deploy),
}

#[derive(Debug, FromArgs, PartialEq)]
#[argh(subcommand, name="deploy", description="Deploy models to Sagemaker endpoints")]
struct Deploy {
    #[argh(option, short='e', description="the type of endpoint for deployment (serverless or server)")]
    endpoint_type: EndpointType, 

    #[argh(option, short='m', description="sageturner deployment mode: docker (supply your own dockerfile) or smart (sageturner builds one for you)")]
    mode: DeploymentMode,

    #[argh(option, short='c', description="path to sageturner.yaml")]
    model_config: String
}

#[derive(Debug, PartialEq)]
enum EndpointType {
    Serverless,
    Server
}

impl FromStr for EndpointType {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "serverless" => Ok(EndpointType::Serverless),
            "server" => Ok(EndpointType::Server),
            _ => Err(anyhow!("Invalid endpoint type. serverless or server only, not: {}", s))
        }
    }
}

impl std::fmt::Display for EndpointType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EndpointType::Serverless => write!(f, "serverless"),
            EndpointType::Server => write!(f, "server"),
        }
    }
}

#[derive(Debug, PartialEq)]
enum DeploymentMode {
    Docker, 
    Smart
}

impl FromStr for DeploymentMode {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "smart" => Ok(DeploymentMode::Smart),
            "docker" => Ok(DeploymentMode::Docker),
            _ => Err(anyhow!("Invalid deployment type. docker or smart, not: {}", s))
        }
    }
}

impl std::fmt::Display for DeploymentMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeploymentMode::Docker => write!(f, "docker"),
            DeploymentMode::Smart => write!(f, "smart"),
        }
    }
}

#[::tokio::main]
async fn main() -> Result<()> {
    let cmd : SageturnerCLI = argh::from_env();

    let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
    let sage_client = aws_sdk_sagemaker::Client::new(&config);
    let ecr_client = aws_sdk_ecr::Client::new(&config);
    let iam_client = aws_sdk_iam::Client::new(&config);
    let s3_client = aws_sdk_s3::Client::new(&config);

    let docker = docker::get_client().await;

    match cmd.nested {
        SageturnerSubCommands::Deploy(deploy) => process_deploy(&ecr_client, &sage_client, &docker, &iam_client, &s3_client, &deploy).await?,
    }

    Ok(())
}

async fn process_deploy(ecr_client: &aws_sdk_ecr::Client, 
                        sage_client: &aws_sdk_sagemaker::Client, 
                        docker_client: &Docker, 
                        iam_client: &aws_sdk_iam::Client, 
                        s3_client: &aws_sdk_s3::Client, 
                        deploy_params: &Deploy) 
                        -> Result<()> {
    println!("Deploying model with config at {} to {} endpoint, {} deployment mode", &deploy_params.model_config, &deploy_params.endpoint_type, &deploy_params.mode);

    let model_config = model_config::parse_config(deploy_params.model_config.into())?;
    model_config::validate_config(&model_config, &deploy_params.endpoint_type, &deploy_params.mode)?;

    // container_mode 

    // EZ Mode vs user supplied docker file
    match model_config.code {
        Some(_) => {
            let code_location = model_config.code.ok_or_else(|| anyhow!("Something went wrong with our validation. Raise an issue"))?;
            let python_packages = model_config.python_packages.ok_or_else(|| anyhow!("Something went wrong with our validation. Raise an issue"))?;
            let system_packages = model_config.system_packages.ok_or_else(|| anyhow!("Something went wrong with our validation. Raise an issue"))?;
            let serve_code = "blah blah blah";
            println!("You didn't select a Dockerfile, dynamically building one (GPU: {}, inference code: {}", model_config.compute.gpu, code_location);
            docker::build_image_ez_mode(model_config.compute.gpu, &python_packages, &system_packages, &model_config.name, &serve_code, docker_client).await?;
        },
        None => {
            let docker_dir = model_config.docker_dir.ok_or_else(|| anyhow!("Something went wrong with our validation. Raise an issue."))?;
            println!("You brought your own Dockerfile, at: {}", docker_dir);
            docker::build_image_byo(&docker_dir, docker_client, &model_config.name).await?;
        },
    }

    let endpoint = docker::push_image(docker_client, ecr_client, &model_config.name).await?;
    let uri = format!("{endpoint}:latest");
    let exec_role_arn: String;
    // TODO make this nicer with if/else let pattern matching in setting the bucket/role names then just have a single call to each function since each is idempotent wrt to name
    if let Some(overrides) = model_config.sagemaker_overrides {
        let should_override_bucket: bool = false;
        let should_override_role: bool = false; 
        if let Some(_) = overrides.bucket {
            should_override_bucket = true;
        }
        if let Some(_) = overrides.role {
            should_override_role = true;
        }
        if should_override_bucket && should_override_role {
            // override both
            let bucket_name = overrides.bucket.ok_or_else(|| anyhow!("Something went wrong with our config parsing. Raise an issue"))?;
            let role_name = overrides.role.ok_or_else(|| anyhow!("Something went wrong with our config parsing. Raise an issue"))?;
            exec_role_arn = aws::create_sagemaker_role(&role_name, iam_client).await?;
            aws::create_sagemaker_bucket(&bucket_name, s3_client).await?;
        } else if should_override_bucket {
            // override bucket and use default for role
            let bucket_name = overrides.bucket.ok_or_else(|| anyhow!("Something went wrong with our config parsing. Raise an issue"))?;
            aws::create_sagemaker_bucket(&bucket_name, s3_client).await?;
            exec_role_arn = aws::create_sagemaker_role("Sageturner-role", iam_client).await?;
        } else if should_override_role {
            // override role and use default for bucket 
            let role_name = overrides.role.ok_or_else(|| anyhow!("Something went wrong with our config parsing. Raise an issue"))?;
            aws::create_sagemaker_bucket("Sageturner-sagemaker", s3_client).await?;
            exec_role_arn = aws::create_sagemaker_role(&role_name, iam_client).await?;
        }
    } else {
        exec_role_arn = aws::create_sagemaker_role("Sageturner-role", iam_client).await?;
        aws::create_sagemaker_bucket("Sageturner-sagemaker", s3_client).await?; // needs to have sagemaker in it for policy
    }

    aws::create_sagemaker_model(&model_config.name, &exec_role_arn, &uri, sage_client).await?;

    aws::create_serverless_endpoint(endpoint_name, model_name, memory_size, max_concurrency, sage_client).await?;
    Ok(())
}