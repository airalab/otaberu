use crate::store::Message;
use crate::store::MessageContent;
use crate::store::MessageRequest;
use crate::store::MessageResponse;
use crate::store::RobotRole;
use crate::store::SignedMessage;

use crate::commands;

use crate::cli::Args;
use crate::store::{JobManager, Jobs, Robots};
use std::sync::{Arc, Mutex};
use tokio::select;
use tokio::sync::broadcast;
use tokio::task::JoinHandle;
use tracing::{error, info};

use std::error::Error;

pub async fn main_normal(
    args: Args,
    robots: Robots,
    to_message_tx: broadcast::Sender<String>,
    from_message_tx: broadcast::Sender<String>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let jobs: Jobs = Arc::new(Mutex::new(JobManager::default()));

    let mut to_message_rx = to_message_tx.subscribe();

    loop {
        select! {
            msg = to_message_rx.recv()=>match msg{
                Ok(msg)=>{
                    let signed_message = serde_json::from_str::<SignedMessage>(&msg)?;
                    let message = serde_json::from_str::<Message>(&signed_message.message)?;

                    let mut should_process = false;
                    {
                        let robot_manager = robots.lock().unwrap();
                        if message.to.unwrap_or("".to_string()) == robot_manager.self_peer_id
                            || matches!(message.content, MessageContent::UpdateConfig { .. })
                        {
                            let role = robot_manager.get_role(signed_message.public_key);
                            info!("role: {:?}", role);
                            if matches!(role, RobotRole::Owner)
                                || matches!(role, RobotRole::OrganizationUser)
                            {
                                should_process = true
                            }
                        }
                    }
                    info!("should process {}", should_process);

                    if should_process{
                        match message.content{
                            MessageContent::JobMessage(message_content) =>{
                                info!("main got job message: {:?}", message_content);
                                if let Ok(message) = serde_json::from_value::<commands::MessageToRobot>(message_content){
                                    commands::message_to_robot(message, &jobs).await
                                }else{
                                    error!("Can't deserialize MessageToRobot");
                                }
                            },
                            MessageContent::StartTunnelReq { job_id, peer_id }=>{

                                commands::start_tunnel(commands::TunnnelClient::RobotClient { peer_id: peer_id, from_robot_tx: from_message_tx.clone(), job_id: job_id.clone() }, job_id, &jobs).await
                            },
                            MessageContent::StartJob(robot_job)=>{
                                info!("new job {:?}", robot_job);
                                commands::launch_new_job(robot_job, &jobs).await;
                            },
                            MessageContent::UpdateConfig{config}=>{

                                info!("UpdateConfig: {:?}", config);
                                let shared_robots = Arc::clone(&robots);
                                let mut robot_manager = shared_robots.lock().unwrap();
                                let signed_message = signed_message.clone();
                                if signed_message.verify() && signed_message.public_key == robot_manager.owner_public_key{
                                    robot_manager.set_robots_config(config, signed_message);
                                    info!("Config updated");
                                    match robot_manager.save_to_file(args.config_path.clone()){
                                        Ok(_)=>{
                                            info!("Config saved to file");
                                        },
                                        Err(_)=>{
                                            error!("Can't save config to file");
                                        }
                                    }
                                }

                            }
                            MessageContent::MessageRequest(request)=>{
                                let response_content:Option<MessageResponse>;
                                match request{
                                    MessageRequest::ListJobs{}=>{
                                        info!("ListJobs request");
                                        let job_manager = jobs.lock().unwrap();
                                        let jobs = job_manager.get_jobs_info();
                                        info!("jobs: {:?}", jobs);
                                        response_content = Some(MessageResponse::ListJobs { jobs: jobs });
                                    },
                                    MessageRequest::GetRobotsConfig{}=>{
                                        info!("GetRobotsConfig request");
                                        let robot_manager = robots.lock().unwrap();
                                        let robots_config = robot_manager.get_robots_config();
                                        info!("config: {:?}", robots_config);
                                        response_content = Some(MessageResponse::GetRobotsConfig { config:robots_config })
                                    }
                                }
                                if let Some(message_response) =response_content{
                                    let message_content = MessageContent::MessageResponse(message_response);
                                    let _ = from_message_tx.send(serde_json::to_string(&Message::new(
                                        message_content,
                                        "".to_string(),
                                        Some(message.from),
                                    ))?);
                                }
                            },
                            _=>{}
                        }
                    }
                },
                Err(_)=>{
                    error!("error while socket receiving libp2p message");
                }
            },
        }
    }
}

pub async fn start_core_thread(
    from_message_tx: &broadcast::Sender<String>,
    to_message_tx: &broadcast::Sender<String>,
    robots: &Robots,
    args: &Args,
) -> JoinHandle<()> {
    let robots = Arc::clone(&robots);
    let args = args.clone();
    let to_message_tx = to_message_tx.clone();
    let from_message_tx = from_message_tx.clone();

    let main_thread = tokio::spawn(async move {
        match main_normal(args, robots, to_message_tx, from_message_tx).await {
            Ok(_) => {}
            Err(err) => {
                error!("CORE MODULE PANIC: {:?}", err);
            }
        };
    });

    return main_thread;
}
