use libp2p::Multiaddr;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fs;
use std::hash::Hash;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;
use tokio::sync::broadcast;
use tracing::info;

use base64::{engine::general_purpose, Engine as _};

use crate::commands::{RobotJob, RobotJobResult};

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
pub enum MessageRequest {
    ListJobs {},
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "response_type")]
pub enum MessageResponse {
    ListJobs { jobs: Vec<JobProcessData> },
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
        config: serde_json::Value,
        signer: String,
        sign: String,
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

#[derive(Default, Debug)]
pub struct JobManager {
    pub data: HashMap<String, JobProcess>,
}

impl JobManager {
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
    pub fn set_job_result(&mut self, reslut: RobotJobResult) {
        let process = self.data.get_mut(&reslut.job_id);
        match process {
            Some(process) => {
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

#[derive(Debug, Clone)]
pub struct MessageManager {
    pub from_message_tx: broadcast::Sender<String>,
    pub to_message_tx: broadcast::Sender<String>,
}

impl MessageManager {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub timestamp: u128,
    pub content: MessageContent,
    pub from: String,
    pub to: Option<String>,
}
impl Message {
    pub fn new(content: MessageContent, from: String, to: Option<String>) -> Self {
        let duration_since_epoch = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap();
        let timestamp_nanos = duration_since_epoch.as_nanos();
        Self {
            timestamp: timestamp_nanos,
            content,
            from,
            to,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RobotRole {
    Current,
    OrganizationRobot,
    OrganizationAdmin,
    Unknown,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Robot {
    pub robot_id: String,
    pub robot_peer_id: String,
    pub name: String,
    pub tags: Vec<String>,
    pub interfaces: HashSet<RobotInterface>,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct RobotsConfig {
    pub robots: Vec<Robot>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Hash, PartialEq, Eq)]
pub struct RobotInterface {
    pub ip4: String,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct RobotsManager {
    pub self_peer_id: String,
    pub robots: HashMap<String, Robot>,
    pub peer_id_to_ip: HashMap<String, String>,
}

impl RobotsManager {
    pub fn add_robot(&mut self, robot: Robot) {
        self.robots
            .insert(robot.robot_peer_id.clone(), robot.clone());
        if let Some(ip4) = self.peer_id_to_ip.get(&robot.robot_peer_id) {
            self.add_interface_to_robot(robot.robot_peer_id, ip4.to_string());
        }
    }

    pub fn read_robots_from_config(&mut self, config: String) {
        let robots_config: RobotsConfig =
            serde_json::from_str::<RobotsConfig>(&config).expect("wrong JSON");
        for robot in robots_config.robots.iter() {
            self.add_robot(robot.clone());
        }
    }

    pub fn add_interface_to_robot(&mut self, robot_peer_id: String, ip4: String) {
        info!("Adding interface {} = {}", robot_peer_id, ip4);
        match self.robots.get_mut(&robot_peer_id) {
            Some(robot) => {
                robot.interfaces.insert(RobotInterface { ip4 });
            }
            None => {
                info!("No robot for peer id {}", robot_peer_id);
                self.peer_id_to_ip.insert(robot_peer_id, ip4);
            }
        }
    }

    pub fn get_role(&self, peer_id: String) -> RobotRole {
        for robot_peer_id in self.robots.keys() {
            if robot_peer_id == &peer_id {
                return RobotRole::OrganizationRobot;
            }
        }
        return RobotRole::Unknown;
    }

    pub fn remove_interface_from_robot(&mut self, robot_peer_id: String, ip4: String) {}

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

#[derive(Debug, Clone)]
pub struct Config {
    pub identity: libp2p::identity::ed25519::Keypair,
    pub bootstrap_addrs: Vec<Multiaddr>,
    pub libp2p_port: u16,
}

impl Config {
    pub fn generate() -> Self {
        Self {
            identity: libp2p::identity::ed25519::Keypair::generate(),
            bootstrap_addrs: Vec::new(),
            libp2p_port: 0,
        }
    }

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

    pub fn add_bootstrap_addr(&mut self, addr: Multiaddr) {
        self.bootstrap_addrs.push(addr);
    }

    pub fn set_libp2p_port(&mut self, port: u16) {
        self.libp2p_port = port;
    }
}

pub type Robots = Arc<Mutex<RobotsManager>>;
pub type Jobs = Arc<Mutex<JobManager>>;
