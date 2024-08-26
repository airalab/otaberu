use std::error::Error;

use core::start_core_thread;
use external_api::{start_unix_socket_thread, start_web_socket_thread};
use node::start_libp2p_thread;

use libp2p::Multiaddr;

use tracing::{error, info};
use tracing_subscriber::FmtSubscriber;

use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;

pub mod cli;
pub mod commands;
pub mod core;
pub mod external_api;
pub mod node;
pub mod store;
pub mod utils;

pub fn generate_key_file(key_filename: String) -> store::Config {
    let config = store::Config::generate();
    let _ = config.save_to_file(key_filename.clone());
    info!("Generated new key and saved it to {}", key_filename);

    config
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let _ = tracing::subscriber::set_global_default(FmtSubscriber::default());

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
    robot_manager.set_owner(args.clone().owner)?;
    let _ = robot_manager.read_config_from_file(args.config_path.clone());

    {
        match std::fs::read_to_string("config.json") {
            Ok(contents) => {
                robot_manager.read_robots_from_config(contents);
            }
            Err(_err) => {
                error!("Error while reading robots list");
            }
        }
    }

    match args.bootstrap_addr.clone() {
        Some(addr) => {
            let addr: Multiaddr = addr.parse().unwrap();
            config.add_bootstrap_addr(addr);
        }
        _ => {}
    }

    let robots: store::Robots = Arc::new(Mutex::new(robot_manager));

    let libp2p_port = args.port_libp2p.parse::<u16>().unwrap();
    config.set_libp2p_port(libp2p_port);

    let (to_message_tx, _to_message_rx) = broadcast::channel::<String>(16);
    let (from_message_tx, _from_message_rx) = broadcast::channel::<String>(16);

    if args.socket_filename.is_some() {
        // Starting unix socket server
        let _unix_socket_thread =
            start_unix_socket_thread(&from_message_tx, &to_message_tx, &robots, &args).await;
    }

    if args.rpc.is_some() {
        // Starting websocket server
        let _web_socket_thread =
            start_web_socket_thread(&from_message_tx, &to_message_tx, &robots, &args).await;
    }

    // Starting libp2p node
    let _libp2p_thread =
        start_libp2p_thread(&from_message_tx, &to_message_tx, &robots, &config, &args).await;

    // Starting core logic
    let core_thread = start_core_thread(&from_message_tx, &to_message_tx, &robots, &args).await;
    let _ = core_thread.await;

    Ok(())
}

#[cfg(test)]
mod tests {}
