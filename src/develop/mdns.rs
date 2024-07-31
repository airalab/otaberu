use crate::store::Config;
use crate::store::Message;
use crate::store::RobotRole;
use crate::store::Robots;

use futures::stream::StreamExt;
use libp2p::{
    gossipsub, identify, kad,
    kad::store::MemoryStore,
    mdns, noise,
    swarm::{NetworkBehaviour, SwarmEvent},
    tcp, yamux,
};

use libp2p::Multiaddr;
use std::collections::hash_map::DefaultHasher;
use std::error::Error;
use std::hash::{Hash, Hasher};
use std::time::Duration;
use tokio::sync::broadcast;
use tokio::{io, select};
use tracing::{error, info};

#[derive(NetworkBehaviour)]
struct MyBehaviour {
    identify: identify::Behaviour,
    gossipsub: gossipsub::Behaviour,
    mdns: mdns::tokio::Behaviour,
    kademlia: kad::Behaviour<MemoryStore>,
}

async fn start(
    identity: libp2p::identity::ed25519::Keypair,
    libp2p_port: u16,
    bootstrap_addrs: Vec<Multiaddr>,
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

            let store = MemoryStore::new(key.public().to_peer_id());

            let kademlia = kad::Behaviour::new(key.public().to_peer_id(), store);

            let identify_config =
                identify::Config::new("/agent/connection/1.0.0".to_string(), key.clone().public());
            let identify = identify::Behaviour::new(identify_config);

            Ok(MyBehaviour {
                gossipsub,
                mdns,
                kademlia,
                identify,
            })
        })?
        .with_swarm_config(|c| c.with_idle_connection_timeout(Duration::from_secs(60)))
        .build();

    // Create a Gossipsub topic
    let topic = gossipsub::IdentTopic::new("merklebot");
    // subscribes to our topic
    swarm.behaviour_mut().gossipsub.subscribe(&topic)?;

    swarm.listen_on(format!("/ip4/0.0.0.0/tcp/{}", libp2p_port).parse()?)?;

    // Botstrap Kadmelia
    if bootstrap_addrs.len() > 0 {
        let bootaddr = bootstrap_addrs.first().unwrap();
        swarm.dial(bootaddr.clone())?;
    }

    let mut from_message_rx = from_message_tx.subscribe();
    // Kick it off
    loop {
        select! {
            msg = from_message_rx.recv()=>match msg{
                Ok(msg)=>{
                    let mut message = serde_json::from_str::<Message>(&msg)?;
                    {
                        let robots_manager = robots.lock().unwrap();
                        message.from = Some(robots_manager.self_peer_id.clone());
                    }
                    // message.from = Some(std::str::from_utf8(&identity.public().to_bytes())?.to_string());

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
                SwarmEvent::Behaviour(behaviour)=> {
                    match behaviour {
                        MyBehaviourEvent::Mdns(mdns::Event::Discovered(list)) => {
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
                        MyBehaviourEvent::Mdns(mdns::Event::Expired(list)) => {
                            for (peer_id, _multiaddr) in list {
                                println!("mDNS discover peer has expired: {peer_id}");
                                swarm.behaviour_mut().gossipsub.remove_explicit_peer(&peer_id);
                            }
                        },
                        MyBehaviourEvent::Gossipsub(gossipsub::Event::Message {
                            propagation_source: peer_id,
                            message_id: id,
                            message,
                        }) => {
                            println!(
                                "Got message: '{}' with id: {id} from peer: {peer_id}",
                                String::from_utf8_lossy(&message.data),
                            );
                            let message_string = String::from_utf8_lossy(&message.data).to_string();
                            let message_data = serde_json::from_str::<Message>(&message_string)?;

                            {
                                let robots_manager = robots.lock().unwrap();
                                if message_data.to.unwrap_or("".to_string())==robots_manager.self_peer_id{
                                    match message_data.from{
                                        Some(sender_peer_id)=>{
                                            {
                                                let role = robots_manager.get_role(sender_peer_id);
                                                info!("role: {:?}", role);
                                                if matches!(role, RobotRole::OrganizationRobot){
                                                    let _ = to_message_tx.send(message_string);
                                                }
                                            }

                                        },
                                        None=>{

                                        }
                                    }
                                }
                            }
                        },
                        MyBehaviourEvent::Kademlia(event)=>{
                           match event{
                                kad::Event::ModeChanged { new_mode } => info!("KadEvent:ModeChanged: {new_mode}"),
                                kad::Event::RoutablePeer { peer, address } => info!("KadEvent:RoutablePeer: {peer} | {address}"),
                                kad::Event::PendingRoutablePeer { peer, address } => info!("KadEvent:PendingRoutablePeer: {peer} | {address}"),
                                kad::Event::InboundRequest { request } => info!("KadEvent:InboundRequest: {request:?}"),
                                kad::Event::RoutingUpdated {
                                    peer,
                                    is_new_peer,
                                    addresses,
                                    bucket_range,
                                    old_peer } => {
                                        info!("KadEvent:RoutingUpdated: {peer} | IsNewPeer? {is_new_peer} | {addresses:?} | {bucket_range:?} | OldPeer: {old_peer:?}");
                                    },
                                kad::Event::OutboundQueryProgressed {
                                    id,
                                    result,
                                    stats,
                                    step } => {

                                    info!("KadEvent:OutboundQueryProgressed: ID: {id:?} | Result: {result:?} | Stats: {stats:?} | Step: {step:?}")
                                },
                                _=>{}
                           }
                        },
                        _=>{}
                    }
                },

                SwarmEvent::NewListenAddr { address, .. } => {
                    println!("Local node is listening on {address}");
                },
                SwarmEvent::ConnectionEstablished{
                    peer_id,
                    connection_id,
                    endpoint,
                    num_established,
                    concurrent_dial_errors,
                    established_in
                }=>{
                    info!("ConnectionEstablished: {peer_id} | {connection_id} | {endpoint:?} | {num_established} | {concurrent_dial_errors:?} | {established_in:?}");
                },
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
    let _ = start(
        config.identity,
        config.libp2p_port,
        config.bootstrap_addrs,
        to_message_tx,
        from_message_tx,
        robots,
    )
    .await;

    Ok(())
}
