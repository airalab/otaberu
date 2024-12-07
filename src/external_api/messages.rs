use crate::store::messages::{Message, MessageContent, SignedMessage};
use crate::store::robot_manager::Robots;
use base64::{engine::general_purpose, Engine as _};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::error::Error;
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;
use tracing::{error, info};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Content {
    pub to: String,
    pub content: MessageContent,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SocketCommand {
    pub action: String,
    pub content: Option<Content>,
    pub signed_message: Option<SignedMessage>,
    pub action_param: Option<String>,
}

#[derive(Default)]
pub struct MessageQueue {
    messages: Vec<String>,
    subscribers: HashMap<String, broadcast::Sender<String>>, // PeerId -> tx to socket thread
}
impl MessageQueue {
    pub fn add_subscriber(&mut self, peer_id: String, tx: broadcast::Sender<String>) {
        self.subscribers.insert(peer_id, tx);
    }
    pub fn add_message(&mut self, message: String) {
        self.messages.push(message);
    }

    pub fn broadcast_messages(&mut self) -> Result<(), broadcast::error::SendError<String>> {
        if let Some(last_message_serialized) = self.messages.last() {
            let mut to: Option<String> = None;
            info!("last message {}", last_message_serialized);
            let last_message = match serde_json::from_str::<SignedMessage>(&last_message_serialized)
            {
                Ok(signed_message) => {
                    to = signed_message.to;
                    signed_message.message
                }
                _ => last_message_serialized.clone(),
            };

            info!("to: {:?}", to);
            match to {
                Some(to) => {
                    info!("subscribers: {:?}", self.subscribers);
                    if let Some(tx) = self.subscribers.get(&to) {
                        info!("Sending message to {}", to);
                        if let Err(err) = tx.send(last_message_serialized.clone()) {
                            error!("Error while sending message to ws tx: {:?}", err);
                        }
                    }
                }
                None => {
                    if let Ok(message) = serde_json::from_str::<Message>(&last_message) {
                        info!("socket received libp2p message: {:?}", message);
                        if let Ok(_content_serialized) = serde_json::to_string(&message.content) {
                            if let Some(peer_id) = message.to {
                                if let Some(tx) = self.subscribers.get(&peer_id) {
                                    if let Err(err) = tx.send(last_message.clone()) {
                                        error!("Error while sending message to ws tx: {:?}", err);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

pub fn process_command(
    command: SocketCommand,
    sender: broadcast::Sender<String>,
    robots: Robots,
    from_message_tx: &broadcast::Sender<String>,
    message_queue: Arc<Mutex<MessageQueue>>,
) -> Result<String, Box<dyn Error + Send + Sync>> {
    info!("command: {:?}", command);
    let mut answer: Option<String> = None;
    match command.action.as_str() {
        "/me" => {
            info!("/me request");
        }
        "/config" => {
            info!("/config request");
            let robots_manager = robots.lock().unwrap();
            if let Some(owner_b64) = command.action_param {
                let decoded_owner = general_purpose::STANDARD.decode(owner_b64)?;
                info!("decoded_owner {:?}", decoded_owner);
                if let Some((config, _)) = robots_manager
                    .config_storage
                    .get_config(decoded_owner.as_slice().try_into()?)
                {
                    let config_text = serde_json::to_string(&config)?;
                    info!("config: {}", config_text);
                    answer = Some(config_text);
                } else {
                    info!("{:?}", robots_manager.config_storage);
                    info!("no config found");
                    answer = Some("{\"ok\":false}".to_string());
                }
            } else {
                answer = Some("{\"ok\":false}".to_string());
            }
        }
        "/network_info" => {
            info!("/network_info request");
            let robots_manager = robots.lock().unwrap();
            let network_info_text =
                serde_json::to_string(&robots_manager.network_manager.peers_info)?;
            info!("{}", network_info_text);
            answer = Some(network_info_text)
        }
        "/send_message" => {
            if let Some(message_content) = command.content {
                let _ = from_message_tx.send(serde_json::to_string(&Message::new(
                    message_content.content,
                    "".to_string(),
                    Some(message_content.to),
                ))?);
                info!("Sent from unix socket to libp2p");
                answer = Some("{\"ok\":true}".to_string());
            }
        }
        "/send_signed_message" => {
            if command.action == "/send_signed_message" {
                if let Some(signed_message) = command.signed_message {
                    let _ = from_message_tx.send(serde_json::to_string(&signed_message)?);
                    info!("Sent from unix socket to libp2p");
                    answer = Some("{\"ok\":true}".to_string());
                }
            }
        }
        "/subscribe_messages" => {
            info!("/subscribe_messages request");
            if let Some(client_peer_id) = command.action_param {
                message_queue
                    .lock()
                    .unwrap()
                    .add_subscriber(client_peer_id, sender);
                answer = Some("{\"ok\":true}".to_string());
            } else {
                answer = Some("{\"ok\":false}".to_string());
            }
        }
        _ => {}
    }

    return Ok(answer.ok_or("no answer")?);
}
