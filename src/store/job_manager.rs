use tokio::sync::broadcast;
use std::sync::{Arc, Mutex};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use super::messages::{ChannelMessageFromJob, ChannelMessageToJob};
use crate::commands::RobotJobResult;

/// Represents a tunnel for job communication
#[derive(Debug, Clone, Serialize)]
pub struct Tunnel {
    pub client_id: String,
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

/// Manages job processes
#[derive(Default, Debug)]
pub struct JobManager {
    pub data: HashMap<String, JobProcess>,
}

impl JobManager {
    /// Creates a new job
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
    /// Sets the result of a job
    pub fn set_job_result(&mut self, reslut: RobotJobResult) {
        let process = self.data.get_mut(&reslut.job_id);
        match process {
            Some(_process) => {
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
    /// Retrieves job information
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

    /// Creates a tunnel for a job
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


pub type Jobs = Arc<Mutex<JobManager>>;
