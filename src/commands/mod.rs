use std::sync::Arc;
use tokio::sync::broadcast;

use serde::{Deserialize, Serialize};

use tokio::sync::broadcast::Sender;
use tracing::{error, info};

use crate::store;
use crate::store::messages::{Message, ChannelMessageFromJob};
use crate::store::job_manager::Jobs;

mod docker;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartTunnelReq {
    pub job_id: String,
    pub client_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageToRobot {
    pub job_id: String,
    pub content: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum MessageContent {
    Terminal { stdin: String },
    Archive { dest_path: String, data: String },
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
pub struct RobotStartTunnelResponse {
    is_ok: bool,
    error: Option<String>,
}
pub enum TunnnelClient {
    RobotClient {
        peer_id: String,
        from_robot_tx: Sender<String>,
        job_id: String,
    },
}

pub async fn launch_new_job(robot_job: RobotJob, jobs: &Jobs) {
    info!("{:?}", robot_job);
    let jobs = Arc::clone(jobs);
    let mut job_manager = jobs.lock().unwrap();
    job_manager.new_job(
        robot_job.id.clone(),
        robot_job.job_type.clone(),
        robot_job.status.clone(),
    );
    job_manager.set_job_status(robot_job.id.clone(), "processing".to_string());

    match robot_job.job_type.as_str() {
        "docker-container-launch" => {
            info!("container launch");
            let shared_jobs = Arc::clone(&jobs);
            tokio::spawn(docker::execute_launch(robot_job, shared_jobs));
        }
        _ => {}
    }
}

pub async fn start_tunnel_messanger(
    tx: broadcast::Sender<ChannelMessageFromJob>,
    client: TunnnelClient,
) {
    let mut rx = tx.subscribe();
    loop {
        let data = rx.recv().await.unwrap();
        let client = &client;
        match client {
            TunnnelClient::RobotClient {
                peer_id,
                from_robot_tx,
                job_id,
            } => {
                match data {
                    ChannelMessageFromJob::TerminalMessage(stdout) => {
                        info!("sending stdout: {:?}", stdout);
                        let _ = from_robot_tx.send(
                            serde_json::to_string(&Message::new(
                                store::messages::MessageContent::TunnelResponseMessage {
                                    job_id: job_id.clone(),
                                    message: ChannelMessageFromJob::TerminalMessage(stdout),
                                },
                                "".to_string(),
                                Some(peer_id.clone()),
                            ))
                            .unwrap(),
                        );

                        // TODO send message
                        // let _res: Result<(), rust_socketio::Error> = socket
                        //     .emit(
                        //         "message_to_client",
                        //         json!({"client_id": client_id, "content": {"stdout": stdout}}),
                        //     )
                        //     .await;
                    }
                    _ => {}
                }
            }
        }
    }
}

pub async fn start_tunnel(tunnel_client: TunnnelClient, job_id: String, jobs: &Jobs) {
    info!("Start tunnel request");
    let jobs = Arc::clone(jobs);
    let mut job_manager = jobs.lock().unwrap();

    match job_manager.get_job_or_none(&job_id) {
        Some(_job) => match tunnel_client {
            TunnnelClient::RobotClient {
                peer_id,
                from_robot_tx,
                job_id,
            } => {
                job_manager.create_job_tunnel(&job_id, peer_id.clone());
                if let Some(channel_tx) = job_manager.get_channel_from_job(&job_id) {
                    tokio::spawn(start_tunnel_messanger(
                        channel_tx.clone(),
                        TunnnelClient::RobotClient {
                            peer_id,
                            from_robot_tx,
                            job_id,
                        },
                    ));
                } else {
                    info!("no channel tx");
                }
            }
        },
        None => {
            //todo: socket res
        }
    }
}

pub async fn message_to_robot(message: MessageToRobot, jobs: &Jobs) {
    info!("Message to robot request");

    info!("Message to robot: {:?}", message);
    let jobs = Arc::clone(jobs);
    let job_manager = jobs.lock().unwrap();

    if let Some(_job) = job_manager.get_job_or_none(&message.job_id) {
        if let Some(channel) = job_manager.get_channel_to_job(&message.job_id) {
            if let Ok(content) = serde_json::from_value::<MessageContent>(message.content) {
                match content {
                    MessageContent::Terminal { stdin } => {
                        channel
                            .send(store::messages::ChannelMessageToJob::TerminalMessage(stdin))
                            .unwrap();
                    }
                    MessageContent::Archive { dest_path, data } => {
                        channel
                            .send(store::messages::ChannelMessageToJob::ArchiveMessage {
                                encoded_tar: data,
                                path: dest_path,
                            })
                            .unwrap();
                    }
                }
            }
        }
    } else {
        error!("no such job: {}", message.job_id);
    }
}
