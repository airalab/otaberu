use super::messages::{process_command, MessageQueue, SocketCommand};
use crate::cli::Args;
use crate::store::Message;
use crate::store::Robots;
use crate::store::SignedMessage;
use std::error::Error;
use std::sync::{Arc, Mutex};
use tokio::net::{UnixListener, UnixStream};
use tokio::select;
use tokio::sync::broadcast;
use tokio::task::JoinHandle;
use tracing::{error, info};

#[derive(Default)]
pub struct SocketServer {
    listener: Option<UnixListener>,
    message_queue: Arc<Mutex<MessageQueue>>,
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
        let from_message_tx = from_message_tx.clone();
        let to_message_tx = to_message_tx.clone();

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

                                    let _ = handle_stream(stream, message_queue, from_message_tx.clone(), stream_robots).await?;
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
                                        let content_serialized = serde_json::to_string(&message.content)?;
                                        msg_to_socket = Some(content_serialized.to_string());
                                    }else if let Ok(signed_message) = serde_json::from_str::<SignedMessage>(&msg){
                                        info!("socket received libp2p signed message: {:?}", signed_message);
                                        msg_to_socket = Some(serde_json::to_string(&signed_message)?);
                                    }
                                    let message_queue_clone = Arc::clone(&self.message_queue);
                                    if let Some(msg_to_socket) = msg_to_socket{
                                        let mut message_queue = message_queue_clone.lock().unwrap();
                                        message_queue.add_message(msg_to_socket);
                                        if let Err(err) = message_queue.broadcast_messages(){
                                            error!("Error while broadcasting messages: {:?}", err);
                                        }
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
                error!("Listener not initialized");
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
) -> Result<(), Box<dyn Error + Send + Sync>> {
    stream.readable().await?;
    let mut data = vec![0; 65536];
    let (subscriber_tx, mut subscriber_rx) = broadcast::channel::<String>(16);

    if let Ok(n) = stream.try_read(&mut data) {
        info!("read {} bytes", n);
        let message = std::str::from_utf8(&data[..n])?;
        info!("message: {}", message);
        match serde_json::from_str::<SocketCommand>(message) {
            Ok(command) => {
                let answer = process_command(
                    command,
                    subscriber_tx,
                    robots,
                    &from_message_tx,
                    message_queue,
                )?;
                stream.writable().await?;
                if let Err(err) = stream.try_write(&answer.into_bytes()) {
                    error!("Can't write command answer to unix socket: {:?}", err);
                }
            }
            Err(err) => {
                error!("Can't deserialize robot message");
                error!("{:?}", err);
            }
        }
    }
    loop {
        select! {
            msg = subscriber_rx.recv()=>{
                if let Ok(msg) = msg{
                    info!("Got subscriber msg: {}", msg);
                    stream.writable().await?;
                    if let Err(err) = stream.try_write(&msg.into_bytes()){
                        error!("Error while sending message to subscriber socket: {:?}", err);
                    }
                }


            }
        }
    }
}

pub async fn start_unix_socket_thread(
    from_message_tx: &broadcast::Sender<String>,
    to_message_tx: &broadcast::Sender<String>,
    robots: &Robots,
    args: &Args,
) -> JoinHandle<()> {
    let mut socket_server = SocketServer::default();
    let from_message_tx = from_message_tx.clone();
    let to_message_tx = to_message_tx.clone();
    let robots = Arc::clone(&robots);

    let unix_socket_filename = args.socket_filename.clone().unwrap();
    let unix_socket_thread = tokio::spawn(async move {
        info!("Start unix socket server");
        match socket_server
            .start(from_message_tx, to_message_tx, robots, unix_socket_filename)
            .await
        {
            Ok(_) => {}
            Err(err) => {
                error!("UNIX SOCKET MODULE PANIC: {:?}", err);
            }
        }
    });

    return unix_socket_thread;
}
