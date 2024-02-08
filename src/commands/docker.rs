use bollard;
use bollard::Docker;
use rust_socketio::asynchronous::Client;

use futures_util::{StreamExt, TryStreamExt};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio;
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;

use std;
use tracing::{error, info};

use serde::{Deserialize, Serialize};

use crate::{
    commands::{RobotJob, RobotJobResult},
    store::{JobManager, Jobs},
};

pub async fn execute_launch(socket: Client, robot_job: RobotJob, jobs: Jobs) {
    let args = serde_json::from_str::<DockerLaunchArgs>(&robot_job.args).unwrap();
    info!("launching docker job {:?}", args);
    let docker_launch = DockerLaunch { args };
    let robot_job_result = match docker_launch.execute(robot_job.clone(), jobs).await {
        Ok(result) => {
            info!("job successfully executed");
            result
        }
        Err(error) => {
            error!("error {:?}", error);
            RobotJobResult {
                job_id: robot_job.id,
                status: String::from("error"),
                logs: error.to_string(),
            }
        }
    };
    let _ = socket
        .emit("job_done", serde_json::json!(robot_job_result))
        .await;
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct DockerLaunchArgs {
    pub image: String,
    pub container_name: String,
    pub custom_cmd: Option<String>,
    pub save_logs: Option<bool>,
    pub network_mode: String,
    pub ports: Vec<DockerMap>,
    pub volumes: Vec<DockerMap>,
    pub env: Vec<String>,
    pub privileged: bool,
}
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct DockerMap {
    key: String,
    value: String,
}

pub struct DockerLaunch {
    pub args: DockerLaunchArgs,
}

impl DockerLaunch {
    pub async fn execute(
        &self,
        robot_job: RobotJob,
        jobs: Jobs,
    ) -> Result<RobotJobResult, bollard::errors::Error> {
        info!("launching docker with image {}", self.args.image);
        let docker = Docker::connect_with_socket_defaults().unwrap();
        info!("docker init");
        docker
            .create_image(
                Some(bollard::image::CreateImageOptions {
                    from_image: self.args.image.as_str(),
                    ..Default::default()
                }),
                None,
                None,
            )
            .try_collect::<Vec<_>>()
            .await?;

        info!("docker image pulled");

        let mut volumes = vec![];
        for volume_pair in self.args.volumes.iter() {
            volumes.push(format!("{}:{}", volume_pair.key, volume_pair.value))
        }

        let mut config = bollard::container::Config::<&str> {
            image: Some(&self.args.image),
            env: Some(self.args.env.iter().map(|s| s as &str).collect()),
            host_config: Some(bollard::models::HostConfig {
                privileged: Some(self.args.privileged),
                network_mode: Some(self.args.network_mode.clone()),
                binds: Some(volumes),
                ..Default::default()
            }),
            ..Default::default()
        };
        match &self.args.custom_cmd {
            Some(custom_cmd) => {
                config.tty = Some(true);
                config.cmd = Some(custom_cmd.split(" ").collect::<Vec<&str>>())
            }
            None => {}
        }

        let id = docker
            .create_container::<&str, &str>(
                Some(bollard::container::CreateContainerOptions {
                    name: &self.args.container_name,
                    platform: None,
                }),
                config,
            )
            .await?
            .id;
        info!("created container with id {}", id);

        docker.start_container::<String>(&id, None).await?;

        let mut concatenated_logs: String = String::new();

        match &self.args.custom_cmd {
            Some(custom_cmd) => {
                let exec = docker
                    .create_exec(
                        &id,
                        bollard::exec::CreateExecOptions {
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
                if let bollard::exec::StartExecResults::Attached {
                    mut output,
                    mut input,
                } = docker.start_exec(&exec, None).await?
                {
                    // pipe stdin into the docker exec stream input
                    {
                        let shared_jobs = Arc::clone(&jobs);
                        let job_manager = shared_jobs.lock().unwrap();
                        let channel_to_job_tx =
                            job_manager.get_channel_to_job_tx_by_job_id(&robot_job.id);
                        if let Some(channel_to_job_tx) = channel_to_job_tx {
                            tokio::task::spawn(async move {
                                let mut channel_to_job_rx = channel_to_job_tx.subscribe();
                                loop {
                                    let data = channel_to_job_rx.recv().await.unwrap();
                                    for byte in data.as_bytes().iter() {
                                        input.write(&[*byte]).await.ok();
                                    }
                                }
                            });
                        }
                    }

                    docker
                        .resize_exec(
                            &exec,
                            bollard::exec::ResizeExecOptions {
                                height: 35,
                                width: 100,
                            },
                        )
                        .await?;
                    {
                        let shared_jobs = Arc::clone(&jobs);
                        while let Some(Ok(output)) = output.next().await {
                            let job_manager = shared_jobs.lock().unwrap();
                            if let Some(tx) = job_manager.get_channel_by_job_id(&robot_job.id) {
                                tx.send(output.to_string()).unwrap();
                            }

                            info!("{:?}", output.into_bytes());
                        }
                    }
                }
            }
            None => {
                let logs_options: bollard::container::LogsOptions<String> =
                    bollard::container::LogsOptions {
                        follow: true,
                        stdout: true,
                        stderr: true,
                        ..Default::default()
                    };

                let mut logs = docker.logs(&id, Some(logs_options));

                while let Some(log) = logs.try_next().await? {
                    concatenated_logs
                        .push_str(std::str::from_utf8(&log.into_bytes()).unwrap_or(""));
                }

                info!("log: {}", concatenated_logs);
            }
        }

        docker
            .remove_container(
                &id,
                Some(bollard::container::RemoveContainerOptions {
                    force: true,
                    ..Default::default()
                }),
            )
            .await?;

        let robot_job_result = RobotJobResult {
            job_id: robot_job.id,
            status: String::from("done"),
            logs: concatenated_logs,
        };

        Ok(robot_job_result)
    }
}
