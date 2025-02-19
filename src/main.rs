use argh::FromArgs;

use anyhow::{anyhow, Result};
use bollard::Docker;

mod docker;
mod aws;
mod model_config;

#[derive(Debug, FromArgs, PartialEq)]
#[argh(description="SimpleSage deploys your models to AWS SageMaker in one command")]
struct SimpleSageCLI {
    #[argh(subcommand)]
    nested: SimpleSageSubCommands
}

#[derive(Debug, FromArgs, PartialEq)]
#[argh(subcommand)]
enum SimpleSageSubCommands {
    Deploy(Deploy),
}

#[derive(Debug, FromArgs, PartialEq)]
#[argh(subcommand, name="deploy", description="Deploy serialised models directly to Sagemaker endpoint")]
struct Deploy {
    #[argh(option, short='e', description="endpoint type (only serverless supported)", default="String::from(\"serverless\")")]
    endpoint_type: String, 

    #[argh(option, short='m', description="path to model config YAML")]
    model_config: String
}

#[::tokio::main]
async fn main() -> Result<()> {
    let cmd : SimpleSageCLI = argh::from_env();

    let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
    let sage_client = aws_sdk_sagemaker::Client::new(&config);
    let ecr_client = aws_sdk_ecr::Client::new(&config);
    let iam_client = aws_sdk_iam::Client::new(&config);
    let s3_client = aws_sdk_s3::Client::new(&config);

    let docker = docker::get_client().await; 

    match cmd.nested {
        SimpleSageSubCommands::Deploy(deploy) => process_deploy(&ecr_client, &sage_client, &docker, &iam_client, &s3_client, &deploy).await.unwrap(),
    }

    Ok(())
}

async fn process_deploy(ecr_client: &aws_sdk_ecr::Client, sage_client: &aws_sdk_sagemaker::Client, docker_client: &Docker, iam_client: &aws_sdk_iam::Client, s3_client: &aws_sdk_s3::Client, deploy_params: &Deploy) -> Result<()> {
    println!("Deploying model with config {} to {} endpoint type", &deploy_params.model_config, &deploy_params.endpoint_type);

    let model_config = model_config::parse_config(deploy_params.model_config.into())?;

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
            exec_role_arn = aws::create_sagemaker_role("simplesage-role", iam_client).await?;
        } else if should_override_role {
            // override role and use default for bucket 
            let role_name = overrides.role.ok_or_else(|| anyhow!("Something went wrong with our config parsing. Raise an issue"))?;
            aws::create_sagemaker_bucket("simplesage-sagemaker", s3_client).await?;
            exec_role_arn = aws::create_sagemaker_role(&role_name, iam_client).await?;
        }
    } else {
        exec_role_arn = aws::create_sagemaker_role("simplesage-role", iam_client).await?;
        aws::create_sagemaker_bucket("simplesage-sagemaker", s3_client).await?; // needs to have sagemaker in it for policy
    }

    aws::create_sagemaker_model(&model_config.name, &exec_role_arn, &uri, sage_client).await?;

    aws::create_serverless_endpoint(endpoint_name, model_name, memory_size, max_concurrency, sage_client).await?;
    Ok(())
}