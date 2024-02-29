use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;
use tokio::sync::mpsc;

#[derive(Debug, Clone, Serialize)]
pub struct Tunnel {
    pub client_id: String,
}

#[derive(Debug, Clone)]
pub struct JobProcess {
    pub job_id: String,
    pub job_type: String,
    pub status: String,
    pub channel_tx: Option<broadcast::Sender<String>>,
    pub channel_to_job_tx: broadcast::Sender<String>,
    pub tunnel: Option<Tunnel>,
}

#[derive(Default, Debug)]
pub struct JobManager {
    pub data: HashMap<String, JobProcess>,
}

impl JobManager {
    pub fn new_job(&mut self, job_id: String, job_type: String, status: String) {
        let (channel_to_job_tx, mut channel_to_job_rx) = broadcast::channel::<String>(100);

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
    pub fn get_job_or_none(&self, job_id: &String) -> Option<JobProcess> {
        match self.data.get(job_id) {
            Some(job) => Some(job.clone()),
            None => None,
        }
    }
    pub fn set_job_status(&mut self, job_id: String, status: String) {
        let mut process = self.data.get_mut(&job_id);
        match process {
            Some(process) => {
                process.status = status;
            }
            None => {}
        }
    }

    pub fn create_job_tunnel(&mut self, job_id: &String, client_id: String) {
        let mut process = self.data.get_mut(job_id);
        let (tx, mut rx) = broadcast::channel::<String>(100);
        match process {
            Some(process) => {
                process.tunnel = Some(Tunnel { client_id });
                process.channel_tx = Some(tx.clone());
            }
            None => {}
        }
    }
    pub fn get_channel_by_job_id(&self, job_id: &String) -> Option<broadcast::Sender<String>> {
        match self.data.get(job_id) {
            Some(job) => match &job.channel_tx {
                Some(channel) => Some(channel.clone()),
                None => None,
            },
            None => None,
        }
    }
    pub fn get_channel_to_job_tx_by_job_id(
        &self,
        job_id: &String,
    ) -> Option<broadcast::Sender<String>> {
        match self.data.get(job_id) {
            Some(job) => Some(job.channel_to_job_tx.clone()),
            None => None,
        }
    }
}

pub type Jobs = Arc<Mutex<JobManager>>;
