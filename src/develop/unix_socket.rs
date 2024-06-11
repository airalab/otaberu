use crate::store::Message;
use crate::store::{Robots, RobotsManager};
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::sync::{Arc, Mutex};
use tokio::net::{UnixListener, UnixStream};
use tokio::select;
use tokio::sync::broadcast;
use tracing::{error, info};

#[derive(Default)]
pub struct SocketServer {
    listener: Option<UnixListener>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct SocketCommand {
    action: String,
    content: Option<String>,
}

impl SocketServer {
    async fn create_listener(&mut self, socket_filename:String) -> Result<(), Box<dyn Error>> {
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
        match self.create_listener(socket_filename).await {
            Ok(_) => {}
            Err(_) => {}
        }
        match &self.listener {
            Some(listener) => {
                info!("Server started");
                loop {
                    select! {
                        _ = async {
                        match listener.accept().await{
                            Ok(listener_res) => {
                                let(stream, _addr) = listener_res;
                                info!("new client");
                                let stream_robots = Arc::clone(&robots);
                                handle_stream(stream, to_message_tx.subscribe(), from_message_tx.clone(), stream_robots).await;
                            }
                            Err(_e) => {
                                // error
                            }
                        }

                        }=>{}
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
    mut to_message_rx: broadcast::Receiver<String>,
    from_message_tx: broadcast::Sender<String>,
    robots: Robots,
) -> Result<(), Box<dyn Error>> {
    stream.readable().await?;
    let mut data = vec![0; 1024];
    match stream.try_read(&mut data) {
        Ok(n) => {
            info!("read {} bytes", n);
            info!("{:?}", data);
            let message = std::str::from_utf8(&data[..n])?;
            info!("message: {}", message);
            let command = serde_json::from_str::<SocketCommand>(&message).unwrap();
            info!("command: {:?}", command);
            if command.action == "/me" {
                info!("/me request");
                //stream.writable().await?;
                //match stream.try_write(b"{\"name\":\"\"}") {
                //    Ok(_) => {}
                //    Err(_) => {}
                //}
            }
            if command.action == "/local_robots" {
                info!("/local_robots request");
                stream.writable().await?;
                let robots_manager = robots.lock().unwrap();
                let robots_text = robots_manager.clone().get_robots_json();
                info!("robots: {}", robots_text);
                match stream.try_write(&robots_text.into_bytes()) {
                    Ok(_) => {}
                    Err(_) => {
                        error!("can't write /robots result to unix socket")
                    }
                }
            }
            if command.action == "/send_message" {
                match command.content {
                    Some(message_content) => {
                        let _ = from_message_tx.send(serde_json::to_string(&Message::new(
                            message_content,
                            None,
                            None,
                        ))?);
                        stream.writable().await?;
                        stream.try_write(b"{\"ok\":true}")?;
                    }
                    None => {}
                }
            }

            if command.action == "/subscribe_messages" {
                info!("/subscribe_messages request");
                loop {
                    info!("waiting message...");
                    match to_message_rx.recv().await {
                        Ok(msg) => {
                            let message = serde_json::from_str::<Message>(&&msg)?;
                            info!("socket received libp2p message: {:?}", message);
                            stream.writable().await?;
                            stream.try_write(message.content.as_bytes())?;
                        }
                        Err(_) => {
                            error!("error while socket receiving libp2p message");
                        }
                    }
                }
            }
        }
        Err(_) => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::develop::unix_socket::SocketServer;
    use tracing_test::traced_test;

    #[ignore]
    #[traced_test]
    #[tokio::test]
    async fn create_listener() {
        let mut socket_server = SocketServer::default();
        let s = socket_server.create_listener().await;
        assert!(s.is_ok());
    }

    #[tokio::test]
    #[traced_test]
    async fn start_socket() {
        let mut socket_server = SocketServer::default();
        let thread = tokio::spawn(async move {
            info!("Start server test");
            socket_server.start().await
        });
        thread.await.unwrap();
        //let result = thread.join().unwrap();
    }
}
