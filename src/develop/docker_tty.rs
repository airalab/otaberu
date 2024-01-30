use bollard::container::{Config, RemoveContainerOptions};
use bollard::Docker;
use std::error::Error;

use bollard::exec::{CreateExecOptions, ResizeExecOptions, StartExecResults};
use bollard::image::CreateImageOptions;
use futures_util::{StreamExt, TryStreamExt};

use std::io::{stdout, Read, Write};
use std::time::Duration;
#[cfg(not(windows))]
use termion::raw::IntoRawMode;
#[cfg(not(windows))]
use termion::{async_stdin, terminal_size};
use tokio::io::AsyncWriteExt;
use tokio::task::spawn;
use tokio::time::sleep;

use crate::cli::Args;
use tracing::info;

const IMAGE: &str = "alpine:3";

pub async fn main_docker_tty(args: Args) -> Result<(), Box<dyn Error>> {
    let docker = Docker::connect_with_socket_defaults().unwrap();

    #[cfg(not(windows))]
    let tty_size = terminal_size()?;

    docker
        .create_image(
            Some(CreateImageOptions {
                from_image: IMAGE,
                ..Default::default()
            }),
            None,
            None,
        )
        .try_collect::<Vec<_>>()
        .await?;

    let alpine_config = Config {
        image: Some(IMAGE),
        tty: Some(true),
        ..Default::default()
    };

    let id = docker
        .create_container::<&str, &str>(None, alpine_config)
        .await?
        .id;
    docker.start_container::<String>(&id, None).await?;

    let exec = docker
        .create_exec(
            &id,
            CreateExecOptions {
                attach_stdout: Some(true),
                attach_stderr: Some(true),
                attach_stdin: Some(true),
                tty: Some(true),
                cmd: Some(vec!["sh"]),
                ..Default::default()
            },
        )
        .await?
        .id;

    #[cfg(not(windows))]
    if let StartExecResults::Attached {
        mut output,
        mut input,
    } = docker.start_exec(&exec, None).await?
    {
        // pipe stdin into the docker exec stream input
        let mut stdin = "ls\r\n".bytes();

        spawn(async move {
            loop {
                if let Some(byte) = stdin.next() {
                    input.write(&[byte]).await.ok();
                } else {
                    sleep(Duration::from_nanos(10)).await;
                }
            }
        });

        docker
            .resize_exec(
                &exec,
                ResizeExecOptions {
                    height: tty_size.1,
                    width: tty_size.0,
                },
            )
            .await?;

        while let Some(Ok(output)) = output.next().await {
            info!("{:?}", output.into_bytes());
        }
    }

    docker
        .remove_container(
            &id,
            Some(RemoveContainerOptions {
                force: true,
                ..Default::default()
            }),
        )
        .await?;
    info!("Docker exec done");
    Ok(())
}
