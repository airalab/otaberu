use rust_socketio::{asynchronous::Client, Payload};

use serde::{Deserialize, Serialize};

use tracing::info;

mod docker;

#[derive(Clone, Serialize, Deserialize)]
pub struct RobotJobResult {
    pub job_id: String,
    pub status: String,
    pub logs: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RobotJob {
    pub id: String,
    pub robot_id: String,
    pub job_type: String,
    pub status: String,
    pub args: String,
}

pub async fn launch_new_job(payload: Payload, socket: Client) {
    match payload {
        Payload::String(str) => {
            let robot_job: RobotJob = serde_json::from_str(&str).unwrap();
            info!("{:?}", robot_job);
            match robot_job.job_type.as_str() {
                "docker-container-launch" => {
                    info!("container launch");
                    tokio::spawn(docker::execute_launch(socket, robot_job));
                }
                _ => {}
            }
        }
        Payload::Binary(bin_data) => info!("{:?}", bin_data),
    };
}
