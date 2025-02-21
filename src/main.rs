use std::str::FromStr;

use anyhow::{anyhow, Result};
use argh::FromArgs;
use bollard::Docker;
use chrono::{DateTime, Utc};

mod aws;
mod docker;
mod model_config;
mod pyserve;

#[derive(Debug, FromArgs, PartialEq)]
#[argh(description = "Sageturner deploys your models to Amazon SageMaker in one command.")]
struct SageturnerCLI {
    #[argh(subcommand)]
    nested: SageturnerSubCommands,
}

#[derive(Debug, FromArgs, PartialEq)]
#[argh(subcommand)]
enum SageturnerSubCommands {
    Deploy(Deploy),
}

#[derive(Debug, FromArgs, PartialEq)]
#[argh(
    subcommand,
    name = "deploy",
    description = "Deploy models to Sagemaker endpoints"
)]
struct Deploy {
    #[argh(
        option,
        short = 'e',
        description = "the type of endpoint for deployment (serverless or server)"
    )]
    endpoint_type: EndpointType,

    #[argh(
        option,
        short = 'm',
        description = "sageturner deployment mode: docker (supply your own dockerfile) or smart (sageturner builds one for you)"
    )]
    mode: DeploymentMode,

    #[argh(option, short = 'c', description = "path to sageturner.yaml")]
    model_config: String,
}

#[derive(Debug, PartialEq)]
enum EndpointType {
    Serverless,
    Server,
}

impl FromStr for EndpointType {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "serverless" => Ok(EndpointType::Serverless),
            "server" => Ok(EndpointType::Server),
            _ => Err(anyhow!(
                "Invalid endpoint type. serverless or server only, not: {}",
                s
            )),
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
    Smart,
}

impl FromStr for DeploymentMode {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "smart" => Ok(DeploymentMode::Smart),
            "docker" => Ok(DeploymentMode::Docker),
            _ => Err(anyhow!(
                "Invalid deployment type. docker or smart, not: {}",
                s
            )),
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
    let cmd: SageturnerCLI = argh::from_env();

    let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
    let sage_client = aws_sdk_sagemaker::Client::new(&config);
    let ecr_client = aws_sdk_ecr::Client::new(&config);
    let iam_client = aws_sdk_iam::Client::new(&config);
    let s3_client = aws_sdk_s3::Client::new(&config);

    let docker = docker::get_client().await;

    match cmd.nested {
        SageturnerSubCommands::Deploy(deploy) => {
            process_deploy(
                &ecr_client,
                &sage_client,
                &docker,
                &iam_client,
                &s3_client,
                &deploy,
            )
            .await?
        }
    }

    Ok(())
}

async fn process_deploy(
    ecr_client: &aws_sdk_ecr::Client,
    sage_client: &aws_sdk_sagemaker::Client,
    docker_client: &Docker,
    iam_client: &aws_sdk_iam::Client,
    s3_client: &aws_sdk_s3::Client,
    deploy_params: &Deploy,
) -> Result<()> {
    println!(
        "Deploying model with config at {} to {} endpoint, {} deployment mode",
        &deploy_params.model_config, &deploy_params.endpoint_type, &deploy_params.mode
    );

    // TODO - unclone this
    let model_config = model_config::parse_config(deploy_params.model_config.clone().into())?;
    model_config::validate_config(
        &model_config,
        &deploy_params.endpoint_type,
        &deploy_params.mode,
    )?;

    // Generate dockerfile & build, or build the supplied dockerfile
    match deploy_params.mode {
        DeploymentMode::Docker => {
            let docker_dir = model_config
                .deployment
                .docker_deploy
                .ok_or_else(|| {
                    anyhow!("Something went wrong with our validation. Raise an issue.")
                })?
                .docker_dir;
            docker::build_image_byo(&docker_dir, docker_client, &model_config.name).await?;
        }
        DeploymentMode::Smart => {
            let code_location = &model_config
                .deployment
                .smart_deploy
                .as_ref()
                .ok_or_else(|| anyhow!("Something went wrong with our validation. Raise an issue"))?
                .code.as_str();
            let python_packages = &model_config
                .deployment
                .smart_deploy
                .as_ref()
                .ok_or_else(|| anyhow!("Something went wrong with our validation. Raise an issue"))?
                .python_packages;
            let system_packages = &model_config
                .deployment
                .smart_deploy
                .as_ref()
                .ok_or_else(|| anyhow!("Something went wrong with our validation. Raise an issue"))?
                .system_packages;
            let serve_code = pyserve::get_serve_code(code_location);
            let gpu = model_config
                .deployment
                .smart_deploy
                .as_ref()
                .ok_or_else(|| anyhow!("Something went wrong with our validation. Raise an issue"))?
                .install_cuda;
            let python_version = model_config
                .deployment
                .smart_deploy
                .as_ref()
                .ok_or_else(|| anyhow!("Something went wrong with our validation. Raise an issue"))?
                .python_version.clone();

            // TODO - unclone this
            let python_packages_str = python_packages
                .clone()
                .unwrap_or(Vec::<String>::new())
                .join(" ");
            let system_packages_str = system_packages
                .clone()
                .unwrap_or(Vec::<String>::new())
                .join(" ");
            docker::build_image_ez_mode(
                gpu,
                &python_packages_str,
                &system_packages_str,
                &model_config.name,
                &serve_code,
                docker_client,
                &python_version,
                code_location // TODO fix this unecessary auto deref 
            )
            .await?;
        }
    }

    let repo_endpoint = docker::push_image(docker_client, ecr_client, &model_config.name).await?;
    let uri = format!("{repo_endpoint}:latest");

    let mut bucket_name = "Sageturner-sagemaker".to_string();
    let mut execution_role_name = "Sageturner-role".to_string();

    // TODO - unclone this
    if let Some(o) = model_config.sagemaker_overrides {
        if let Some(b) = o.bucket { bucket_name = b.clone() }
        if let Some(r) = o.role { execution_role_name = r.clone() }
    }

    let exec_role_arn = aws::create_sagemaker_role(&execution_role_name, iam_client).await?;
    aws::create_sagemaker_bucket(&bucket_name, s3_client).await?;

    // Upload a model artefact if we have it
    match model_config.artefact {
        Some(a) => {
            let utc_now: DateTime<Utc> = Utc::now();
            let s3_key = format!("{}/{}/{}", &model_config.name, utc_now, a);
            let s3_path = aws::upload_artefact(&a, &bucket_name, &s3_key, s3_client).await?;
            aws::create_sagemaker_model(
                &model_config.name,
                &exec_role_arn,
                &uri,
                sage_client,
                Some(s3_path),
            )
            .await?;
        }
        None => {
            // No artefact to put on S3
            aws::create_sagemaker_model(
                &model_config.name,
                &exec_role_arn,
                &uri,
                sage_client,
                None,
            )
            .await?;
        }
    }

    let endpoint_name = format!("{}-sageturner-endpoint", &model_config.name);
    match deploy_params.endpoint_type {
        EndpointType::Serverless => {
            let memory = model_config
                .compute
                .serverless
                .as_ref()
                .ok_or_else(|| anyhow!("Something went wrong with our validation. Raise an issue"))?
                .memory;
            let max_concurrency = model_config
                .compute
                .serverless
                .as_ref()
                .ok_or_else(|| anyhow!("Something went wrong with our validation. Raise an issue"))?
                .max_concurrency;
            let provisioned_concurrency = model_config
                .compute
                .serverless
                .as_ref()
                .ok_or_else(|| anyhow!("Something went wrong with our validation. Raise an issue"))?
                .provisioned_concurrency;
            aws::create_serverless_endpoint(
                &endpoint_name,
                &model_config.name,
                memory,
                max_concurrency,
                provisioned_concurrency,
                sage_client,
            )
            .await?;
        }
        EndpointType::Server => {
            let instance_type = model_config
                .compute
                .server_compute
                .as_ref()
                .ok_or_else(|| anyhow!("Something went wrong with our validation. Raise an issue"))?
                .instance_type
                .clone();
            let initial_instance_count = model_config
                .compute
                .server_compute
                .as_ref()
                .ok_or_else(|| anyhow!("Something went wrong with our validation. Raise an issue"))?
                .initial_instance_count;
            aws::create_server_endpoint(
                &endpoint_name,
                &model_config.name,
                &instance_type,
                initial_instance_count,
                sage_client,
            )
            .await?;
        }
    }
    println!("Sageturner done!");
    Ok(())
}
