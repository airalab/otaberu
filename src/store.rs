use libp2p::{Multiaddr, PeerId};
use libp2p_identity::ed25519;
use serde::{Deserialize, Serialize};
use std::any::Any;
use std::collections::HashMap;
use std::error::Error;
use std::fs::{self};
use std::hash::Hash;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;
use tokio::sync::broadcast;
use tracing::info;

use base64::{engine::general_purpose, Engine as _};

use crate::commands::{RobotJob, RobotJobResult};

/// Represents a tunnel for job communication
#[derive(Debug, Clone, Serialize)]
pub struct Tunnel {
    pub client_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChannelMessageToJob {
    TerminalMessage(String),
    ArchiveMessage { encoded_tar: String, path: String },
    ArchiveRequest { path: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChannelMessageFromJob {
    TerminalMessage(String),
    ArchiveMessage { encoded_tar: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "request_type")]
pub enum RobotRequest {
    GetConfigVersion { version: u32, owner: [u8; 32] },
    ShareConfigMessage { signed_message: SignedMessage },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "response_type")]
pub enum RobotResponse {
    GetConfigVersion { version: u32, owner: [u8; 32] },
    ShareConfigMessage {},
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "request_type")]
pub enum MessageRequest {
    ListJobs {},
    GetRobotsConfig {},
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "response_type")]
pub enum MessageResponse {
    ListJobs { jobs: Vec<JobProcessData> },
    GetRobotsConfig { config: RobotsConfig },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum MessageContent {
    CustomMessage(serde_json::Value),
    MessageResponse(MessageResponse),
    MessageRequest(MessageRequest),
    JobMessage(serde_json::Value),
    StartTunnelReq {
        job_id: String,
        peer_id: String,
    },
    TunnelResponseMessage {
        job_id: String,
        message: ChannelMessageFromJob,
    },
    StartJob(RobotJob),
    UpdateConfig {
        config: RobotsConfig,
    },
}

#[derive(Debug, Clone)]
pub struct JobProcess {
    pub job_id: String,
    pub job_type: String,
    pub status: String,
    pub channel_tx: Option<broadcast::Sender<ChannelMessageFromJob>>,
    pub channel_to_job_tx: broadcast::Sender<ChannelMessageToJob>,
    pub tunnel: Option<Tunnel>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobProcessData {
    pub job_id: String,
    pub job_type: String,
    pub status: String,
}

impl From<JobProcess> for JobProcessData {
    fn from(job_process: JobProcess) -> Self {
        return Self {
            job_id: job_process.job_id,
            job_type: job_process.job_type,
            status: job_process.status,
        };
    }
}

/// Manages job processes
#[derive(Default, Debug)]
pub struct JobManager {
    pub data: HashMap<String, JobProcess>,
}

impl JobManager {
    /// Creates a new job
    pub fn new_job(&mut self, job_id: String, job_type: String, status: String) {
        let (channel_to_job_tx, _channel_to_job_rx) =
            broadcast::channel::<ChannelMessageToJob>(100);

        self.data.insert(
            job_id.clone(),
            JobProcess {
                job_id,
                job_type,
                status,
                channel_tx: None,
                channel_to_job_tx: channel_to_job_tx.clone(),
                tunnel: None,
            },
        );
    }
    /// Sets the result of a job
    pub fn set_job_result(&mut self, reslut: RobotJobResult) {
        let process = self.data.get_mut(&reslut.job_id);
        match process {
            Some(_process) => {
                self.set_job_status(reslut.job_id, reslut.status);
            }
            None => {}
        }
    }
    pub fn get_job_or_none(&self, job_id: &String) -> Option<JobProcess> {
        match self.data.get(job_id) {
            Some(job) => Some(job.clone()),
            None => None,
        }
    }
    /// Retrieves job information
    pub fn get_jobs_info(&self) -> Vec<JobProcessData> {
        return self.data.clone().into_values().map(|x| x.into()).collect();
    }
    pub fn set_job_status(&mut self, job_id: String, status: String) {
        let process = self.data.get_mut(&job_id);
        match process {
            Some(process) => {
                process.status = status;
            }
            None => {}
        }
    }

    /// Creates a tunnel for a job
    pub fn create_job_tunnel(&mut self, job_id: &String, client_id: String) {
        let process = self.data.get_mut(job_id);
        let (tx, _rx) = broadcast::channel::<ChannelMessageFromJob>(100);
        match process {
            Some(process) => {
                process.tunnel = Some(Tunnel { client_id });
                process.channel_tx = Some(tx.clone());
            }
            None => {}
        }
    }
    pub fn get_channel_from_job(
        &self,
        job_id: &String,
    ) -> Option<broadcast::Sender<ChannelMessageFromJob>> {
        match self.data.get(job_id) {
            Some(job) => match &job.channel_tx {
                Some(channel) => Some(channel.clone()),
                None => None,
            },
            None => None,
        }
    }
    pub fn get_channel_to_job(
        &self,
        job_id: &String,
    ) -> Option<broadcast::Sender<ChannelMessageToJob>> {
        match self.data.get(job_id) {
            Some(job) => Some(job.channel_to_job_tx.clone()),
            None => None,
        }
    }
}

/// Manages messages for the system
#[derive(Debug, Clone)]
pub struct MessageManager {
    pub from_message_tx: broadcast::Sender<String>,
    pub to_message_tx: broadcast::Sender<String>,
}

impl MessageManager {}

/// Represents a signed message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedMessage {
    pub message: String,
    sign: Vec<u8>,
    pub public_key: [u8; 32],
}

impl SignedMessage {
    /// Verifies the signature of the message
    pub fn verify(&self) -> bool {
        if let Ok(public_key) = ed25519::PublicKey::try_from_bytes(&self.public_key) {
            return public_key.verify(&*self.message.as_bytes(), &self.sign);
        }
        return false;
    }
}

/// Represents a message in the system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub timestamp: String,
    pub content: MessageContent,
    pub from: String,
    pub to: Option<String>,
}
impl Message {
    /// Creates a new message
    pub fn new(content: MessageContent, from: String, to: Option<String>) -> Self {
        let duration_since_epoch = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap();
        let timestamp_nanos = duration_since_epoch.as_nanos().to_string();
        Self {
            timestamp: timestamp_nanos,
            content,
            from,
            to,
        }
    }
    /// Signs the message using the provided keypair
    pub fn signed(self, keypair: ed25519::Keypair) -> Result<SignedMessage, serde_json::Error> {
        let message_str = serde_json::to_string(&self)?;
        let sign = keypair.sign(&*message_str.as_bytes());
        Ok(SignedMessage {
            message: message_str,
            sign,
            public_key: keypair.public().to_bytes(),
        })
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RobotRole {
    Current,
    OrganizationRobot,
    OrganizationUser,
    Owner,
    Unknown,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Robot {
    pub robot_id: String,
    pub robot_peer_id: String,
    pub robot_public_key: String,
    pub name: String,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub username: String,
    pub public_key: String,
    pub tags: Vec<String>,
}

impl User {
    pub fn get_public_key_bytes(&self) -> Result<[u8; 32], Box<dyn Error>> {
        let decoded_key = general_purpose::STANDARD.decode(&self.public_key);
        match decoded_key {
            Ok(decoded_key) => {
                let public_key_bytes: [u8; 32] = decoded_key.as_slice().try_into()?;
                return Ok(public_key_bytes);
            }
            Err(_err) => {
                return Err("can't decode user public key from base64")?;
            }
        }
    }
    pub fn get_peer_id(&self) -> Result<String, Box<dyn Error>> {
        let identity =
            libp2p::identity::ed25519::PublicKey::try_from_bytes(&self.get_public_key_bytes()?)?;
        let public_key: libp2p::identity::PublicKey = identity.into();
        let peer_id = public_key.to_peer_id();
        return Ok(peer_id.to_base58());
    }
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct RobotsConfig {
    pub version: u32,
    pub robots: Vec<Robot>,
    pub users: Vec<User>,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct ConfigStorage {
    configs: HashMap<[u8; 32], (RobotsConfig, SignedMessage)>, // owner public key -> config
}

impl ConfigStorage {
    pub fn update_config(
        &mut self,
        owner: [u8; 32],
        config: RobotsConfig,
        signed_message: SignedMessage,
    ) {
        match self.configs.get(&owner) {
            Some((old_config, _)) => {
                if old_config.version >= config.version {
                    return;
                }
            }
            None => {}
        }

        info!(
            "Updating config in config storage owner {:?} {:?}",
            owner, signed_message
        );
        self.configs.insert(owner, (config, signed_message));
    }
    pub fn get_config(&self, owner: &[u8; 32]) -> Option<(RobotsConfig, SignedMessage)> {
        if let Some((config, message)) = self.configs.get(owner) {
            return Some((config.clone(), message.clone()));
        }
        return None;
    }
    pub fn get_config_version(&self, owner: &[u8; 32]) -> u32 {
        match self.get_config(owner) {
            Some((config, _message)) => {
                return config.version;
            }
            None => {
                return 0;
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Hash, PartialEq, Eq)]
pub struct RobotInterface {
    pub ip4: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RobotsManagerDump {
    config: RobotsConfig,
    config_message: SignedMessage,
}

/// Manages robots in the system
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct RobotsManager {
    pub self_peer_id: String,
    pub owner_peer_id: String,
    pub owner_public_key: [u8; 32],
    pub robots: HashMap<String, Robot>, // peer id tp Robot
    pub users: HashMap<String, User>,   // public key to User
    pub config_version: u32,
    pub config_message: Option<SignedMessage>,
    pub peer_id_to_ip: HashMap<String, String>,
    pub peers: Vec<PeerId>,
    pub config_storage: ConfigStorage,
}

impl RobotsManager {
    /// Adds a robot to the manager
    pub fn add_robot(&mut self, robot: Robot) {
        self.robots
            .insert(robot.robot_peer_id.clone(), robot.clone());
        // if let Some(ip4) = self.peer_id_to_ip.get(&robot.robot_peer_id) {
        //     self.add_interface_to_robot(robot.robot_peer_id, ip4.to_string());
        // }
    }
    pub fn add_user(&mut self, user: User) {
        // if let Ok(peer_id) = user.get_peer_id(){
        //     self.users.insert(peer_id, user.clone());
        // }else{
        //     error!("can't get peer_id for user {:?}", user);
        // }
        self.users.insert(user.public_key.clone(), user);
    }

    /// Sets the robots configuration
    pub fn set_robots_config(&mut self, config: RobotsConfig, signed_message: SignedMessage) {
        if config.version > self.config_version {
            self.config_version = config.version.clone();
            self.robots.clear();
            for robot in &config.robots {
                self.add_robot(robot.clone());
            }
            self.users.clear();
            for user in &config.users {
                self.add_user(user.clone());
            }
            self.config_message = Some(signed_message.clone());
            info!("OWNER_PUBLIC_KEY {:?}", self.owner_public_key);
            self.config_storage.update_config(
                self.owner_public_key,
                config.clone(),
                signed_message,
            );
        } else {
            info!("config version is too old");
        }
    }

    /// Saves the current configuration to a file
    pub fn save_to_file(&self, filepath: String) -> Result<(), Box<dyn Error>> {
        let dump = RobotsManagerDump {
            config: self.get_robots_config(),
            config_message: self
                .config_message
                .clone()
                .ok_or("no config signed message")?,
        };
        fs::write(filepath, serde_json::to_string(&dump)?)?;
        Ok(())
    }
    /// Reads the configuration from a file
    pub fn read_config_from_file(&mut self, filepath: String) -> Result<(), Box<dyn Error>> {
        let dump_str = fs::read_to_string(filepath)?;
        let dump: RobotsManagerDump = serde_json::from_str(&dump_str)?;
        self.set_robots_config(dump.config, dump.config_message);
        Ok(())
    }

    pub fn get_robots_config(&self) -> RobotsConfig {
        return RobotsConfig {
            version: self.config_version,
            robots: self
                .robots
                .values()
                .map(|robot| robot.clone())
                .collect::<Vec<Robot>>(),
            users: self
                .users
                .values()
                .map(|user| user.clone())
                .collect::<Vec<User>>(),
        };
    }

    pub fn set_peers(&mut self, peers: Vec<PeerId>) {
        self.peers = peers
    }

    pub fn set_owner(&mut self, owner_b64: String) -> Result<(), Box<dyn Error>> {
        let decoded_key = general_purpose::STANDARD.decode(owner_b64);
        match decoded_key {
            Ok(owner_public_key) => {
                match owner_public_key.as_slice().try_into() {
                    Ok(pk) => {
                        self.owner_public_key = pk;
                    }
                    _ => {
                        return Err("can't transform pk array".into());
                    }
                };
            }
            _ => {
                return Err("can't decode pk from b64".into());
            }
        }

        Ok(())
    }

    pub fn read_robots_from_config(&mut self, config: String) {
        let robots_config: RobotsConfig =
            serde_json::from_str::<RobotsConfig>(&config).expect("wrong JSON");
        for robot in robots_config.robots.iter() {
            self.add_robot(robot.clone());
        }
    }

    // pub fn add_interface_to_robot(&mut self, robot_peer_id: String, ip4: String) {
    //     info!("Adding interface {} = {}", robot_peer_id, ip4);
    //     match self.robots.get_mut(&robot_peer_id) {
    //         Some(robot) => {
    //             robot.interfaces.insert(RobotInterface { ip4 });
    //         }
    //         None => {
    //             info!("No robot for peer id {}", robot_peer_id);
    //             self.peer_id_to_ip.insert(robot_peer_id, ip4);
    //         }
    //     }
    // }

    /// Gets the role of an entity based on its ID
    pub fn get_role<T: Any>(&self, id: T) -> RobotRole {
        let value_any = &id as &dyn Any;
        if let Some(_peer_id) = value_any.downcast_ref::<String>() {
            // for user_peer_id in self.users.keys() {
            //     if user_peer_id == peer_id {
            //     return RobotRole::OrganizationUser;
            //     }
            // }
        } else if let Some(public_key) = value_any.downcast_ref::<[u8; 32]>() {
            if *public_key == self.owner_public_key {
                return RobotRole::Owner;
            }

            info!("public key: {:?}", public_key);
            let pk_str = general_purpose::STANDARD.encode(&public_key.to_vec());
            info!("pk_str: {}", pk_str);
            for robot_public_key in self
                .robots
                .values()
                .map(|robot| robot.robot_public_key.clone())
            {
                if robot_public_key == pk_str {
                    return RobotRole::OrganizationRobot;
                }
            }
            info!("users: {:?}", self.users);
            if let Some(_user) = self.users.get(&pk_str) {
                return RobotRole::OrganizationUser;
            }
        }

        return RobotRole::Unknown;
    }

    pub fn remove_interface_from_robot(&mut self, _robot_peer_id: String, _ip4: String) {}

    pub fn merge_update(&mut self, update_robots: RobotsConfig) {
        for robot in update_robots.robots.iter() {
            if !self.robots.contains_key(&robot.robot_peer_id) {
                self.add_robot(robot.clone());
            }
        }
    }

    pub fn get_robots_json(self) -> String {
        serde_json::to_string(&self).unwrap()
    }
}

/// Represents the configuration for the system
#[derive(Debug, Clone)]
pub struct Config {
    pub identity: libp2p::identity::ed25519::Keypair,
    pub bootstrap_addrs: Vec<Multiaddr>,
    pub libp2p_port: u16,
}

impl Config {
    /// Generates a new configuration
    pub fn generate() -> Self {
        Self {
            identity: libp2p::identity::ed25519::Keypair::generate(),
            bootstrap_addrs: Vec::new(),
            libp2p_port: 0,
        }
    }

    /// Saves the configuration to a file
    pub fn save_to_file(self: &Self, filepath: String) -> Result<(), Box<dyn Error>> {
        let encoded_key = general_purpose::STANDARD.encode(&self.identity.to_bytes().to_vec());
        fs::write(filepath, encoded_key)?;
        Ok(())
    }

    pub fn get_public_key_encoded(self: &Self) -> String {
        let public_key_encoded =
            general_purpose::STANDARD.encode(&self.identity.public().to_bytes().to_vec());
        public_key_encoded
    }

    pub fn get_peer_id(self: &Self) -> String {
        let public_key: libp2p::identity::PublicKey = self.identity.public().into();
        let peer_id = public_key.to_peer_id();
        return peer_id.to_string();
    }

    /// Loads the configuration from a file
    pub fn load_from_file(filepath: String) -> Result<Self, Box<dyn Error>> {
        let key = fs::read(filepath)?;
        let decoded_key: &mut [u8] = &mut general_purpose::STANDARD.decode(key)?;
        let parsed_identity = libp2p::identity::ed25519::Keypair::try_from_bytes(decoded_key)?;
        Ok(Self {
            identity: parsed_identity,
            bootstrap_addrs: Vec::new(),
            libp2p_port: 0,
        })
    }

    /// Loads the configuration from a secret key string
    pub fn load_from_sk_string(sk_string: String) -> Result<Self, Box<dyn Error>> {
        let decoded_key: &mut [u8] = &mut general_purpose::STANDARD.decode(sk_string)?;
        let parsed_secret = libp2p::identity::ed25519::SecretKey::try_from_bytes(decoded_key)?;
        let identity = libp2p::identity::ed25519::Keypair::from(parsed_secret);
        Ok(Self {
            identity,
            bootstrap_addrs: Vec::new(),
            libp2p_port: 0,
        })
    }

    pub fn add_bootstrap_addr(&mut self, addr: Multiaddr) {
        self.bootstrap_addrs.push(addr);
    }

    pub fn set_libp2p_port(&mut self, port: u16) {
        self.libp2p_port = port;
    }
}

/// Type alias for thread-safe access to RobotsManager
pub type Robots = Arc<Mutex<RobotsManager>>;
/// Type alias for thread-safe access to JobManager
pub type Jobs = Arc<Mutex<JobManager>>;
