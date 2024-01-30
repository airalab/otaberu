use std::error::Error;
use tokio;

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

use agent::{Agent, AgentBuilder};

mod agent;
mod cli;
mod commands;
mod develop;

async fn main_normal(args: Args) -> Result<(), Box<dyn Error>> {
    let agent: Agent = AgentBuilder::default().api_key(args.api_key).build();
    info!("Starting agent: {:?}", agent);

    let socket = ClientBuilder::new(args.robot_server_url)
        .auth(json!({"api_key": agent.api_key}))
        .on("error", |err, _| {
            async move { eprintln!("Error: {:#?}", err) }.boxed()
        })
        .on("new_job", |payload: Payload, socket: Client| {
            async move { commands::launch_new_job(payload, socket).await }.boxed()
        })
        .connect()
        .await
        .expect("Connection failed");

    sleep(Duration::from_secs(1));
    loop {
        info!("me request");
        let _res = socket
            .emit_with_ack(
                "me",
                json!({}),
                Duration::from_secs(30),
                |message: Payload, _socket: Client| {
                    async move {
                        info!("got me {:?}", message);
                        match message {
                            Payload::String(str) => info!("{}", str),
                            Payload::Binary(bytes) => info!("Received bytes: {:#?}", bytes),
                        }
                    }
                    .boxed()
                },
            )
            .await;
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
