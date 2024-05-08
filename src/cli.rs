use clap::Parser;

#[derive(Parser, Debug, Clone)]
#[command(author, version, about, long_about = None)]
pub struct Args {
    #[arg(short, long)]
    pub api_key: String,
    #[arg(short, long, default_value = "http://robots.merklebot.com:8888")]
    pub robot_server_url: String,
    #[arg(short, long, default_value = "normal")]
    pub mode: String,
}

pub fn get_args() -> Args {
    let args = Args::parse();
    args
}
