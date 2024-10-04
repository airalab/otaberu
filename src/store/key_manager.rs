use libp2p::{Multiaddr, PeerId};
use base64::{engine::general_purpose, Engine as _};
use std::fs::{self};
use std::error::Error;



/// Manages robots in the system
/// Represents the configuration for the system
#[derive(Debug, Clone)]
pub struct KeyConfig {
    pub identity: libp2p::identity::ed25519::Keypair,
    pub bootstrap_addrs: Vec<Multiaddr>,
    pub libp2p_port: u16,
}

impl KeyConfig {
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
