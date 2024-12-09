use crate::cli::Args;

use crate::store::key_manager::KeyConfig;
use crate::store::messages::{
    Message, MessageContent, MessageRequest, RobotRequest, RobotResponse, SignedMessage,
    VerifiableMessage,
};
use crate::store::robot_manager::Robots;

use futures::stream::StreamExt;
use libp2p::PeerId;
use libp2p::StreamProtocol;
use libp2p::{
    gossipsub, identify, kad,
    kad::store::MemoryStore,
    mdns, noise, ping, request_response,
    swarm::{NetworkBehaviour, SwarmEvent},
    tcp, yamux,
};

use libp2p::Multiaddr;
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::error::Error;
use std::hash::{Hash, Hasher};
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use tokio::task::JoinHandle;
use tokio::{io, select};
use tracing::{debug, error, info};

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "msg_type")]
pub enum RoutedMessage {}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "msg_type")]
pub enum RequestMessage {
    Echo { text: String },
    SignedMessage(SignedMessage),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "msg_type")]
pub enum ResponseMessage {
    Echo { text: String },
}

#[derive(NetworkBehaviour)]
struct MyBehaviour {
    identify: identify::Behaviour,
    request_response: request_response::json::Behaviour<RobotRequest, RobotResponse>,
    messaging: request_response::cbor::Behaviour<RequestMessage, ResponseMessage>,
    gossipsub: gossipsub::Behaviour,
    mdns: mdns::tokio::Behaviour,
    kademlia: kad::Behaviour<MemoryStore>,
    ping: ping::Behaviour,
}

fn process_signed_message(
    message_string: String,
    robots: Robots,
    to_message_tx: broadcast::Sender<String>,
    args: &Args,
) {
    if let Ok(signed_message) = serde_json::from_str::<SignedMessage>(&message_string) {
        debug!("signed_message: {:?}", signed_message);
        let mut robot_manager = robots.lock().unwrap();
        if signed_message.verify() {
            debug!("Verified");
            match signed_message.encryption {
                Some(enc_info) => {
                    let _ = to_message_tx.send(message_string);
                }
                None => {
                    if let Ok(message) = serde_json::from_str::<Message>(&signed_message.message) {
                        debug!("message: {:?}", message);
                        let _ = to_message_tx.send(message_string);

                        if args.rpc.is_some() {
                            if let MessageContent::UpdateConfig { config } = message.content {
                                debug!("UPDATE CONFIG");
                                debug!("{:?}", signed_message.public_key);
                                robot_manager.config_storage.update_config(
                                    signed_message.public_key,
                                    config,
                                    signed_message,
                                );
                            }
                        }
                    }
                }
            }
        } else {
            debug!("not verified");
        }
    }
}

async fn start(
    identity: libp2p::identity::ed25519::Keypair,
    libp2p_port: u16,
    bootstrap_addrs: Vec<Multiaddr>,
    to_message_tx: broadcast::Sender<String>,
    from_message_tx: broadcast::Sender<String>,
    robots: Robots,
    args: Args,
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

            let request_response =
                request_response::json::Behaviour::<RobotRequest, RobotResponse>::new(
                    [(
                        StreamProtocol::new("/rn/1"),
                        request_response::ProtocolSupport::Full,
                    )],
                    request_response::Config::default(),
                );

            let messaging =
                request_response::cbor::Behaviour::<RequestMessage, ResponseMessage>::new(
                    [(
                        StreamProtocol::new("/rn/messaging/1"),
                        request_response::ProtocolSupport::Full,
                    )],
                    request_response::Config::default(),
                );

            let ping = ping::Behaviour::default();

            Ok(MyBehaviour {
                gossipsub,
                mdns,
                kademlia,
                identify,
                ping,
                request_response,
                messaging,
            })
        })?
        .with_swarm_config(|c| c.with_idle_connection_timeout(Duration::from_secs(10)))
        .build();

    // Create a Gossipsub topic
    let topic = gossipsub::IdentTopic::new("rn");
    // subscribes to our topic
    swarm.behaviour_mut().gossipsub.subscribe(&topic)?;

    swarm.listen_on(format!("/ip4/0.0.0.0/tcp/{}", libp2p_port).parse()?)?;

    // Botstrap Kadmelia
    if bootstrap_addrs.len() > 0 {
        let bootaddr = bootstrap_addrs.first().unwrap();
        swarm.dial(bootaddr.clone())?;
        swarm.behaviour_mut().kademlia.add_address(
            &PeerId::from_str("12D3KooWB19yrtJ8ed9YGaDFhbExo27UdVGkqNzfG1dJfTqEXVFX").unwrap(),
            bootaddr.clone(),
        );
    }

    let mut from_message_rx = from_message_tx.subscribe();
    // Kick it off
    loop {
        select! {
                    msg = from_message_rx.recv()=>match msg{
                        Ok(msg)=>{
                            if let Ok(mut message) = serde_json::from_str::<Message>(&msg){
                                {
                                    let robots_manager = robots.lock().unwrap();
                                    message.from = robots_manager.self_peer_id.clone();
                                }
                                // message.from = Some(std::str::from_utf8(&identity.public().to_bytes())?.to_string());

                                debug!("libp2p received socket message: {:?}", message);
                                // if let Some(to) = &message.to{
                                //     let robots_manager = robots.lock().unwrap();
                                //     if let Some(to_pk) = robots_manager.network_manager.get_peer_id_public_key(&to){
                                //         if let Ok(encrypted_message) = message.encrypted(identity.clone(), &to_pk){
                                //             info!("Sending enc message {:?}", encrypted_message);
                                //             if let Ok(msg_str) = serde_json::to_string(&encrypted_message){
                                //                 if let Err(e) = swarm
                                //                     .behaviour_mut().gossipsub
                                //                     .publish(topic.clone(), msg_str.as_bytes()) {
                                //                     println!("Publish error: {e:?}");
                                //                 }
                                //             } else{
                                //                 info!("Can't serialize message");
                                //             }
                                //         }else{
                                //             info!("Can't encrypt message");
                                //         }
                                //     }else{
                                //         info!("Unknown public key for {}", to);
                                //     }
                                // } else{
                                    if let Ok(signed_message) = message.signed(identity.clone()){
                                        if let Err(e) = swarm
                                            .behaviour_mut().gossipsub
                                            .publish(topic.clone(), serde_json::to_string(&signed_message)?.as_bytes()) {
                                            println!("Publish error: {e:?}");
                                        }
                                    }
                                // }
                            } else if let Ok(signed_message) = serde_json::from_str::<SignedMessage>(&msg){
                                debug!("libp2p received socket signed_message: {:?}", signed_message);
                                process_signed_message(msg.clone(), Arc::clone(&robots), to_message_tx.clone(), &args);
                                if let Err(e) = swarm
                                    .behaviour_mut().gossipsub
                                    .publish(topic.clone(), serde_json::to_string(&signed_message)?.as_bytes()) {
                                    println!("Publish error: {e:?}");
                                }

                            }
                        }
                        Err(_)=>{
                            error!("error while socket receiving libp2p message");
                        }
                    },
                    event = swarm.select_next_some() => match event {
                        SwarmEvent::Behaviour(behaviour)=> {
                            {
                                let mut robots_manager = robots.lock().unwrap();
                                robots_manager.set_peers(swarm.connected_peers().map(|peer_id| peer_id.clone()).collect::<Vec<PeerId>>())
                            }
                            // info!("Event: {:?}", behaviour);
                            match behaviour {
                                MyBehaviourEvent::Mdns(mdns::Event::Discovered(list)) => {
                                    for (peer_id, multiaddr) in list {
                                        let ip4: String  = (&multiaddr.to_string().split("/").collect::<Vec<_>>()[2]).to_string();
                                        info!("{:?}", ip4);
                                        // {
                                        //     let mut robots_manager = robots.lock().unwrap();
                                        //     info!("Adding interface");
                                        //     robots_manager.add_interface_to_robot(peer_id.to_string(), ip4);
                                        // }
                                        debug!("mDNS discovered a new peer: {peer_id}, {multiaddr}");
                                        swarm.behaviour_mut().gossipsub.add_explicit_peer(&peer_id);
                                    }
                                },
                                MyBehaviourEvent::Mdns(mdns::Event::Expired(list)) => {
                                    for (peer_id, _multiaddr) in list {
                                        debug!("mDNS discover peer has expired: {peer_id}");
                                        swarm.behaviour_mut().gossipsub.remove_explicit_peer(&peer_id);
                                    }
                                },
                                MyBehaviourEvent::Gossipsub(gossipsub::Event::Message {
                                    propagation_source: peer_id,
                                    message_id: id,
                                    message,
                                }) => {
                                    info!(
                                        "Got message: '{}' with id: {id} from peer: {peer_id}",
                                        String::from_utf8_lossy(&message.data),
                                    );
                                    let message_string = String::from_utf8_lossy(&message.data).to_string();
                                    process_signed_message(message_string, Arc::clone(&robots), to_message_tx.clone(), &args);

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
                                MyBehaviourEvent::Messaging(event)=>
        {
                                    debug!("req resp {:?}", event);
                                    match event{
                                        request_response::Event::Message{
                                            peer,
                                            message:
                                            request_response::Message::Request{
                                                request,
                                                channel, ..
                                            },
                                        }=>{

                                            debug!("request: {:?}", request );

                                                match request{
                                                    RequestMessage::Echo{text}=>{
                                                        info!("messaging got text {} from {}", text, peer);
                                                    },
                                                    _=>{

                                                    }
                                                }
                                        },
                                        request_response::Event::Message{
                                            peer,
                                            message:
                                            request_response::Message::Response {
                                                response,
                                                ..
                                            },
                                        }=>{
                                            match response {
                                                _=>{}

                                            }
                                        },

                                        _=>{}
                                    }
                                },
                                MyBehaviourEvent::RequestResponse(event)=>{
                                    debug!("req resp {:?}", event);
                                    match event{
                                        request_response::Event::Message{
                                            peer,
                                            message:
                                            request_response::Message::Request{
                                                request,
                                                channel, ..
                                            },
                                        }=>{

                                            debug!("request: {:?}", request );

                                                match request{
                                                    RobotRequest::GetConfigVersion{version, owner}=>{
                                                        let robot_manager = robots.lock().unwrap();
                                                        debug!("get config verison request from peer {}", peer);

                                                        let mut stored_version = 0;

                                                        if let Some((config, signed_message)) = robot_manager.config_storage.get_config(&owner){
                                                            if version<config.version{
                                                                debug!("sharing config");
                                                                swarm.behaviour_mut().request_response.send_request(&peer, RobotRequest::ShareConfigMessage { signed_message} );
                                                            }
                                                            stored_version = config.version;
                                                        }

                                                        let _ =swarm.behaviour_mut().request_response.send_response(channel, RobotResponse::GetConfigVersion { version:stored_version, owner });

                                                    },
                                                    RobotRequest::ShareConfigMessage{signed_message}=>{
                                                        debug!("got config from peer {}", peer);
                                                        let robots = Arc::clone(&robots);
                                                        process_signed_message(serde_json::to_string(&signed_message)?, robots, to_message_tx.clone(), &args);

                                                    }
                                                }
                                        },
                                        request_response::Event::Message{
                                            peer,
                                            message:
                                            request_response::Message::Response {
                                                response,
                                                ..
                                            },
                                        }=>{
                                            match response {
                                                RobotResponse::GetConfigVersion{version, owner}=>{
                                                    info!("got config version {} from peer {}", version, peer);
                                                    let robot_manager = robots.lock().unwrap();

                                                    if let Some((config, signed_message)) = robot_manager.config_storage.get_config(&owner){
                                                        info!("config version {}", config.version);
                                                        if version<config.version{
                                                            info!("sharing config");
                                                            swarm.behaviour_mut().request_response.send_request(&peer, RobotRequest::ShareConfigMessage { signed_message} );
                                                        }
                                                    }else{
                                                        error!("No config");
                                                    }
                                                },
                                                _=>{}

                                            }
                                        },

                                        _=>{}
                                    }
                                }
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
                            let robot_manager = robots.lock().unwrap();
                            let version = robot_manager.config_version.clone();
                            let owner = robot_manager.owner_public_key.clone();
                            let get_config_version_req = RobotRequest::GetConfigVersion{version, owner};
                            info!("Sending config version req {:?}", get_config_version_req);
                            swarm.behaviour_mut().request_response.send_request(&peer_id, get_config_version_req);
                            swarm.behaviour_mut().messaging.send_request(&peer_id, RequestMessage::Echo { text:  "hello".to_owned()});
                        },
                        _ => {}
                    }
                }
    }
}

pub async fn start_libp2p_thread(
    from_message_tx: &broadcast::Sender<String>,
    to_message_tx: &broadcast::Sender<String>,
    robots: &Robots,
    config: &KeyConfig,
    args: &Args,
) -> JoinHandle<()> {
    let from_message_tx = from_message_tx.clone();
    let to_message_tx = to_message_tx.clone();
    let robots = Arc::clone(robots);
    let config = config.clone();
    let args = args.clone();

    let libp2p_thread = tokio::spawn(async move {
        info!("Start libp2p node");
        match start(
            config.identity,
            config.libp2p_port,
            config.bootstrap_addrs,
            to_message_tx,
            from_message_tx,
            robots,
            args,
        )
        .await
        {
            Ok(_) => {}
            Err(err) => {
                error!("LIBP2P MODULE PANIC: {:?}", err);
            }
        }
    });

    return libp2p_thread;
}
