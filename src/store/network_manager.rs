use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::SystemTime;

use libp2p;
use tracing::info;

/// Peer info contains status, last hanshake etc for each peer
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct PeerInfo {
    pub peer_id: String,
    pub peers: Vec<String>,
    pub last_handshake: u64,
    pub is_online: bool,
    pub public_key: [u8; 32],
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct NetworkManager {
    pub peers_info: HashMap<String, PeerInfo>,
}

impl NetworkManager {
    pub fn process_handshake(&mut self, pk: &[u8; 32], peers: Vec<String>) {
        if let Ok(identity) = libp2p::identity::ed25519::PublicKey::try_from_bytes(pk) {
            let public_key: libp2p::identity::PublicKey = identity.into();
            let peer_id = public_key.to_peer_id();
            let from = peer_id.to_base58();
            info!("Got handshake from {}", from);

            let cur_time = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            if let Some(peer_info) = self.peers_info.get_mut(&from) {
                peer_info.last_handshake = cur_time;
                peer_info.is_online = true;
            } else {
                let peer_info = PeerInfo {
                    peer_id: from.clone(),
                    public_key: *pk,
                    last_handshake: cur_time,
                    is_online: true,
                    peers,
                };
                self.peers_info.insert(from, peer_info);
            }
        }
    }
    pub fn clean_old_handshakes(&mut self) {
        let cur_time = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        for (_peer_id, peer_info) in self.peers_info.iter_mut() {
            if cur_time - peer_info.last_handshake > 30 {
                peer_info.is_online = false
            }
        }
    }

    pub fn get_peer_id_public_key(&self, peer_id: &String) -> Option<[u8; 32]> {
        if let Some(peer_info) = self.peers_info.get(peer_id) {
            return Some(peer_info.public_key);
        }
        return None;
    }
}
