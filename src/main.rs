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
use std::sync::{Arc, Mutex};
use tokio::select;
use tokio::sync::broadcast;

use agent::{Agent, AgentBuilder};

use develop::unix_socket::SocketServer;

mod agent;
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
    let agent: Agent = AgentBuilder::default()
        .api_key(args.api_key)
        .robot_server_url(args.robot_server_url.clone())
        .build();
    info!("Starting agent: {:?}", agent);
    let jobs: store::Jobs = Arc::new(Mutex::new(store::JobManager::default()));

    let mut socket = ClientBuilder::new(args.robot_server_url)
        .auth(json!({"api_key": agent.clone().api_key, "public_key": config.get_public_key_encoded(), "session_type": "ROBOT"}))
        .on("error", |err, _| {
            async move { eprintln!("Error: {:#?}", err) }.boxed()
        });

    {
        let shared_jobs: Arc<Mutex<store::JobManager>> = Arc::clone(&jobs);
        let shared_agent = agent.clone();
        socket = socket.on("new_job", move |payload: Payload, socket: Client| {
            let shared_jobs = Arc::clone(&shared_jobs);
            let agent = shared_agent.clone();
            async move {
                if let Payload::Text(str) = payload {
                    let robot_job: RobotJob =
                        serde_json::from_value(str.first().unwrap().clone()).unwrap(); //serde_json::from_str(&str).unwrap();
                    commands::launch_new_job(robot_job, Some(socket), agent, shared_jobs).await
                }
            }
            .boxed()
        })
    }

    {
        let shared_jobs: Arc<Mutex<store::JobManager>> = Arc::clone(&jobs);
        socket = socket.on("start_tunnel", move |payload: Payload, socket: Client| {
            info!("Start tunnel request");
            let shared_jobs = Arc::clone(&shared_jobs);
            async move {
                if let Payload::Text(str) = payload {
                    let start_tunnel_request: commands::StartTunnelReq =
                        serde_json::from_value(str.first().unwrap().clone()).unwrap();
                    commands::start_tunnel(
                        commands::TunnnelClient::SocketClient {
                            socket: socket,
                            client_id: start_tunnel_request.client_id,
                            job_id: start_tunnel_request.job_id.clone(),
                        },
                        start_tunnel_request.job_id,
                        shared_jobs,
                    )
                    .await
                }
            }
            .boxed()
        })
    }
    {
        let shared_jobs: Arc<Mutex<store::JobManager>> = Arc::clone(&jobs);
        socket = socket.on(
            "message_to_robot",
            move |payload: Payload, _socket: Client| {
                info!("Message to robot request");
                let shared_jobs = Arc::clone(&shared_jobs);
                async move {
                    match payload {
                        Payload::Text(payload) => {
                            let message: MessageToRobot =
                                serde_json::from_value(payload.first().unwrap().clone()).unwrap();
                            commands::message_to_robot(message, shared_jobs).await
                        }
                        _ => {}
                    }
                }
                .boxed()
            },
        )
    }

    {
        let shared_robots: store::Robots = Arc::clone(&robots);
        socket = socket.on("update_robots", move |payload: Payload, _socket: Client| {
            match payload {
                Payload::Text(value) => {
                    let robots_update: store::RobotsConfig =
                        serde_json::from_value(value.first().expect("no value got").clone())
                            .expect("can't parse value");
                    {
                        let mut robots_manager = shared_robots.lock().unwrap();
                        robots_manager.merge_update(robots_update);
                    }
                }
                _ => {}
            }

            async move {}.boxed()
        })
    }
    let _socket = socket.connect().await?;
    let mut to_message_rx = to_message_tx.subscribe();

    let _res = _socket.emit("me", json!({})).await;

    let sleep = tokio::time::sleep(Duration::from_secs(5));
    tokio::pin!(sleep);
    let shared_jobs: Arc<Mutex<store::JobManager>> = Arc::clone(&jobs);

    loop {
        select! {
            msg = to_message_rx.recv()=>match msg{
                Ok(msg)=>{
                    let message = serde_json::from_str::<Message>(&msg)?;
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
                            let agent = agent.clone();
                            commands::launch_new_job(robot_job, None, agent, shared_jobs).await;

                        },
                        _=>{}
                    }
                },
                Err(_)=>{
                    error!("error while socket receiving libp2p message");
                }
            },
            () = &mut sleep => {
                info!("me request");
                let _res = _socket
                    .emit("me", json!({}))
                    .await;
                sleep.as_mut().reset(tokio::time::Instant::now() + Duration::from_secs(60));
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
    let config_load_res = store::Config::load_from_file(args.key_filename.clone());
    match config_load_res {
        Ok(loaded_config) => config = loaded_config,
        Err(..) => {
            info!("Can't load private key from file");
        }
    }

    let robots: store::Robots = Arc::new(Mutex::new(store::RobotsManager {
        self_peer_id: config.get_peer_id(),
        ..Default::default()
    }));
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
        socket_server
            .start(
                from_message_tx_socket,
                to_message_tx_socket,
                unix_socket_robots,
                unix_socket_filename,
            )
            .await
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
    loop {
        tokio::time::sleep(Duration::from_secs(1));
    }

    Ok(())
}

#[cfg(test)]
mod tests {}
