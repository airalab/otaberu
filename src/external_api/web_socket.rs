use crate::cli::Args;
use crate::store::Robots;
use futures::SinkExt;
use std::error::Error;
use std::sync::{Arc, Mutex};
use tokio::select;
use tokio::sync::broadcast;
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite;
use tracing::{error, info};

use tokio::net::{TcpListener, TcpStream};

use futures_util::StreamExt;

use super::messages::{process_command, MessageQueue, SocketCommand};

pub async fn start(
    from_message_tx: broadcast::Sender<String>,
    to_message_tx: broadcast::Sender<String>,
    robots: Robots,
    rpc_addr: String,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let listener = TcpListener::bind(&rpc_addr)
        .await
        .expect("can't bind web socket address");
    info!("Websocket listening on {}", rpc_addr);

    let message_queue = Arc::new(Mutex::new(MessageQueue::default()));
    let mut to_message_rx = to_message_tx.subscribe();

    loop {
        select! {
            listener_res = listener.accept()=>{
                if let Ok((stream, _)) = listener_res{

                    let robots = Arc::clone(&robots);
                    let message_queue= Arc::clone(&message_queue);
                    let from_message_tx =from_message_tx.clone();

                    tokio::spawn(async {
                        let _ = accept_connection(stream, message_queue, from_message_tx, robots).await;
                    });
                }
            },
            msg = to_message_rx.recv()=>{
                info!("Got msg: {:?}", msg);
                match msg{
                    Ok(msg) => {
                        let message_queue_clone = Arc::clone(&message_queue);
                            let mut message_queue = message_queue_clone.lock().unwrap();
                            message_queue.add_message(msg);
                            if let Err(err) = message_queue.broadcast_messages(){
                                error!("Error while broadcasting messages: {:?}", err);
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

pub async fn accept_connection(
    stream: TcpStream,
    message_queue: Arc<Mutex<MessageQueue>>,
    from_message_tx: broadcast::Sender<String>,
    robots: Robots,
) -> Result<(), Box<dyn Error>> {
    let addr = stream
        .peer_addr()
        .expect("connected streams should have a peer address");
    info!("Peer address: {}", addr);

    let ws_stream = tokio_tungstenite::accept_async(stream)
        .await
        .expect("Error during the websocket handshake occurred");

    info!("New WebSocket connection: {}", addr);

    let (mut ws_sender, mut ws_receiver) = ws_stream.split();
    let (subscriber_tx, mut subscriber_rx) = broadcast::channel::<String>(16);
    loop {
        select! {
            msg = ws_receiver.next() =>{
                match msg{
                    Some(msg)=>{
                        let msg = msg?;
                        info!("New message: {:?}", msg);
                        if msg.is_text(){
                            let message = msg.to_text()?;
                            if let Ok(command) = serde_json::from_str::<SocketCommand>(&message){
                                info!("Got command: {:?}", command);
                                let robots = Arc::clone(&robots);
                                let message_queue = Arc::clone(&message_queue);

                                if let Ok(answer) = process_command(command, subscriber_tx.clone(), robots, &from_message_tx, message_queue){
                                    match ws_sender.send(tungstenite::Message::Text(answer)).await{
                                        Ok(_)=>{
                                            info!("Answer sent");
                                        },
                                        Err(err)=>{
                                            error!("Error while sending answer {:?}", err);
                                        }
                                    }
                                }
                            }
                        }else{
                            break;
                        }

                    },
                    None => {break;}
                }
            },
            msg = subscriber_rx.recv()=>{
                info!("got msg for subscriber: {:?}", msg);
                if let Ok(msg) = msg{
                    match ws_sender.send(tungstenite::Message::Text(msg)).await{
                        Ok(_)=>{
                            info!("Msg sent to subscriber");
                        },
                        Err(err)=>{
                            error!("Couldn't send msg to subscriber: {:?}", err);
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

pub async fn start_web_socket_thread(
    from_message_tx: &broadcast::Sender<String>,
    to_message_tx: &broadcast::Sender<String>,
    robots: &Robots,
    args: &Args,
) -> JoinHandle<()> {
    let from_message_tx = from_message_tx.clone();
    let to_message_tx = to_message_tx.clone();
    let robots = Arc::clone(&robots);

    let rpc_addr = args.clone().rpc.unwrap_or("127.0.0.1:8888".to_string());
    let web_socket_thread = tokio::spawn(async move {
        info!("Start unix socket server");
        match start(from_message_tx, to_message_tx, robots, rpc_addr).await {
            Ok(_) => {}
            Err(err) => {
                error!("WEB SOCKET MODULE PANIC: {:?}", err);
            }
        }
    });

    return web_socket_thread;
}
