use base64::{engine::general_purpose, Engine as _};
use libp2p::{Multiaddr, PeerId};
use serde::{Deserialize, Serialize};
use std::any::Any;
use std::collections::HashMap;
use std::error::Error;
use std::fs::{self};
use std::sync::{Arc, Mutex};
use tracing::info;

use super::config::{ConfigStorage, RobotsConfig};
use super::messages::SignedMessage;
use super::network_manager::NetworkManager;

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

#[derive(Debug, Clone, Serialize, Deserialize, Hash, PartialEq, Eq)]
pub struct RobotInterface {
    pub ip4: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RobotsManagerDump {
    config: RobotsConfig,
    config_message: SignedMessage,
}

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
    pub network_manager: NetworkManager,
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

    pub fn get_peers(&self) -> Vec<PeerId> {
        self.peers.clone()
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

/// Type alias for thread-safe access to RobotsManager
pub type Robots = Arc<Mutex<RobotsManager>>;
