use std::{fs::File, io::Read};

use bollard::{image::BuildImageOptions, Docker};
use tempfile::tempdir; 
use tar::Builder;

use futures_util::stream::StreamExt;

pub async fn build_image(path: &str) {
    let docker = Docker::connect_with_socket_defaults().unwrap(); 

    let temp_dir = tempdir().unwrap();

    let tar_path = temp_dir.path().join("archive.tar");
    let tar_file = File::create(&tar_path).unwrap();
    let mut builder = Builder::new(tar_file);
    builder.append_dir_all("", path).unwrap();
    builder.finish().unwrap();

    let options = BuildImageOptions{
        dockerfile: "Dockerfile",
        t: "image", 
        rm: true,
        ..Default::default()
    };

    let mut archive = File::open(tar_path).unwrap();
    let mut contents = Vec::new();
    archive.read_to_end(&mut contents).unwrap();

    let mut build = docker.build_image(options, None, Some(contents.into()));

    while let Some(msg) = build.next().await {
        let status = msg.unwrap().status.unwrap();
        println!("Building docker image.... {status}");
    }


}