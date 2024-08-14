use crate::store::Message;
use crate::store::MessageContent;
use crate::store::Robots;
use crate::store::SignedMessage;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::sync::{Arc, Mutex};
use tokio::net::{UnixListener, UnixStream};
use tokio::select;
use tokio::sync::broadcast;
use tracing::{error, info};

#[derive(Default)]
pub struct MessageQueue {
    messages: Vec<String>,
    subscribers: Vec<UnixStream>,
}
impl MessageQueue {
    pub fn add_subscriber(&mut self, stream: UnixStream) {
        self.subscribers.push(stream);
    }
    pub fn add_message(&mut self, message: String) {
        self.messages.push(message);
    }

    pub fn broadcast_messages(&mut self) -> Result<(), Box<dyn Error + Send + Sync>> {
        if let Some(last_message) = self.messages.last() {
            let message_bytes = last_message.as_bytes();

            for stream in &mut self.subscribers {
                if let Err(err) = stream.try_write(message_bytes) {
                    error!("Can't write message to socket {:?}", err);
                }
            }
        }
        Ok(())
    }
}

#[derive(Default)]
pub struct SocketServer {
    listener: Option<UnixListener>,
    message_queue: Arc<Mutex<MessageQueue>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Content {
    to: String,
    content: MessageContent,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct SocketCommand {
    action: String,
    content: Option<Content>,
    signed_message: Option<SignedMessage>,
}

impl SocketServer {
    async fn create_listener(&mut self, socket_filename: String) -> Result<(), Box<dyn Error>> {
        info!("creating listener");
        if std::fs::metadata(socket_filename.clone()).is_ok() {
            info!("A socket is already present. Deleting...");
            std::fs::remove_file(socket_filename.clone())?;
        }

        self.listener = Some(UnixListener::bind(socket_filename)?);
        info!("listener created");
        Ok(())
    }

    pub async fn start(
        &mut self,
        from_message_tx: broadcast::Sender<String>,
        to_message_tx: broadcast::Sender<String>,
        robots: Robots,
        socket_filename: String,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        info!("Starting server");
        if (self.create_listener(socket_filename).await).is_ok() {};

        match &self.listener {
            Some(listener) => {
                info!("Server started");
                let mut to_message_rx = to_message_tx.subscribe();
                loop {
                    select! {
                        listener_res = listener.accept()=>{
                            match listener_res{
                                Ok(listener_res)=>{
                                    let(stream, _addr) = listener_res;
                                    info!("new client");
                                    let stream_robots = Arc::clone(&robots);
                                    let message_queue= Arc::clone(&self.message_queue);

                                    let _ = handle_stream(stream, message_queue, from_message_tx.clone(), stream_robots).await;
                                },
                                Err(_)=>{
                                    error!("Error while accepting connection");
                                }
                            }
                        },
                        msg = to_message_rx.recv()=>{
                            match msg{
                                Ok(msg) => {
                                    let mut msg_to_socket: Option<String> = None;
                                    if let Ok(message) = serde_json::from_str::<Message>(&msg){
                                        info!("socket received libp2p message: {:?}", message.content);
                                        let content_serialized = message.content.serialize(serde_json::value::Serializer)?;
                                        msg_to_socket = Some(content_serialized.to_string());
                                    }else if let Ok(signed_message) = serde_json::from_str::<SignedMessage>(&msg){
                                        info!("socket received libp2p signed message: {:?}", signed_message);
                                        msg_to_socket = Some(serde_json::to_string(&signed_message)?);
                                    }
                                    let message_queue_clone = Arc::clone(&self.message_queue);
                                    if let Some(msg_to_socket) = msg_to_socket{
                                        let mut message_queue = message_queue_clone.lock().unwrap();
                                        message_queue.add_message(msg_to_socket);
                                        message_queue.broadcast_messages()?;
                                    }
                                }
                                Err(_) => {
                                    error!("error while socket receiving libp2p message");
                                }
                            }
                        }
                    }
                }
            }
            None => {
                error!("Listener not initialized")
            }
        }
        Ok(())
    }
}

async fn handle_stream(
    stream: UnixStream,
    message_queue: Arc<Mutex<MessageQueue>>,
    from_message_tx: broadcast::Sender<String>,
    robots: Robots,
) -> Result<(), Box<dyn Error>> {
    stream.readable().await?;
    let mut data = vec![0; 65536];
    if let Ok(n) = stream.try_read(&mut data) {
        info!("read {} bytes", n);
        let message = std::str::from_utf8(&data[..n])?;
        info!("message: {}", message);
        match serde_json::from_str::<SocketCommand>(message) {
            Ok(command) => {
                info!("command: {:?}", command);
                match command.action.as_str() {
                    "/me" => {
                        info!("/me request");
                    }
                    "/local_robots" => {
                        info!("/local_robots request");
                        stream.writable().await?;
                        let robots_manager = robots.lock().unwrap();
                        let robots_text = robots_manager.clone().get_robots_json();
                        info!("robots: {}", robots_text);
                        match stream.try_write(&robots_text.into_bytes()) {
                            Ok(_) => {}
                            Err(err) => {
                                error!("can't write /robots result to unix socket: {:?}", err)
                            }
                        }
                    }
                    "/send_message" => {
                        if let Some(message_content) = command.content {
                            let _ = from_message_tx.send(serde_json::to_string(&Message::new(
                                message_content.content,
                                "".to_string(),
                                Some(message_content.to),
                            ))?);
                            info!("Sent from unix socket to libp2p");
                            stream.writable().await?;
                            stream.try_write(b"{\"ok\":true}")?;
                        }
                    }
                    "/send_signed_message" => {
                        if command.action == "/send_signed_message" {
                            if let Some(signed_message) = command.signed_message {
                                let _ =
                                    from_message_tx.send(serde_json::to_string(&signed_message)?);
                                info!("Sent from unix socket to libp2p");
                                stream.writable().await?;
                                if let Err(_err) = stream.try_write(b"{\"ok\":true}") {
                                    error!(
                                        "Can't write /send_signed_message result to unix socket"
                                    );
                                }
                            }
                        }
                    }
                    "/subscribe_messages" => {
                        info!("/subscribe_messages request");
                        stream.writable().await?;
                        stream.try_write(b"{\"ok\":true}")?;
                        message_queue.lock().unwrap().add_subscriber(stream);
                    }
                    _ => {}
                }
            }
            Err(err) => {
                error!("Can't deserialize robot message");
                error!("{:?}", err);
            }
        }
    }
    Ok(())
}
