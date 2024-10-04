use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::SystemTime;

/// Peer info contains status, last hanshake etc for each peer
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct PeerInfo{
    pub peer_id: String,

    pub last_handshake: u64,
    pub is_online: bool
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct NetworkManager{
    pub peers_info: HashMap<String, PeerInfo>
}

impl NetworkManager{
    pub fn process_handshake(&mut self, peer_id: String){
        let cur_time = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs();    
        if let Some(peer_info) = self.peers_info.get_mut(&peer_id){
           peer_info.last_handshake = cur_time;
           peer_info.is_online = true; 
        } else{
            let peer_info = PeerInfo{
                peer_id: peer_id.clone(),
                last_handshake: cur_time,
                is_online: true
            };
            self.peers_info.insert(peer_id, peer_info);
        }
    }
    pub fn clean_old_handshakes(&mut self){
        let cur_time = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs();    
        
        for (_peer_id, peer_info) in self.peers_info.iter_mut(){
            if cur_time - peer_info.last_handshake > 60{
                peer_info.is_online = false
            }
        }
    }
    
}
