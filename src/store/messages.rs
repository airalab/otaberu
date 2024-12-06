use base64::{engine::general_purpose, Engine as _};
use libp2p_identity::ed25519::{self, PublicKey};
use serde::{Deserialize, Serialize};
use std::{io::Read, time::SystemTime};

use super::job_manager::{JobInfo, JobProcessData};
use crate::commands::RobotJob;

use super::config::RobotsConfig;
use super::robot_manager::Robot;

use rand::{rngs::OsRng, RngCore};
use sha2::{Digest, Sha256};
use x25519_dalek::{EphemeralSecret, PublicKey as X25519Public, StaticSecret};

use chacha20poly1305::{
    aead::{Aead, KeyInit},
    ChaCha20Poly1305, Nonce,
};

use tracing::{info, debug};

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
    JobInfo { job_id: String },
    GetRobotsConfig {},
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "response_type")]
pub enum MessageResponse {
    ListJobs { jobs: Vec<JobProcessData> },
    JobInfo { job_info: Option<JobInfo> },
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
    Handshake {
        peers: Vec<String>,
    },
}

pub trait VerifiableMessage {
    fn verify(&self) -> bool;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptionInfo {
    ephemeral_public: [u8; 32],
    nonce: [u8; 12],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedMessage {
    pub message: String,
    pub encryption: Option<EncryptionInfo>,
    pub sign: Vec<u8>,
    pub to: Option<String>,
    pub public_key: [u8; 32],
}

impl SignedMessage {
    pub fn decrypt(&self, keypair: ed25519::Keypair) -> SignedMessage {
        if let Some(enc_info) = self.encryption.clone() {
            let from_x25519 = ed25519_pub_to_x25519(&self.public_key);
            let ephemeral_x25519 = X25519Public::from(enc_info.ephemeral_public);

            let mut hasher = Sha256::new();
            hasher.update(keypair.secret().as_ref());
            let hash_bytes: [u8; 32] = hasher.finalize().into();
            let x25519_secret = StaticSecret::from(hash_bytes);

            let dh1 = x25519_secret.diffie_hellman(&ephemeral_x25519);
            let dh2 = x25519_secret.diffie_hellman(&from_x25519);

            let mut hasher = Sha256::new();
            hasher.update(dh1.as_bytes());
            hasher.update(dh2.as_bytes());
            let key = hasher.finalize();

            let cipher = ChaCha20Poly1305::new(&key);
            if let Ok(dec_data) = cipher.decrypt(Nonce::from_slice(&enc_info.nonce), self.message.as_ref()) {
                if let Ok(decrypted_str) = String::from_utf8(dec_data) {
                    return SignedMessage {
                        message: decrypted_str,
                        encryption: None,
                        sign: self.sign.clone(),
                        to: self.to.clone(),
                        public_key: self.public_key,
                    };
                }
            }
        }
        self.clone()
    }
}
impl VerifiableMessage for SignedMessage {
    /// Verifies the signature of the message
    fn verify(&self) -> bool {
        match &self.encryption {
            Some(_enc_info) => {
                if let Ok(decoded_enc_msg) = general_purpose::STANDARD.decode(&self.message) {
                    if let Ok(public_key) = ed25519::PublicKey::try_from_bytes(&self.public_key) {
                        return public_key.verify(&*&decoded_enc_msg, &self.sign);
                    }
                }
            }
            None => {
                if let Ok(public_key) = ed25519::PublicKey::try_from_bytes(&self.public_key) {
                    return public_key.verify(self.message.as_bytes(), &self.sign);
                }
            }
        }
        false
    }
}
pub fn ed25519_pub_to_x25519(ed25519_pk: &[u8; 32]) -> X25519Public {
    let mut hasher = Sha256::new();
    hasher.update(ed25519_pk);
    let x25519_bytes: [u8; 32] = hasher.finalize().into();
    X25519Public::from(x25519_bytes)
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
        let sign = keypair.sign(message_str.as_bytes());
        Ok(SignedMessage {
            message: message_str,
            encryption: None,
            sign,
            to: self.to,
            public_key: keypair.public().to_bytes(),
        })
    }

    pub fn encrypted(
        self,
        keypair: ed25519::Keypair,
        to_pk: &[u8; 32],
    ) -> Result<SignedMessage, serde_json::Error> {
        let message_str = serde_json::to_string(&self)?;

        let mut hasher = Sha256::new();
        hasher.update(keypair.secret().as_ref());
        let hash_bytes: [u8; 32] = hasher.finalize().into();
        let x25519_secret = StaticSecret::from(hash_bytes);

        let ephemeral_secret = EphemeralSecret::random_from_rng(OsRng);
        let ephemeral_public = X25519Public::from(&ephemeral_secret);

        let to_x25519 = ed25519_pub_to_x25519(to_pk);
        info!("to_x25519 {:?}", to_x25519);
        let dh1 = ephemeral_secret.diffie_hellman(&to_x25519);
        info!("dh1 {:?}", dh1.as_bytes());
        let dh2 = x25519_secret.diffie_hellman(&to_x25519);
        info!("dh2 {:?}", dh2.as_bytes());

        let mut hasher = Sha256::new();
        hasher.update(dh1.as_bytes());
        hasher.update(dh2.as_bytes());
        let key = hasher.finalize();
        info!("key {:?}", key);

        let mut nonce = [0u8; 12];
        OsRng.fill_bytes(&mut nonce);

        let cipher = ChaCha20Poly1305::new(&key);
        let encrypted_data = cipher
            .encrypt(Nonce::from_slice(&nonce), message_str.as_bytes())
            .unwrap(); // TODO: remove unwrap

        let sign = keypair.sign(&encrypted_data);

        let enc_data_encoded = general_purpose::STANDARD.encode(encrypted_data);
        Ok(SignedMessage {
            message: enc_data_encoded,
            encryption: Some(EncryptionInfo {
                ephemeral_public: *ephemeral_public.as_bytes(),
                nonce,
            }),
            sign,
            to: self.to,
            public_key: keypair.public().to_bytes(),
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use libp2p_identity::Keypair;

    #[test]
    fn test_message_signing() {
        let keypair = Keypair::generate_ed25519().try_into_ed25519().unwrap();
        let public_key: libp2p::identity::PublicKey = keypair.public().into();
        let message = Message::new(
            MessageContent::CustomMessage(serde_json::json!({ "test": "test" })),
            public_key.to_peer_id().to_string(),
            None,
        );
        
        let signed = message.signed(keypair).unwrap();
        assert!(signed.verify());
    }

    #[test]
    fn test_message_encryption() {
        let keypair1 = Keypair::generate_ed25519().try_into_ed25519().unwrap();
        let keypair2 = Keypair::generate_ed25519().try_into_ed25519().unwrap();
        
        let message = Message::new(
            MessageContent::CustomMessage(serde_json::json!({ "message": "test" })),
            "from".to_string(), 
            Some("to".to_string()),
        );

        let encrypted = message.clone().encrypted(keypair1, &keypair2.public().to_bytes()).unwrap();
        assert!(encrypted.verify());
        assert!(encrypted.encryption.is_some());
        println!("Encrypted message {:?}", encrypted);
        let decrypted = encrypted.decrypt(keypair2);
        println!("Decrypted message {:?}", decrypted);
        let decrypted_message: Message = serde_json::from_str(&decrypted.message).unwrap();
        
        assert_eq!(serde_json::to_string(&decrypted_message.content).unwrap(), serde_json::to_string(&message.content).unwrap());
    }
}
