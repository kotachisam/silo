use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "silo", version, about = "Ephemeral GPU rental orchestration")]
pub struct Cli {
    #[arg(short = 'p', long, global = true)]
    pub provider: Option<String>,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    Search(SearchArgs),
    Up(UpArgs),
    Status,
    Ssh {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        remote: Vec<String>,
    },
    Tunnel(TunnelArgs),
    Down,
}

#[derive(Args, Debug)]
pub struct SearchArgs {
    #[arg(long, default_value_t = 1)]
    pub gpus: u32,
    #[arg(long, default_value_t = 90)]
    pub vram: u32,
    #[arg(long, default_value_t = 200)]
    pub disk: u32,
    #[arg(long)]
    pub max_price: Option<f32>,
    #[arg(long, default_value = "US")]
    pub region: String,
    #[arg(long, default_value_t = 0.99)]
    pub reliability: f32,
    #[arg(long)]
    pub gpu_name: Option<String>,
    #[arg(long, default_value_t = 20)]
    pub limit: u32,
}

#[derive(Args, Debug)]
pub struct UpArgs {
    pub offer_id: String,
    #[arg(long, default_value = "ubuntu:22.04")]
    pub image: String,
    #[arg(long, default_value_t = 200)]
    pub disk: u32,
    #[arg(long)]
    pub boot: Option<PathBuf>,
}

#[derive(Args, Debug)]
pub struct TunnelArgs {
    pub port: u16,
    #[arg(long)]
    pub remote_port: Option<u16>,
}
