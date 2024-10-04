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


/// Manages messages for the system
#[derive(Debug, Clone)]
pub struct MessageManager {
    pub from_message_tx: broadcast::Sender<String>,
    pub to_message_tx: broadcast::Sender<String>,
}

impl MessageManager {}




