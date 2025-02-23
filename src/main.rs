use std::{path::Path, str::FromStr, time::Duration};

use anyhow::{anyhow, Result};
use argh::FromArgs;
use aws_config::timeout::TimeoutConfig;
use bollard::Docker;
use chrono::Utc;


mod aws;
mod docker;
mod model_config;
mod pyserve;

const DEFAULT_ROLE_NAME: &str = "sageturner-role-sagemaker";
const DEFAULT_BUCKET_NAME: &str = "sageturner-sagemaker-models";

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
    Setup(Setup)
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
        description = "the type of endpoint for deployment: serverless, server)"
    )]
    endpoint_type: EndpointType,

    #[argh(
        option,
        short = 'm',
        description = "sageturner container mode: generate, provide"
    )]
    mode: ContainerMode,

    #[argh(option, short = 'c', description = "path to config YAML")]
    config_path: String,
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
enum ContainerMode {
    Generate,
    Provide,
}

impl FromStr for ContainerMode {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "generate" => Ok(ContainerMode::Generate),
            "provide" => Ok(ContainerMode::Provide),
            _ => Err(anyhow!(
                "Invalid container mode. use generate or provide, not: {}",
                s
            )),
        }
    }
}

impl std::fmt::Display for ContainerMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ContainerMode::Generate => write!(f, "generate"),
            ContainerMode::Provide => write!(f, "provide"),
        }
    }
}

#[derive(Debug, FromArgs, PartialEq)]
#[argh(
    subcommand,
    name = "setup",
    description = "Create Sageturner bucket and role"
)]
struct Setup {}

#[::tokio::main]
async fn main() -> Result<()> {
    let cmd: SageturnerCLI = argh::from_env();

    let config = aws_config::defaults(aws_config::BehaviorVersion::latest()).timeout_config(TimeoutConfig::builder()
    .connect_timeout(Duration::from_secs(8))
    .build()).load().await;
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
        },
        SageturnerSubCommands::Setup(_) => {
            println!("Performing initial setup: creating Sageturner role and bucket");
            // Create role with name sageturner-role, attach SagemakerFullAccessPolicy
            aws::create_sagemaker_role(DEFAULT_ROLE_NAME, &iam_client).await?;
            // Create bucket with name sageturner-sagemaker-models, attach SagemakerFullAccessPolicy
            aws::create_sagemaker_bucket(DEFAULT_BUCKET_NAME, &s3_client).await?;
            println!("Setup done");
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
        "Deploying model with config at {} to {} endpoint, {} container mode",
        &deploy_params.config_path, &deploy_params.endpoint_type, &deploy_params.mode
    );

    let config_dir = Path::new(&deploy_params.config_path).parent().expect("Your config path didn't point to a YAML file");
    let deploy_timestamp = Utc::now().format("%d%m%Y%H%M").to_string();

    // TODO - unclone this
    let model_config = model_config::parse_config(deploy_params.config_path.clone().into())?;
    model_config::validate_config(
        &model_config,
        &deploy_params.endpoint_type,
        &deploy_params.mode,
        config_dir
    )?;

    // Generate dockerfile & build, or build the supplied dockerfile
    match deploy_params.mode {
        ContainerMode::Provide => {
            let docker_dir = model_config
                .container
                .provide_container
                .ok_or_else(|| {
                    anyhow!("Something went wrong with our validation. Raise an issue.")
                })?
                .docker_dir;
            docker::build_image_byo(Path::new(&docker_dir), docker_client, &model_config.name, config_dir).await?;
        }
        ContainerMode::Generate => {
            let code_location = &model_config
                .container
                .generate_container
                .as_ref()
                .ok_or_else(|| anyhow!("Something went wrong with our validation. Raise an issue"))?
                .code_dir.as_str();
            let python_packages = &model_config
                .container
                .generate_container
                .as_ref()
                .ok_or_else(|| anyhow!("Something went wrong with our validation. Raise an issue"))?
                .python_packages;
            let system_packages = &model_config
                .container
                .generate_container
                .as_ref()
                .ok_or_else(|| anyhow!("Something went wrong with our validation. Raise an issue"))?
                .system_packages;
            let serve_code = pyserve::get_serve_code();
            let gpu = model_config
                .container
                .generate_container
                .as_ref()
                .ok_or_else(|| anyhow!("Something went wrong with our validation. Raise an issue"))?
                .install_cuda;
            let python_version = model_config
                .container
                .generate_container
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
                code_location, // TODO fix this unecessary auto deref,
                config_dir
            )
            .await?;
        }
    }

    let repo_endpoint = docker::push_image(docker_client, ecr_client, &model_config.name).await?;
    let uri = format!("{repo_endpoint}:latest");

    let mut bucket_name = DEFAULT_BUCKET_NAME.to_string();
    let mut execution_role_name = DEFAULT_ROLE_NAME.to_string();

    // TODO - unclone this
    if let Some(o) = model_config.overrides {
        if let Some(b) = o.bucket_name { 
            println!("Overriding default bucket name with: {}", b);
            bucket_name = b.clone() 
        }
        
        if let Some(r) = o.role_arn { 
            println!("Overriding default role name with: {}", r);
            execution_role_name = r.clone();
        }
    }

    let execution_role_arn = aws::get_role_arn(&execution_role_name, iam_client).await?;
    let final_model_name: String;
    // Upload a model artefact if we have it
    match model_config.artefact {
        Some(a) => {
            let path = Path::new(&a);
            let a_name = path.file_name().ok_or_else(|| anyhow!("Couldn't extract filename from artefact path"))?;
            let s3_key = format!("{}/{}/{}", &model_config.name, deploy_timestamp, a_name.to_str().unwrap());
            let s3_path = aws::upload_artefact(&a, &bucket_name, &s3_key, s3_client, config_dir).await?;
            final_model_name = aws::create_sagemaker_model(
                &model_config.name,
                &execution_role_arn,
                &uri,
                sage_client,
                Some(s3_path),
                &deploy_timestamp
            )
            .await?;
        }
        None => {
            // No artefact to put on S3
            final_model_name = aws::create_sagemaker_model(
                &model_config.name,
                &execution_role_arn,
                &uri,
                sage_client,
                None,
                &deploy_timestamp
            )
            .await?;
        }
    }

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
                &final_model_name,
                memory,
                max_concurrency,
                provisioned_concurrency,
                sage_client,
                &deploy_timestamp
            )
            .await?;
        }
        EndpointType::Server => {
            let instance_type = model_config
                .compute
                .server
                .as_ref()
                .ok_or_else(|| anyhow!("Something went wrong with our validation. Raise an issue"))?
                .instance_type
                .clone();
            let initial_instance_count = model_config
                .compute
                .server
                .as_ref()
                .ok_or_else(|| anyhow!("Something went wrong with our validation. Raise an issue"))?
                .initial_instance_count;
            aws::create_server_endpoint(
                &final_model_name,
                &instance_type,
                initial_instance_count,
                &execution_role_arn,
                sage_client,
                &deploy_timestamp
            )
            .await?;
        }
    }
    println!("Sageturner done!");
    Ok(())
}
