use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;
use tokio::sync::broadcast;
use tracing::{info, error};
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
        let (channel_to_job_tx, _channel_to_job_rx) = broadcast::channel::<String>(100);

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
        let process = self.data.get_mut(&job_id);
        match process {
            Some(process) => {
                process.status = status;
            }
            None => {}
        }
    }

    pub fn create_job_tunnel(&mut self, job_id: &String, client_id: String) {
        let process = self.data.get_mut(job_id);
        let (tx, _rx) = broadcast::channel::<String>(100);
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

#[derive(Debug, Clone)]
pub struct MessageManager {
    pub from_message_tx: broadcast::Sender<String>,
    pub to_message_tx: broadcast::Sender<String>,
}

impl MessageManager {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub timestamp: u128,
    pub content: String,
    pub from: Option<String>,
    pub to: Option<String>
}
impl Message {
    pub fn new(content: String, from:Option<String>, to: Option<String>) -> Self {
        let duration_since_epoch = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap();
        let timestamp_nanos = duration_since_epoch.as_nanos();

        Self {
            timestamp: timestamp_nanos,
            content,
            from,
            to
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Robot{
    pub robot_id: String,
    pub robot_peer_id: String,
    pub name: String,
    pub tags: Vec<String>,
    pub interfaces: Vec<RobotInterface>
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RobotsConfig{
    pub robots: Vec<Robot>
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RobotInterface{
    pub ip4: String
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct RobotsManager{
   pub robots: HashMap<String, Robot> 

}

impl RobotsManager{
   pub fn add_robot(&mut self, robot: Robot){
        info!("Adding robot: {:?}", robot);
        self.robots.insert(robot.robot_peer_id.clone(), robot);
   }

   pub fn read_robots_from_config(&mut self, config: String){
        let robots_config: RobotsConfig = serde_json::from_str::<RobotsConfig>(&config).expect("wrong JSON");
        for robot in robots_config.robots.iter(){
            self.add_robot(robot.clone());
        }
   }

   pub fn add_interface_to_robot(&mut self, robot_peer_id: String, ip4: String){
       info!("Adding interface {} = {}", robot_peer_id, ip4);
        match self.robots.get_mut(&robot_peer_id){
            Some(robot)=>{
                robot.interfaces.push(RobotInterface{ip4})
            },
            None =>{}
        }
   }

   pub fn remove_interface_from_robot(&mut self, robot_peer_id: String, ip4: String){
        
   }

   pub fn get_robots_json(self)->String{
        serde_json::to_string(&self).unwrap()
        
   }



}

pub type Robots = Arc<Mutex<RobotsManager>>;
pub type Jobs = Arc<Mutex<JobManager>>;
