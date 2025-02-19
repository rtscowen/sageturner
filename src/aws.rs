// AWS fuckery 

// create sagemaker role - if it doesn't exist simplesage-role
// create S3 bucket - simplesage-sagemaker
// 

// have default buckets and default roles that it creates; 
// in the wire file, allow an override bucket and role 

use anyhow::{anyhow, Result};
use aws_sdk_sagemaker::types::{builders::ProductionVariantServerlessConfigBuilder, ContainerDefinition, ModelInput, ProductionVariant, ProductionVariantServerlessConfig};
use base64::prelude::*;
use bollard::auth::DockerCredentials;

pub async fn create_sagemaker_role(role_name: &str, client: &aws_sdk_iam::Client) -> Result<String> {
    // Define the trust policy
    let trust_policy = r#"
    {
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

    let role = client.create_role()
        .role_name(role_name)
        .assume_role_policy_document(trust_policy)
        .send()
        .await;

    let arn; 
    match role {
        Ok(r) => {
            match r.role() {
                Some(r) => {
                    arn = r.arn.clone();
                },
                None => {return Err(anyhow!("Role shouldn't be empty"))},
            }
        },
        Err(err) => {
            match err.into_service_error() {
                aws_sdk_iam::operation::create_role::CreateRoleError::EntityAlreadyExistsException(entity_already_exists_exception) => {
                    println!("Role with that name already exists, getting role ARN and returning early to avoid duplicate policy attachment.");
                    match client.get_role().role_name(role_name).send().await {
                        Ok(r) => {
                            match r.role() {
                                Some(r) => {
                                    arn = r.arn.clone();
                                    return Ok(arn);
                                },
                                None => {return Err(anyhow!("Role shouldn't be empty"))},
                            }
                        },
                        Err(_) => {return Err(anyhow!("Shouldn't have failed to get this role"))},
                    }
                },
                err @ _ => return Err(err.into()),
            }
        },
    }
    

    client.attach_role_policy()
        .role_name(role_name)
        .policy_arn("arn:aws:iam::aws:policy/AmazonSageMakerFullAccess")
        .send()
        .await?;

    Ok(arn)
}

pub async fn create_sagemaker_bucket(bucket_name: &str, client: &aws_sdk_s3::Client) -> Result<()> {
    let bucket = client.create_bucket()
        .bucket(bucket_name)
        .send()
        .await;

    match bucket {
        Ok(_) => {Ok(())},
        Err(err) => match err.into_service_error() {
            aws_sdk_s3::operation::create_bucket::CreateBucketError::BucketAlreadyExists(_) => {
                println!("Bucket already exists");
                Ok(())
            },
            err @ _ => return Err(err.into()),
        },
    }
}

pub async fn get_docker_credentials_for_ecr(ecr_client: &aws_sdk_ecr::Client) -> Result<DockerCredentials> {
    println!("Getting Docker Credentials");
    // assume default registry
    let ecr_auth = ecr_client.get_authorization_token()
        .send()
        .await?;

    let token = ecr_auth.authorization_data()[0].authorization_token().ok_or_else(|| anyhow!("Couldn't read auth token"))?;
    let decoded_token = BASE64_STANDARD.decode(token)?;

    let parts: Vec<_> = decoded_token.split(|c| *c == b':').collect(); // split at : for user:password
    let username = String::from_utf8(parts[0].to_vec()).unwrap();
    let password = String::from_utf8(parts[1].to_vec()).unwrap();

    let endpoint = ecr_auth.authorization_data()[0].proxy_endpoint().ok_or_else(|| anyhow!("Couldn't read proxy endpoint"))?;

    Ok(DockerCredentials{
        username: Some(username),
        password: Some(password),
        serveraddress: Some(endpoint.to_string()),
        ..Default::default()
    })
}

pub async fn create_sagemaker_model(model_name: &str, execution_role_arn: &str, container_image: &str, sage_client: &aws_sdk_sagemaker::Client) -> Result<()> {
    let container = ContainerDefinition::builder()
        .image(container_image)
        .model_data_url(input)
        .build();

    sage_client.create_model()
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
    sage_client: &aws_sdk_sagemaker::Client
) -> Result<()> {

    let serverless_config = ProductionVariantServerlessConfig::builder()
        .max_concurrency(max_concurrency)
        .memory_size_in_mb(memory_size)
        .build();

    let production_variant = ProductionVariant::builder()
        .variant_name("variant-1")
        .model_name(model_name)
        .serverless_config(serverless_config)
        .build();

    let endpoint_config_name = format!("{}-config", endpoint_name);

    sage_client.create_endpoint_config()
        .endpoint_config_name(&endpoint_config_name)
        .production_variants(production_variant)
        .send()
        .await?;

    sage_client.create_endpoint()
        .endpoint_name(endpoint_name)
        .endpoint_config_name(&endpoint_config_name)
        .send()
        .await?;

    println!("Serverless endpoint {} created successfully", endpoint_name);
    Ok(())
}