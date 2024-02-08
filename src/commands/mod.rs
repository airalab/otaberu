use futures_util::future::err;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc};

use rust_socketio::{asynchronous::Client, Payload};

use serde::{Deserialize, Serialize};

use serde_json::json;
use tracing::info;

use crate::store;

mod docker;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartTunnelReq {
    pub job_id: String,
    pub client_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageToRobot{
    pub job_id: String,
    pub content: String 
}

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RobotStartTunnelResponse{
    is_ok: bool,
    error: Option<String>
}

pub async fn launch_new_job(payload: Payload, socket: Client, jobs: store::Jobs) {
    match payload {
        Payload::String(str) => {
            let robot_job: RobotJob = serde_json::from_str(&str).unwrap();
            info!("{:?}", robot_job);

            match robot_job.job_type.as_str() {
                "docker-container-launch" => {
                    info!("container launch");
                    let mut job_manager = jobs.lock().unwrap();
                    job_manager.new_job(
                        robot_job.id.clone(),
                        robot_job.job_type.clone(),
                        robot_job.status.clone(),
                    );
                    let shared_jobs = Arc::clone(&jobs);
                    tokio::spawn(docker::execute_launch(socket, robot_job, shared_jobs));
                }
                _ => {}
            }
        }
        Payload::Binary(bin_data) => info!("{:?}", bin_data),
    };
}

pub async fn start_tunnel_messanger(
    socket: Client,
    tx: broadcast::Sender<String>,
    client_id: String,
) {
    let mut rx = tx.subscribe();
    loop {
        let data = rx.recv().await.unwrap();
        info!("from docker: {}", data);
        let _res: Result<(), rust_socketio::Error> = socket
            .emit(
                "message_to_client",
                json!({"client_id": client_id, "content": data}),
            )
            .await;
    }
}

pub async fn start_tunnel(payload: Payload, socket: Client, jobs: store::Jobs) {
    let mut ack_result = RobotStartTunnelResponse{
        is_ok: false,
        error: None
    };
    match payload {
        Payload::String(str) => {
            info!("Start tunnel request");
            let start_tunnel_request: StartTunnelReq = serde_json::from_str(&str).unwrap();
            info!("Start tunnel: {:?}", start_tunnel_request);
            let mut job_manager = jobs.lock().unwrap();

            match job_manager.get_job_or_none(&start_tunnel_request.job_id) {
                Some(job) => {
                    job_manager.create_job_tunnel(
                        &start_tunnel_request.job_id,
                        start_tunnel_request.client_id.clone(),
                    );
                    if let Some(channel_tx) =
                        job_manager.get_channel_by_job_id(&start_tunnel_request.job_id)
                    {
                        tokio::spawn(start_tunnel_messanger(
                            socket,
                            channel_tx.clone(),
                            start_tunnel_request.client_id.clone(),
                        ));
                        ack_result.is_ok = true;
                    } else {
                        info!("no channel tx");
                    }
                }
                None => {
                    //todo: socket res
                }
            }
        }
        Payload::Binary(bin_data) => info!("{:?}", bin_data),
    };
}


pub async fn message_to_robot(payload: Payload, socket: Client, jobs: store::Jobs) {
    match payload {
        Payload::String(str) => {
            info!("Message to robot request");
            let message: MessageToRobot = serde_json::from_str(&str).unwrap();
            info!("Message to robot: {:?}", message);
            let mut job_manager = jobs.lock().unwrap();

            match job_manager.get_job_or_none(&message.job_id) {
                Some(job) => {
                    if let Some(channel) = job_manager.get_channel_to_job_tx_by_job_id(&message.job_id){
                        channel.send(message.content).unwrap();
                    };
                }
                None => {
                    //todo: socket res
                }
            }
        }
        Payload::Binary(bin_data) => info!("{:?}", bin_data),
    };
}
