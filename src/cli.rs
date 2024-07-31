use clap::Parser;

#[derive(Parser, Debug, Clone)]
#[command(author, version, about, long_about = None)]
pub struct Args {
    #[arg(short, long)]
    pub api_key: String,

    #[arg(short, long, default_value = "https://robots.merklebot.com")]
    pub robot_server_url: String,

    #[arg(short, long, default_value = "normal")]
    pub mode: String,

    #[arg(short, long, default_value = "merklebot.socket")]
    pub socket_filename: String,

    #[arg(short, long, default_value = "merklebot.key")]
    pub key_filename: String,

    #[arg(short, long)]
    pub bootstrap_addr: Option<String>,

    #[arg(short, long, default_value = "8765")]
    pub port_libp2p: String,
}

pub fn get_args() -> Args {
    let args = Args::parse();
    args
}
