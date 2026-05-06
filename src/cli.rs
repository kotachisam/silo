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
#[command(after_long_help = SEARCH_LEGEND)]
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

const SEARCH_LEGEND: &str = "\
COLUMNS

Block 1 (perf):
  ID         Vast.ai offer ID — pass to `silo up <ID>`
  CUDA       Maximum CUDA compute capability supported
  N          Number of GPUs in the offer (e.g. 1x, 2x)
  Model      GPU model (spaces replaced with underscores)
  PCIE       PCIe bus bandwidth in GB/s
  cpu_ghz    Host CPU clock speed in GHz
  vCPUs      Virtual CPUs allocated to the instance
  RAM/GB     Host system RAM
  VRAM/GB    GPU memory per card
  Disk/GB    Allocatable disk space
  $/hr       Hourly rental cost in USD
  DLP        Vast.ai 'deep learning performance' benchmark — higher is better
  DLP/$      DLP per dollar/hour — primary value-for-money metric

Block 2 (infra):
  score      Internal vast.ai composite host score
  NV Driver  Host's NVIDIA driver version
  Net_up     Outbound bandwidth in Mbps
  Net_down   Inbound bandwidth in Mbps
  R%         Reliability percentage over the last ~30 days (host uptime)
  Max_Days   Maximum days the host plans to keep this offer available
  mach_id    Vast.ai machine ID
  status     verified | unverified | deverified — host's vast.ai audit standing
  host_id    Vast.ai host ID (one host can offer many machines)
  ports      Direct inbound ports the host exposes

Block 3:
  country    Geographic location of the host

NOTE: this legend shows on `silo search --help`. The short `-h` form keeps it terse.
";

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
