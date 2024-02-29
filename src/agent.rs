#[derive(Debug, Clone)]
pub struct Agent {
    pub api_key: String,
    pub robot_server_url: String,
}

pub struct AgentBuilder {
    api_key: Option<String>,
    robot_server_url: Option<String>,
}

impl Default for AgentBuilder {
    fn default() -> Self {
        Self {
            api_key: None,
            robot_server_url: None,
        }
    }
}

impl AgentBuilder {
    pub fn api_key(mut self, api_key: String) -> Self {
        self.api_key = Some(api_key);
        self
    }
    pub fn robot_server_url(mut self, robot_server_url: String) -> Self {
        self.robot_server_url = Some(robot_server_url);
        self
    }
    pub fn build(self) -> Agent {
        Agent {
            api_key: self.api_key.unwrap(),
            robot_server_url: self.robot_server_url.unwrap(),
        }
    }
}
