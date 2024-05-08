use std::error::Error;

use futures_util::FutureExt;

use rust_socketio::{
    asynchronous::{Client, ClientBuilder},
    Payload,
};
use serde_json::json;
use std::thread::sleep;
use std::time::Duration;

use cli::Args;

use tracing::info;
use tracing_subscriber::FmtSubscriber;

use std::sync::{Arc, Mutex};
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
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let agent: Agent = AgentBuilder::default()
        .api_key(args.api_key)
        .robot_server_url(args.robot_server_url.clone())
        .build();
    info!("Starting agent: {:?}", agent);
    let jobs: store::Jobs = Arc::new(Mutex::new(store::JobManager::default()));

    let mut socket = ClientBuilder::new(args.robot_server_url)
        .auth(json!({"api_key": agent.api_key.clone(), "public_key": config.get_public_key_encoded(), "session_type": "ROBOT"}))
        .on("error", |err, _| {
            async move { eprintln!("Error: {:#?}", err) }.boxed()
        });

    {
        let shared_jobs: Arc<Mutex<store::JobManager>> = Arc::clone(&jobs);
        socket = socket.on("new_job", move |payload: Payload, socket: Client| {
            let shared_jobs = Arc::clone(&shared_jobs);
            let agent = agent.clone();
            async move { commands::launch_new_job(payload, socket, agent, shared_jobs).await }
                .boxed()
        })
    }

    {
        let shared_jobs: Arc<Mutex<store::JobManager>> = Arc::clone(&jobs);
        socket = socket.on("start_tunnel", move |payload: Payload, socket: Client| {
            info!("Start tunnel request");
            let shared_jobs = Arc::clone(&shared_jobs);
            async move { commands::start_tunnel(payload, socket, shared_jobs).await }.boxed()
        })
    }
    {
        let shared_jobs: Arc<Mutex<store::JobManager>> = Arc::clone(&jobs);
        socket = socket.on(
            "message_to_robot",
            move |payload: Payload, socket: Client| {
                info!("Message to robot request");
                let shared_jobs = Arc::clone(&shared_jobs);
                async move { commands::message_to_robot(payload, socket, shared_jobs).await }
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
    let _socket = socket.connect().await.expect("Connection failed");

    sleep(Duration::from_secs(10));
    loop {
        info!("me request");
        let _res = _socket
            .emit("me", json!({}))
            .await
            .expect("Server unreachable");
        sleep(Duration::from_secs(60));
    }
}

pub fn generate_key_file() -> store::Config {
    let config = store::Config::generate();
    let _ = config.save_to_file(String::from("merklebot.key"));
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
    let config_load_res = store::Config::load_from_file(String::from("merklebot.key"));
    match config_load_res {
        Ok(loaded_config) => config = loaded_config,
        Err(..) => {
            info!("Can't load private key from file");
        }
    }

    let robots: store::Robots = Arc::new(Mutex::new(store::RobotsManager::default()));
    {
        match std::fs::read_to_string("config.json") {
            Ok(contents) => {
                let mut robots_manager = robots.lock().unwrap();
                robots_manager.read_robots_from_config(contents);
            }
            Err(_) => {
                config = generate_key_file();
            }
        }
    }

    let mut socket_server = SocketServer::default();
    let (to_message_tx, _to_message_rx) = broadcast::channel::<String>(16);
    let (from_message_tx, _from_message_rx) = broadcast::channel::<String>(16);

    let to_message_tx_socket = to_message_tx.clone();
    let from_message_tx_socket = from_message_tx.clone();
    let unix_socket_robots = Arc::clone(&robots);
    let _unix_socket_thread = tokio::spawn(async move {
        info!("Start unix socket server");
        socket_server
            .start(
                from_message_tx_socket,
                to_message_tx_socket,
                unix_socket_robots,
            )
            .await
    });

    let libp2p_robots = Arc::clone(&robots);
    let libp2p_config = config.clone();
    let _libp2p_thread = tokio::spawn(async move {
        info!("Start libp2p node");
        develop::mdns::main_libp2p(libp2p_config, to_message_tx, from_message_tx, libp2p_robots)
            .await
    });

    let main_args = args.clone();
    let main_config = config.clone();
    let main_robots = Arc::clone(&robots);
    //let _main_thread = tokio::spawn(async move {
    //   info!("Start main logic");
    //    main_normal(main_args, main_config).await
    //}

    let _ = main_normal(main_args, main_config, main_robots).await;
    Ok(())

    //loop {
    //    tokio::time::sleep(Duration::from_secs(1));
    //}
}

#[cfg(test)]
mod tests {}
