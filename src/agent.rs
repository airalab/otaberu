#[derive(Debug)]
pub struct Agent {
    pub api_key: String,
}

pub struct AgentBuilder {
    api_key: Option<String>,
}

impl Default for AgentBuilder {
    fn default() -> Self {
        Self { api_key: None }
    }
}

impl AgentBuilder {
    pub fn api_key(mut self, api_key: String) -> Self {
        self.api_key = Some(api_key);
        self
    }

    pub fn build(self) -> Agent {
        Agent {
            api_key: self.api_key.unwrap(),
        }
    }
}
