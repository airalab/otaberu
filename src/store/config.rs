use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::info;

use super::messages::{SignedMessage};
use super::robot_manager::{Robot,User};

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