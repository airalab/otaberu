use bollard;
use bollard::Docker;
use rust_socketio::asynchronous::Client;

use futures_util::TryStreamExt;
use std;
use tracing::{error, info};

use serde::{Deserialize, Serialize};

use crate::commands::{RobotJob, RobotJobResult};

pub async fn execute_launch(socket: Client, robot_job: RobotJob) {
    let args = serde_json::from_str::<DockerLaunchArgs>(&robot_job.args).unwrap();
    info!("launching docker job {:?}", args);
    let docker_launch = DockerLaunch { args };

    let robot_job_result = match docker_launch.execute(robot_job.clone()).await {
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
    pub custom_cmd: String,
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
        if self.args.custom_cmd.len() > 0 {
            config.cmd = Some(self.args.custom_cmd.split(" ").collect::<Vec<&str>>())
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

        let logs_options: bollard::container::LogsOptions<String> =
            bollard::container::LogsOptions {
                follow: true,
                stdout: true,
                stderr: true,
                ..Default::default()
            };

        let mut logs = docker.logs(&id, Some(logs_options));
        let mut concatenated_logs: String = String::new();

        while let Some(log) = logs.try_next().await? {
            concatenated_logs.push_str(std::str::from_utf8(&log.into_bytes()).unwrap_or(""));
        }

        info!("log: {}", concatenated_logs);

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
