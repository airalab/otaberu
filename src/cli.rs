use clap::Parser;

#[derive(Parser, Debug, Clone)]
#[command(author, version, about, long_about = None)]
pub struct Args {
    #[arg(short = 'm', long, default_value = "normal")]
    pub mode: String,

    #[arg(short = 's', long, default_value = "rn.socket")]
    pub socket_filename: String,

    #[arg(short = 'f', long, default_value = "rn.key")]
    pub key_filename: String,

    #[arg(short = 'b', long)]
    pub bootstrap_addr: Option<String>,

    #[arg(short = 'l', long, default_value = "8765")]
    pub port_libp2p: String,

    #[arg(short = 'c', long, default_value = "rn.json")]
    pub config_path: String,

    #[arg(short = 'k', long)]
    pub secret_key: Option<String>,

    #[arg(short = 'o', long)]
    pub owner: String,
}

pub fn get_args() -> Args {
    let args = Args::parse();
    args
}
