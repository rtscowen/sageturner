use std::path::Path;
use std::time::Duration;

use anyhow::{anyhow, Result};
use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::client::Waiters;
use aws_sdk_sagemaker::types::{
    ContainerDefinition, KendraSettings, ProductionVariant, ProductionVariantServerlessConfig
};
use base64::prelude::*;
use bollard::auth::DockerCredentials;

pub async fn get_role_arn(role_name: &str, client: &aws_sdk_iam::Client) -> Result<String> {
    match client.get_role().role_name(role_name).send().await {
        Ok(r) => {
            match r.role() {
                Some(r) => Ok(r.arn.clone()),
                None => {
                    return Err(anyhow!("Error getting role ARN"))
                },
            }
        },
        Err(e) => return Err(anyhow!("Error getting role ARN: {}", e)),
    }
}

pub async fn create_sagemaker_role(
    role_name: &str,
    client: &aws_sdk_iam::Client,
) -> Result<()> {
    let trust_policy = r#"{
        "Version": "2012-10-17",
        "Statement": [
            {
                "Effect": "Allow",
                "Principal": {
                    "Service": "sagemaker.amazonaws.com"
                },
                "Action": "sts:AssumeRole"
            }
        ]
    }"#;

    println!("role: {}", role_name);
    client
        .create_role()
        .role_name(role_name)
        .assume_role_policy_document(trust_policy)
        .send()
        .await?;

    println!("Attaching policy");
    client
        .attach_role_policy()
        .role_name(role_name)
        .policy_arn("arn:aws:iam::aws:policy/AmazonSageMakerFullAccess")
        .send()
        .await?;
    println!("Role created");
    Ok(())
}

pub async fn create_sagemaker_bucket(bucket_name: &str, client: &aws_sdk_s3::Client) -> Result<()> {
    println!("bucket: {}", bucket_name);
    println!("Checking if bucket already exists");
    let already_exists = match client.head_bucket().bucket(bucket_name).send().await {
        Ok(_) => true,
        Err(_) => false,
    };

    if !already_exists {
        println!("Creating bucket");
        let constraint = aws_sdk_s3::types::BucketLocationConstraint::from("eu-west-2".to_string().as_str());
        let cfg = aws_sdk_s3::types::CreateBucketConfiguration::builder()
            .location_constraint(constraint)
            .build();
        client
            .create_bucket()
            .bucket(bucket_name)
            .create_bucket_configuration(cfg)
            .send()
            .await?;
    }
    Ok(())
}

pub async fn get_docker_credentials_for_ecr(
    ecr_client: &aws_sdk_ecr::Client,
) -> Result<DockerCredentials> {
    println!("Getting Docker Credentials");
    // assume default registry
    let ecr_auth = ecr_client.get_authorization_token().send().await?;

    let token = ecr_auth.authorization_data()[0]
        .authorization_token()
        .ok_or_else(|| anyhow!("Couldn't read auth token"))?;
    let decoded_token = BASE64_STANDARD.decode(token)?;

    let parts: Vec<_> = decoded_token.split(|c| *c == b':').collect(); // split at : for user:password
    let username = String::from_utf8(parts[0].to_vec()).unwrap();
    let password = String::from_utf8(parts[1].to_vec()).unwrap();

    let endpoint = ecr_auth.authorization_data()[0]
        .proxy_endpoint()
        .ok_or_else(|| anyhow!("Couldn't read proxy endpoint"))?;

    Ok(DockerCredentials {
        username: Some(username),
        password: Some(password),
        serveraddress: Some(endpoint.to_string()),
        ..Default::default()
    })
}

pub async fn create_sagemaker_model(
    model_name: &str,
    execution_role_arn: &str,
    container_image: &str,
    sage_client: &aws_sdk_sagemaker::Client,
    model_data_url: Option<String>,
) -> Result<()> {
    let container = match model_data_url {
        Some(u) => {
            ContainerDefinition::builder()
                .image(container_image)
                .model_data_url(u)
                .build()
        }
        None => {
            ContainerDefinition::builder()
                .image(container_image)
                .build()
        }
    };

    sage_client
        .create_model()
        .set_model_name(Some(model_name.to_string()))
        .set_execution_role_arn(Some(execution_role_arn.to_string()))
        .set_primary_container(Some(container))
        .send()
        .await?;
    Ok(())
}

pub async fn create_serverless_endpoint(
    endpoint_name: &str,
    model_name: &str,
    memory_size: i32,
    max_concurrency: i32,
    provisioned_concurrency: i32,
    execution_role_arn: &str,
    sage_client: &aws_sdk_sagemaker::Client,
) -> Result<()> {
    println!(
        "Creating serverless endpoint {}. Might take a few mins.",
        endpoint_name
    );
    let serverless_config = ProductionVariantServerlessConfig::builder()
        .max_concurrency(max_concurrency)
        .memory_size_in_mb(memory_size)
        .provisioned_concurrency(provisioned_concurrency)
        .build();

    let production_variant = ProductionVariant::builder()
        .variant_name("sageturner-variant-1")
        .model_name(model_name)
        .serverless_config(serverless_config)
        .build();

    let endpoint_config_name = format!("{}-config", endpoint_name);

    sage_client
        .create_endpoint_config()
        .endpoint_config_name(&endpoint_config_name)
        .production_variants(production_variant)
        .send()
        .await?;

    sage_client
        .create_endpoint()
        .endpoint_name(endpoint_name)
        .endpoint_config_name(&endpoint_config_name)
        .send()
        .await?;

    println!(
        "Serverless endpoint {} created successfully. It may take a few mins to go live.",
        endpoint_name
    );
    Ok(())
}

pub async fn create_server_endpoint(
    endpoint_name: &str,
    model_name: &str,
    instance_type: &str,
    initial_instance_count: i32,
    execution_role_arn: &str,
    sage_client: &aws_sdk_sagemaker::Client,
) -> Result<()> {
    println!(
        "Creating server endpoint {}. Might take a few mins.",
        endpoint_name
    );
    let production_variant = ProductionVariant::builder()
        .variant_name("sageturner-variant-1")
        .model_name(model_name)
        .instance_type(instance_type.into())
        .initial_instance_count(initial_instance_count)
        .build();

    let endpoint_config_name = format!("{}-config", endpoint_name);

    sage_client
        .create_endpoint_config()
        .endpoint_config_name(&endpoint_config_name)
        .production_variants(production_variant)
        .execution_role_arn(execution_role_arn)
        .send()
        .await?;

    sage_client
        .create_endpoint()
        .endpoint_name(endpoint_name)
        .endpoint_config_name(&endpoint_config_name)
        .send()
        .await?;

    println!(
        "Server endpoint {} created successfully. It may take a few mins to go live.",
        endpoint_name
    );
    Ok(())
}

pub async fn upload_artefact(
    object_path: &str,
    bucket_name: &str,
    s3_key: &str,
    s3_client: &aws_sdk_s3::Client,
) -> Result<String> {
    println!("Uploading file {} to bucket {} with key {}", object_path, bucket_name, s3_key);
    if !is_tar_gz(object_path) {
        return Err(anyhow!("Artefact needs to be a .tar.gz file (ask perplexity how to create one, if you're not sure"));
    }
    let body = ByteStream::from_path(Path::new(object_path)).await?;
    s3_client
        .put_object()
        .bucket(bucket_name)
        .key(s3_key)
        .body(body)
        .send()
        .await?;

    s3_client.wait_until_object_exists()
        .bucket(bucket_name)
        .key(s3_key)
        .wait(Duration::from_secs(8))
        .await?;

    let s3_path = format!("s3://{}/{}", bucket_name, s3_key);
    Ok(s3_path)
}

fn is_tar_gz(file_path: &str) -> bool {
    Path::new(file_path)
        .extension()
        .is_some_and(|ext| ext == "gz")
        && Path::new(file_path)
            .file_stem()
            .and_then(|stem| Path::new(stem).extension())
            .is_some_and(|ext| ext == "tar")
}
