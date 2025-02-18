use argh::FromArgs;

use anyhow::Result;
use aws_sdk_sagemaker::types::{builders::ContainerDefinitionBuilder, ContainerDefinition};
use base64::prelude::*;
use bollard::{auth::DockerCredentials, image::{PushImageOptions, TagImageOptions}, Docker};
use futures_util::StreamExt;

mod docker;
mod aws;

#[derive(Debug, FromArgs, PartialEq)]
#[argh(description="SimpleSage gets your models to AWS SageMaker in one step")]
struct SimpleSageCLI {
    #[argh(subcommand)]
    nested: SimpleSageSubCommands
}

#[derive(Debug, FromArgs, PartialEq)]
#[argh(subcommand)]
enum SimpleSageSubCommands {
    Deploy(Deploy),
    Setup(Setup)
}

#[derive(Debug, FromArgs, PartialEq)]
#[argh(subcommand, name="deploy", description="Deploy serialised models directly to Sagemaker endpoint")]
struct Deploy {
    #[argh(option, short='w', description="wire config path")]
    wire_file: String, 
    
    #[argh(option, short='d', description="dockerfile path")]
    dockerfile: String,

    #[argh(option, short='e', description="endpoint type")]
    endpoint_type: String,

    #[argh(option, short='r', description="repo name")]
    repo_name: String,
}


#[derive(Debug, FromArgs, PartialEq)]
#[argh(subcommand, name="setup", description="Bootstrap sagemaker with necessary setup - incurs no cost")]
struct Setup {}

#[::tokio::main]
async fn main() -> Result<()> {
    let cmd : SimpleSageCLI = argh::from_env();

    let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
    let sage_client = aws_sdk_sagemaker::Client::new(&config);
    let ecr_client = aws_sdk_ecr::Client::new(&config);
    let iam_client = aws_sdk_iam::Client::new(&config);
    let s3_client = aws_sdk_iam::Client::new(&config);

    let docker = docker::get_client().await; 

    match cmd.nested {
        SimpleSageSubCommands::Deploy(deploy) => process_deploy(&ecr_client, &sage_client, &docker, &deploy).await.unwrap(),
        SimpleSageSubCommands::Setup(setup) => process_setup(&ecr_client, &sage_client, &docker, &setup).await.unwrap(),
    }

    Ok(())
}

async fn process_deploy(ecr_client: &aws_sdk_ecr::Client, sage_client: &aws_sdk_sagemaker::Client, docker_client: &Docker, iam_client: &aws_sdk_iam::Client, s3_client: &aws_sdk_s3::Client, deploy_params: &Deploy) -> Result<()> {
    println!("Deploying model located at: {}", &deploy_params.wire_file);
    docker::build_image(&deploy_params.dockerfile, docker_client, &deploy_params.repo_name).await?; 
    docker::push_image(docker_client, ecr_client, &deploy_params.repo_name).await?;
    aws::create_sagemaker_role("simplesage-role", iam_client).await?;
    aws::create_sagemaker_bucket("simplesage-sagemaker", s3_client).await?;
    aws::create_sagemaker_model("TestModel", "simplesage-role", "container_image", sage_client).await?;
    aws::create_serverless_endpoint(endpoint_name, model_name, memory_size, max_concurrency, sage_client).await?;
    Ok(())
}

async fn process_setup(ecr_client: &aws_sdk_ecr::Client, sage_client: &aws_sdk_sagemaker::Client, docker_client: &Docker, deploy_params: &Setup) -> Result<()> {
    todo!()
}