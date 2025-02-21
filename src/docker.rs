use std::{
    collections::HashMap,
    fs::File,
    io::{Read, Write},
};

use anyhow::{anyhow, Result};
use aws_sdk_ecr::operation::describe_repositories::DescribeRepositoriesError;
use bollard::{
    image::{BuildImageOptions, PushImageOptions, TagImageOptions},
    secret::{BuildInfo, ImageId},
    Docker,
};
use tar::Builder;
use tempfile::tempdir;

use futures_util::stream::StreamExt;

use crate::aws::get_docker_credentials_for_ecr;

pub async fn get_client() -> Docker {
    Docker::connect_with_socket_defaults().unwrap()
}

pub async fn build_image_byo(path: &str, docker: &Docker, repo_name: &str) -> Result<()> {
    println!("Building your docker image at {path}, as {repo_name}:latest");
    let temp_dir = tempdir()?;

    let tar_path = temp_dir.path().join("archive_byo.tar");
    let tar_file = File::create(&tar_path).unwrap();
    let mut builder = Builder::new(tar_file);
    builder.append_dir_all("", path).unwrap();
    builder.finish().unwrap();

    let mut archive = File::open(tar_path).unwrap();
    let mut contents = Vec::new();
    archive.read_to_end(&mut contents).unwrap();

    let options = BuildImageOptions {
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
        if let BuildInfo {
            aux: Some(ImageId { id: Some(id) }),
            ..
        } = build_output
        {
            image_id = id;
        }
    }

    if image_id.is_empty() {
        Err(anyhow!("No image tag"))
    } else {
        Ok(())
    }
}

pub async fn build_image_ez_mode(
    gpu: bool,
    extra_python: &str,
    extra_system: &str,
    name: &str,
    serve_code: &str,
    docker_client: &Docker,
) -> Result<()> {
    println!("Building dynamically generated image, with Python packages {}, and system packages {}, and your serve code", extra_python, extra_system);
    let dockerfile_contents = if gpu {
        gpu_dockerfile()
    } else {
        cpu_dockerfile()
    };

    let tempdir = tempdir()?;

    let docker_path = tempdir.path().join("Dockerfile");
    let mut docker_file = File::create(docker_path)?;
    docker_file.write_all(dockerfile_contents.as_bytes())?;

    let python_path = tempdir.path().join("serve.py");
    let mut python_file = File::create(python_path)?;
    python_file.write_all(serve_code.as_bytes())?;

    let tar_path = tempdir.path().join("archive_ez.tar");
    let tar_file = File::create(&tar_path)?;
    let mut builder = Builder::new(tar_file);
    builder.append_file("Dockerfile", &mut docker_file)?;
    builder.append_file("serve.py", &mut python_file)?;
    builder.finish()?;

    let mut archive = File::open(tar_path)?;
    let mut contents = Vec::new();
    archive.read_to_end(&mut contents)?;

    let mut build_args = HashMap::new();
    build_args.insert("EXTRA_PYTHON_PACKAGES", extra_python);
    build_args.insert("EXTRA_SYSTEM_PACKAGES", extra_system);

    let options = BuildImageOptions {
        dockerfile: "Dockerfile",
        t: name,
        rm: true,
        buildargs: build_args,
        ..Default::default()
    };
    let mut build = docker_client.build_image(options, None, Some(contents.into()));

    let mut image_id: String = "".to_string();
    while let Some(msg) = build.next().await {
        let build_output = msg?;
        print!("{}", build_output.stream.unwrap_or_default());
        if let BuildInfo {
            aux: Some(ImageId { id: Some(id) }),
            ..
        } = build_output
        {
            image_id = id;
        }
    }

    if image_id.is_empty() {
        Err(anyhow!("No image tag"))
    } else {
        Ok(())
    }
}

pub async fn push_image(
    docker: &Docker,
    ecr_client: &aws_sdk_ecr::Client,
    image_name: &str,
) -> Result<String> {
    println!("Pushing image {} to ECR", image_name);
    let repo_check = ecr_client
        .describe_repositories()
        .repository_names(image_name)
        .send()
        .await;
    let uri;
    match repo_check {
        Ok(desc) => {
            uri = desc.repositories()[0]
                .repository_uri
                .clone()
                .ok_or_else(|| anyhow!("Error reading repo URI"))?;
        }
        Err(err) => {
            match err.into_service_error() {
                DescribeRepositoriesError::RepositoryNotFoundException(_) => {
                    let new_repo = ecr_client
                        .create_repository()
                        .repository_name(image_name)
                        .send()
                        .await?;

                    let new_repo_info = new_repo
                        .repository()
                        .ok_or_else(|| anyhow!("Error reading new repo info"))?;
                    uri = new_repo_info
                        .repository_uri
                        .clone()
                        .ok_or_else(|| anyhow!("Error reading new repo URI"))?
                }
                err => return Err(err.into()),
            };
        }
    };

    docker
        .tag_image(
            image_name,
            Some(TagImageOptions {
                tag: "latest",
                repo: &uri,
            }),
        )
        .await?;

    let push_options = Some(PushImageOptions::<String> {
        tag: "latest".to_string(),
    });
    let credentials = get_docker_credentials_for_ecr(ecr_client).await?;
    let mut push_stream = docker.push_image(&uri, push_options, Some(credentials));

    while let Some(stream) = push_stream.next().await {
        let info = stream?;
        let progess = info.progress;
        println!("{:?}", progess)
    }
    Ok(uri)
}

fn cpu_dockerfile() -> String {
    let content = r#"
    FROM UBUNTU:22.04
    SHELL ["/bin/bash", "-c"]

    ARG PYTHON_VERSION="3.12"
    ARG EXTRA_PYTHON_PACKAGES
    ARG EXTRA_SYSTEM_PACKAGES
    ARG LOAD_AND_PREDICT_SCRIPT_PATH

    RUN apt-get -y update && DEBIAN_FRONTEND=noninteractive apt-get -y install --no-install-recommends \
        build-essential libssl-dev zlib1g-dev \
        libbz2-dev libreadline-dev libsqlite3-dev curl git \
        libncursesw5-dev xz-utils tk-dev libxml2-dev libxmlsec1-dev libffi-dev liblzma-dev wget

    ENV HOME=/home/root 
    RUN curl -fsSL https://pyenv.run | bash
    ENV PYENV_ROOT=${HOME}/.pyenv
    ENV PATH=${PYENV_ROOT}/shims:${PYENV_ROOT}/bin:$PATH

    RUN pyenv install ${PYTHON_VERSION}
    RUN pyenv global ${PYTHON_VERSION}

    # Install extra system packages
    RUN if [ ${EXTRA_SYSTEM_PACKAGES} != "" ]; then \
            apt-get -y install --no-install-recommends ${EXTRA_SYSTEM_PACKAGES} \
        fi

    # Install FastAPI as standard 
    RUN pip install fastapi[standard]

    # Install extra python packages 
    RUN if [ ${EXTRA_PYTHON_PACKAGES} != "" ]; then \
            pip install --no-input ${EXTRA_PYTHON_PACKAGES} \
        fi

    ENV PYTHONUNBUFFERED=TRUE
    ENV PYTHONDONTWRITEBYTECODE=TRUE
    ENV PATH="${PATH}:/opt/program/"

    COPY ${LOAD_AND_PREDICT_SCRIPT_PATH} /opt/program/
    COPY serve.py /opt/program/
    WORKDIR /opt/program/

    ENTRYPOINT [ "python", "serve.py" ]
    "#;
    content.to_string()
}

fn gpu_dockerfile() -> String {
    let content = r#"
    FROM UBUNTU:22.04
    SHELL ["/bin/bash", "-c"]

    ARG PYTHON_VERSION="3.12"
    ARG EXTRA_PYTHON_PACKAGES
    ARG EXTRA_SYSTEM_PACKAGES
    ARG LOAD_AND_PREDICT_SCRIPT_PATH 

    RUN apt-get -y update && DEBIAN_FRONTEND=noninteractive apt-get -y install --no-install-recommends \
        build-essential libssl-dev zlib1g-dev \
        libbz2-dev libreadline-dev libsqlite3-dev curl git \
        libncursesw5-dev xz-utils tk-dev libxml2-dev libxmlsec1-dev libffi-dev liblzma-dev wget

    RUN wget https://developer.download.nvidia.com/compute/cuda/repos/ubuntu2204/x86_64/cuda-ubuntu2204.pin --no-check-certificate && \
        mv cuda-ubuntu2204.pin /etc/apt/preferences.d/cuda-repository-pin-600 && \
        wget https://developer.download.nvidia.com/compute/cuda/12.8.0/local_installers/cuda-repo-ubuntu2204-12-8-local_12.8.0-570.86.10-1_amd64.deb --no-check-certificate && \
        dpkg -i cuda-repo-ubuntu2204-12-8-local_12.8.0-570.86.10-1_amd64.deb && \
        cp /var/cuda-repo-ubuntu2204-12-8-local/cuda-*-keyring.gpg /usr/share/keyrings/ && \
        apt-get -y update && apt-get -y install cuda-toolkit-12-8

    ENV HOME=/home/root 
    RUN curl -fsSL https://pyenv.run | bash
    ENV PYENV_ROOT=${HOME}/.pyenv
    ENV PATH=${PYENV_ROOT}/shims:${PYENV_ROOT}/bin:$PATH

    RUN pyenv install ${PYTHON_VERSION}
    RUN pyenv global ${PYTHON_VERSION}

    # Install extra system packages
    RUN if [ ${EXTRA_SYSTEM_PACKAGES} != "" ]; then \
            apt-get -y install --no-install-recommends ${EXTRA_SYSTEM_PACKAGES} \
        fi

    # Install FastAPI as standard 
    RUN pip install fastapi[standard]

    # Install extra python packages 
    RUN if [ ${EXTRA_PYTHON_PACKAGES} != "" ]; then \
            pip install --no-input ${EXTRA_PYTHON_PACKAGES} \
        fi

    ENV PYTHONUNBUFFERED=TRUE
    ENV PYTHONDONTWRITEBYTECODE=TRUE
    ENV PATH="${PATH}:/opt/program/"

    COPY serve.py /opt/program/
    COPY ${LOAD_AND_PREDICT_SCRIPT_PATH} /opt/program/
    WORKDIR /opt/program/

    ENTRYPOINT [ "python", "serve.py" ]
    "#;
    content.to_string()
}
