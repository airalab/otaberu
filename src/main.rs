use std::error::Error;

use commands::{MessageToRobot, RobotJob};
use futures_util::FutureExt;

use libp2p::Multiaddr;
use rust_socketio::{
    asynchronous::{Client, ClientBuilder},
    Payload,
};
use serde_json::json;
use std::time::Duration;

use cli::Args;

use tracing::{error, info};
use tracing_subscriber::FmtSubscriber;

use crate::store::Message;
use crate::store::MessageContent;
use crate::store::MessageRequest;
use crate::store::MessageResponse;
use crate::store::SignedMessage;
use std::sync::{Arc, Mutex};
use tokio::select;
use tokio::sync::broadcast;

use develop::unix_socket::SocketServer;

mod cli;
mod commands;
mod develop;
mod store;
mod utils;

async fn main_normal(
    args: Args,
    config: store::Config,
    robots: store::Robots,
    to_message_tx: broadcast::Sender<String>,
    from_message_tx: broadcast::Sender<String>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let jobs: store::Jobs = Arc::new(Mutex::new(store::JobManager::default()));

    let mut to_message_rx = to_message_tx.subscribe();

    let shared_jobs: Arc<Mutex<store::JobManager>> = Arc::clone(&jobs);

    loop {
        select! {
            msg = to_message_rx.recv()=>match msg{
                Ok(msg)=>{
                    let signed_message = serde_json::from_str::<SignedMessage>(&msg)?;
                    let message = serde_json::from_str::<Message>(&signed_message.message)?;
                    match message.content{
                        MessageContent::JobMessage(message_content) =>{
                            info!("main got job message: {:?}", message_content);
                            if let Ok(message) = serde_json::from_value::<MessageToRobot>(message_content){
                                let shared_jobs = Arc::clone(&shared_jobs);
                                commands::message_to_robot(message, shared_jobs).await
                            }else{
                                error!("Can't deserialize MessageToRobot");
                            }
                        },
                        MessageContent::StartTunnelReq { job_id, peer_id }=>{
                            let shared_jobs = Arc::clone(&shared_jobs);

                            commands::start_tunnel(commands::TunnnelClient::RobotClient { peer_id: peer_id, from_robot_tx: from_message_tx.clone(), job_id: job_id.clone() }, job_id, shared_jobs).await
                        },
                        MessageContent::StartJob(robot_job)=>{
                            info!("new job {:?}", robot_job);
                            let shared_jobs = Arc::clone(&shared_jobs);
                            commands::launch_new_job(robot_job, None, shared_jobs).await;
                        },
                        MessageContent::UpdateConfig{config}=>{

                            info!("UpdateConfig: {:?}", config);
                            let shared_robots = Arc::clone(&robots);
                            let mut robot_manager = shared_robots.lock().unwrap();
                            let signed_message = signed_message.clone();
                            if signed_message.verify() && signed_message.public_key == robot_manager.owner_public_key{
                                robot_manager.set_robots_config(config, Some(signed_message));
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
                            let mut response_content:Option<MessageResponse> = None;
                            match request{
                                MessageRequest::ListJobs{}=>{
                                    info!("ListJobs request");
                                    let shared_jobs = Arc::clone(&shared_jobs);
                                    let job_manager = shared_jobs.lock().unwrap();
                                    let jobs = job_manager.get_jobs_info();
                                    info!("jobs: {:?}", jobs);
                                    response_content = Some(MessageResponse::ListJobs { jobs: jobs });
                                },
                                MessageRequest::GetRobotsConfig{}=>{
                                    info!("GetRobotsConfig request");
                                    let shared_robots = Arc::clone(&robots);
                                    let robot_manager = shared_robots.lock().unwrap();
                                    let robots_config = robot_manager.get_robots_config();
                                    info!("config: {:?}", robots_config);
                                    response_content = Some(MessageResponse::GetRobotsConfig { config:robots_config })
                                }
                                _=>{

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
                },
                Err(_)=>{
                    error!("error while socket receiving libp2p message");
                }
            },
        }
    }
}

pub fn generate_key_file(key_filename: String) -> store::Config {
    let config = store::Config::generate();
    let _ = config.save_to_file(key_filename);
    info!("Generated new key and saved it to merklebot.key");

    config
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let _ = tracing::subscriber::set_global_default(FmtSubscriber::default());
    //console_subscriber::init();

    let args = cli::get_args();
    match args.mode.as_str() {
        "normal" => {}
        _ => {
            panic!("Not known mode: {}", args.mode)
        }
    }

    let mut config: store::Config = store::Config::generate();
    match args.clone().secret_key {
        Some(secret_key_string) => {
            config = store::Config::load_from_sk_string(secret_key_string)?;
        }
        None => {
            let config_load_res = store::Config::load_from_file(args.key_filename.clone());
            match config_load_res {
                Ok(loaded_config) => {
                    config = loaded_config;
                }
                Err(err) => {
                    info!("Can't load private key from file {:?}", err);
                }
            }
        }
    }
    if let Err(err) = config.save_to_file(args.key_filename.clone()) {
        error!("Can't write key to file {:?}", err);
    }

    let mut robot_manager = store::RobotsManager {
        self_peer_id: config.get_peer_id(),
        ..Default::default()
    };
    robot_manager.read_config_from_file(args.config_path.clone());
    let robots: store::Robots = Arc::new(Mutex::new(robot_manager));

    {
        let mut robots_manager = robots.lock().unwrap();
        robots_manager.set_owner(args.clone().owner)?;
    }

    {
        match std::fs::read_to_string("config.json") {
            Ok(contents) => {
                let mut robots_manager = robots.lock().unwrap();
                robots_manager.read_robots_from_config(contents);
            }
            Err(_err) => {
                error!("Error while reading robots list");
            }
        }
    }

    let (to_message_tx, _to_message_rx) = broadcast::channel::<String>(16);
    let (from_message_tx, _from_message_rx) = broadcast::channel::<String>(16);

    let mut socket_server = SocketServer::default();

    let to_message_tx_socket = to_message_tx.clone();
    let from_message_tx_socket = from_message_tx.clone();
    let unix_socket_robots = Arc::clone(&robots);
    let unix_socket_filename = args.socket_filename.clone();
    let _unix_socket_thread = tokio::spawn(async move {
        info!("Start unix socket server");
        match socket_server
            .start(
                from_message_tx_socket,
                to_message_tx_socket,
                unix_socket_robots,
                unix_socket_filename,
            )
            .await
        {
            Ok(_) => {}
            Err(err) => {
                error!("UNIX SOCKET MODULE PANIC: {:?}", err);
            }
        }
    });

    let to_message_tx_libp2p = to_message_tx.clone();
    let from_message_tx_libp2p = from_message_tx.clone();

    let libp2p_robots = Arc::clone(&robots);
    let mut libp2p_config = config.clone();

    match args.bootstrap_addr.clone() {
        Some(addr) => {
            let addr: Multiaddr = addr.parse().unwrap();
            libp2p_config.add_bootstrap_addr(addr);
        }
        _ => {}
    }

    let libp2p_port = args.port_libp2p.parse::<u16>().unwrap();
    libp2p_config.set_libp2p_port(libp2p_port);

    let _libp2p_thread = tokio::spawn(async move {
        info!("Start libp2p node");
        develop::mdns::main_libp2p(
            libp2p_config,
            to_message_tx_libp2p,
            from_message_tx_libp2p,
            libp2p_robots,
        )
        .await
    });

    let main_args = args.clone();
    let mut main_config = config.clone();
    let main_robots = Arc::clone(&robots);

    //main_config.add_bootstrap_addr()

    let to_message_tx_libp2p = to_message_tx.clone();
    let from_message_tx_libp2p = from_message_tx.clone();

    let _main_thread = tokio::spawn(async move {
        main_normal(
            main_args,
            main_config,
            main_robots,
            to_message_tx_libp2p,
            from_message_tx_libp2p,
        )
        .await;
    });
    _main_thread.await;

    Ok(())
}

#[cfg(test)]
mod tests {}
