use serde::{Deserialize, Serialize};
use base64::{engine::general_purpose, Engine as _};
use libp2p_identity::ed25519;
use std::time::SystemTime;

use super::job_manager::{JobProcessData};
use crate::commands::{RobotJob};

use super::robot_manager::{Robot};
use super::config::{RobotsConfig};



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
    Handshake{}
}

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