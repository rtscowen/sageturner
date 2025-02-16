use std::{fs::File, io::Read};

use anyhow::{anyhow, Result, Context};
use aws_sdk_ecr::{error::SdkError, operation::describe_repositories::DescribeRepositoriesError};
use base64::prelude::*;
use bollard::{auth::DockerCredentials, image::{BuildImageOptions, PushImageOptions, TagImageOptions}, secret::{BuildInfo, ImageId}, Docker};
use tempfile::tempdir; 
use tar::Builder;

use futures_util::stream::StreamExt;

pub async fn get_client() -> Docker {
    Docker::connect_with_socket_defaults().unwrap()
}

pub async fn build_image(path: &str, docker: &Docker, repo_name: &str) -> Result<()> {
    println!("Building docker image at {path}, to repo {repo_name}");
    let temp_dir = tempdir().unwrap();

    let tar_path = temp_dir.path().join("archive.tar");
    let tar_file = File::create(&tar_path).unwrap();
    let mut builder = Builder::new(tar_file);
    builder.append_dir_all("", path).unwrap();
    builder.finish().unwrap();

    
    let mut archive = File::open(tar_path).unwrap();
    let mut contents = Vec::new();
    archive.read_to_end(&mut contents).unwrap();
    
    let options = BuildImageOptions{
        dockerfile: "Dockerfile",
        t: repo_name, 
        rm: true,
        ..Default::default()
    };
    let mut build = docker.build_image(options, None, Some(contents.into()));

    let mut image_id: String = "".to_string();
    while let Some(msg) = build.next().await {
        let build_output = msg?;
        print!("{}", build_output.stream.unwrap_or_default());
        if let BuildInfo { aux: Some(ImageId { id: Some(id) }), .. } = build_output {
            image_id = id;
        }
    }

    if image_id.is_empty() {
        Err(anyhow!("No image tag"))
    } else {
        Ok(())
    }

}

pub async fn push_image(docker: &Docker, ecr_client: &aws_sdk_ecr::Client, repo_name: &str) -> Result<()> {
    println!("Pushing image to repo: {repo_name}");
    let repo_check = ecr_client.describe_repositories().repository_names(repo_name).send().await;
    let uri;
    match repo_check {
        Ok(desc) => {
            uri = desc.repositories()[0].repository_uri.clone().ok_or_else(|| anyhow!("Error reading repo URI"))?;
        },
        Err(err) => {
            match err.into_service_error() {
                DescribeRepositoriesError::RepositoryNotFoundException(_) => {
                    let new_repo = ecr_client.create_repository()
                        .repository_name(repo_name)
                        .send()
                        .await?;

                    let new_repo_info = new_repo.repository().ok_or_else(|| anyhow!("Error reading new repo info"))?;
                    uri = new_repo_info.repository_uri.clone().ok_or_else(|| anyhow!("Error reading new repo URI"))?
                },
                err @ _ => return Err(err.into()),
            };
        },
    };
        
    docker.tag_image(repo_name, Some(TagImageOptions{
        tag: "latest",
        repo: &uri
    })).await?;

    let push_options = Some(PushImageOptions::<String> {
        tag: "latest".to_string()
    });
    let credentials = get_docker_credentials(ecr_client).await?;
    let mut push_stream = docker.push_image(&uri, push_options, Some(credentials));

    while let Some(stream) = push_stream.next().await {
        let info = stream?;
        let progess = info.progress;
        println!("{:?}", progess)
    }
    Ok(())
}

async fn get_docker_credentials(ecr_client: &aws_sdk_ecr::Client) -> Result<DockerCredentials> {
    println!("Getting Docker Credentials ");
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