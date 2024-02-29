use std::error::Error;
use tokio::{self, sync::oneshot::channel};

use futures_util::FutureExt;

use rust_socketio::{
    asynchronous::{Client, ClientBuilder},
    Payload,
};
use serde_json;
use serde_json::json;
use std::thread::sleep;
use std::time::Duration;

use cli::Args;

use tracing::info;
use tracing_subscriber::FmtSubscriber;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;

use agent::{Agent, AgentBuilder};

mod agent;
mod cli;
mod commands;
mod develop;
mod store;
mod utils;

async fn main_normal(args: Args) -> Result<(), Box<dyn Error>> {
    let agent: Agent = AgentBuilder::default()
        .api_key(args.api_key)
        .robot_server_url(args.robot_server_url.clone())
        .build();
    info!("Starting agent: {:?}", agent);
    let jobs: store::Jobs = Arc::new(Mutex::new(store::JobManager::default()));

    let mut socket = ClientBuilder::new(args.robot_server_url)
        .auth(json!({"api_key": agent.api_key.clone(), "session_type": "ROBOT"}))
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
                info!("Start tunnel request");
                let shared_jobs = Arc::clone(&shared_jobs);
                async move { commands::message_to_robot(payload, socket, shared_jobs).await }
                    .boxed()
            },
        )
    }
    let socket = socket.connect().await.expect("Connection failed");

    sleep(Duration::from_secs(1));
    loop {
        // info!("me request");
        // let _res = socket
        //     .emit_with_ack(
        //         "me",
        //         json!({}),
        //         Duration::from_secs(30),
        //         |message: Payload, _socket: Client| {
        //             async move {
        //                 info!("got me {:?}", message);
        //                 match message {
        //                     Payload::String(str) => info!("{}", str),
        //                     Payload::Binary(bytes) => info!("Received bytes: {:#?}", bytes),
        //                 }
        //             }
        //             .boxed()
        //         },
        //     )
        //     .await;
        sleep(Duration::from_secs(10));
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let _ = tracing::subscriber::set_global_default(FmtSubscriber::default());

    let args = cli::get_args();
    match args.mode.as_str() {
        "normal" => main_normal(args).await,
        "docker_tty" => develop::docker_tty::main_docker_tty(args).await,
        _ => {
            panic!("Not known mode: {}", args.mode)
        }
    }
}

#[cfg(test)]
mod tests {}
