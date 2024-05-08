use crate::cli::Args;
use crate::store::Config;
use crate::store::Message;
use crate::store::Robots;

use futures::stream::StreamExt;
use libp2p::{
    gossipsub, mdns, noise,
    swarm::{NetworkBehaviour, SwarmEvent},
    tcp, yamux,
};
use std::collections::hash_map::DefaultHasher;
use std::error::Error;
use std::hash::{Hash, Hasher};
use std::time::Duration;
use tokio::sync::broadcast;
use tokio::{io, select};
use tracing::{error, info};

#[derive(NetworkBehaviour)]
struct MyBehaviour {
    gossipsub: gossipsub::Behaviour,
    mdns: mdns::tokio::Behaviour,
}

async fn start(
    identity: libp2p::identity::ed25519::Keypair,
    to_message_tx: broadcast::Sender<String>,
    from_message_tx: broadcast::Sender<String>,
    robots: Robots,
) -> Result<(), Box<dyn Error>> {
    let public_key: libp2p::identity::PublicKey = identity.public().into();
    info!("PeerId: {:?}", public_key.to_peer_id());

    let mut swarm = libp2p::SwarmBuilder::with_existing_identity(identity.clone().into())
        .with_tokio()
        .with_tcp(
            tcp::Config::default(),
            noise::Config::new,
            yamux::Config::default,
        )?
        .with_quic()
        .with_behaviour(|key| {
            // To content-address message, we can take the hash of message and use it as an ID.
            let message_id_fn = |message: &gossipsub::Message| {
                let mut s = DefaultHasher::new();
                message.data.hash(&mut s);
                gossipsub::MessageId::from(s.finish().to_string())
            };

            // Set a custom gossipsub configuration
            let gossipsub_config = gossipsub::ConfigBuilder::default()
                .heartbeat_interval(Duration::from_secs(10)) // This is set to aid debugging by not cluttering the log space
                .validation_mode(gossipsub::ValidationMode::Strict) // This sets the kind of message validation. The default is Strict (enforce message signing)
                .message_id_fn(message_id_fn) // content-address messages. No two messages of the same content will be propagated.
                .build()
                .map_err(|msg| io::Error::new(io::ErrorKind::Other, msg))?; // Temporary hack because `build` does not return a proper `std::error::Error`.

            // build a gossipsub network behaviour
            let gossipsub = gossipsub::Behaviour::new(
                gossipsub::MessageAuthenticity::Signed(key.clone()),
                gossipsub_config,
            )?;

            let mdns =
                mdns::tokio::Behaviour::new(mdns::Config::default(), key.public().to_peer_id())?;
            Ok(MyBehaviour { gossipsub, mdns })
        })?
        .with_swarm_config(|c| c.with_idle_connection_timeout(Duration::from_secs(60)))
        .build();

    // Create a Gossipsub topic
    let topic = gossipsub::IdentTopic::new("merklebot");
    // subscribes to our topic
    swarm.behaviour_mut().gossipsub.subscribe(&topic)?;

    swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse()?)?;

    let mut from_message_rx = from_message_tx.subscribe();

    // Kick it off
    loop {
        select! {

            msg = from_message_rx.recv()=>match msg{
                Ok(msg)=>{
                    let mut message = serde_json::from_str::<Message>(&&msg)?;
                    message.from = Some(std::str::from_utf8(&identity.public().to_bytes())?.to_string());

                    info!("libp2p received socket message: {:?}", message);

                    if let Err(e) = swarm
                        .behaviour_mut().gossipsub
                        .publish(topic.clone(), serde_json::to_string(&message)?.as_bytes()) {
                        println!("Publish error: {e:?}");
                    }

                }
                Err(_)=>{
                    error!("error while socket receiving libp2p message");
                }
            },
            event = swarm.select_next_some() => match event {
                SwarmEvent::Behaviour(MyBehaviourEvent::Mdns(mdns::Event::Discovered(list))) => {
                    for (peer_id, multiaddr) in list {
                        let ip4: String  = (&multiaddr.to_string().split("/").collect::<Vec<_>>()[2]).to_string();
                        info!("{:?}", ip4);
                        {
                            let mut robots_manager = robots.lock().unwrap();
                            info!("Adding interface");
                            robots_manager.add_interface_to_robot(peer_id.to_string(), ip4);
                        }
                        println!("mDNS discovered a new peer: {peer_id}, {multiaddr}");
                        swarm.behaviour_mut().gossipsub.add_explicit_peer(&peer_id);
                    }
                },
                SwarmEvent::Behaviour(MyBehaviourEvent::Mdns(mdns::Event::Expired(list))) => {
                    for (peer_id, _multiaddr) in list {
                        println!("mDNS discover peer has expired: {peer_id}");
                        swarm.behaviour_mut().gossipsub.remove_explicit_peer(&peer_id);
                    }
                },
                SwarmEvent::Behaviour(MyBehaviourEvent::Gossipsub(gossipsub::Event::Message {
                    propagation_source: peer_id,
                    message_id: id,
                    message,
                })) => {

                    let _ = to_message_tx.send(String::from_utf8_lossy(&message.data).to_string());
                    println!(
                        "Got message: '{}' with id: {id} from peer: {peer_id}",
                        String::from_utf8_lossy(&message.data),
                    )
                }
                SwarmEvent::NewListenAddr { address, .. } => {
                    println!("Local node is listening on {address}");
                }
                _ => {}
            }
        }
    }
}

pub async fn main_libp2p(
    config: Config,
    to_message_tx: broadcast::Sender<String>,
    from_message_tx: broadcast::Sender<String>,
    robots: Robots,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let _ = start(config.identity, to_message_tx, from_message_tx, robots).await;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracing_test::traced_test;

    #[traced_test]
    #[tokio::test]
    async fn launch_libp2p() {
        let s = start().await;
        assert!(s.is_ok());
    }
}
