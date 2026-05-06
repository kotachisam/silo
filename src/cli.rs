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
    Cost,
    Config(ConfigArgs),
}

#[derive(Args, Debug)]
pub struct ConfigArgs {
    #[command(subcommand)]
    pub action: ConfigAction,
}

#[derive(Subcommand, Debug)]
pub enum ConfigAction {
    /// Print the current config file contents
    Show,
    /// Open the config file in $EDITOR (creates it with a commented template if missing)
    Edit,
}

#[derive(Args, Debug)]
#[command(after_long_help = SEARCH_LEGEND)]
pub struct SearchArgs {
    #[arg(long, help = "Number of GPUs per offer (config: search.default_gpus, fallback: 1)")]
    pub gpus: Option<u32>,
    #[arg(long, help = "Minimum VRAM per GPU in GB (config: search.default_vram_gb, fallback: 90)")]
    pub vram: Option<u32>,
    #[arg(long, help = "Minimum disk space in GB (config: search.default_disk_gb, fallback: 200)")]
    pub disk: Option<u32>,
    #[arg(long, help = "Maximum hourly price in USD (config: search.default_max_price)")]
    pub max_price: Option<f32>,
    #[arg(long, help = "Geographic region (config: search.default_region, fallback: US)")]
    pub region: Option<String>,
    #[arg(long, help = "Minimum host reliability 0-1 (config: search.default_reliability, fallback: 0.99)")]
    pub reliability: Option<f32>,
    #[arg(long, help = "GPU model exact match (e.g. 'RTX 4090')")]
    pub gpu_name: Option<String>,
    #[arg(long, help = "Maximum offers to return (config: search.default_limit, fallback: 20)")]
    pub limit: Option<u32>,
    #[arg(long, help = "Force verified-only filter (presence overrides config to true)")]
    pub verified_only: bool,
    #[arg(long, help = "Force include-deverified (presence overrides config to true)")]
    pub include_deverified: bool,
}

const SEARCH_LEGEND: &str = "\
COLUMNS

Block 1 (perf):
  ID         Vast.ai offer ID — pass to `silo up <ID>`
  CUDA       Maximum CUDA compute capability supported
  N          Number of GPUs in the offer (e.g. 1x, 2x)
  Model      GPU model (spaces replaced with underscores)
  PCIE       PCIe bus bandwidth in GB/s
  GHz        Host CPU clock speed
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

STATUS FILTERING

By default, deverified hosts are excluded — these are hosts vast.ai has
actively pulled the trust badge from, which is a stronger negative signal
than 'never verified.' Override with:
  --verified-only       Only audited hosts (recommended for production)
  --include-deverified  Show all hosts including deverified

NOTE: this legend shows on `silo search --help`. The short `-h` form keeps it terse.
";

#[derive(Args, Debug)]
pub struct UpArgs {
    pub offer_id: String,
    #[arg(long, help = "Profile name from config (config: up.default_profile)")]
    pub profile: Option<String>,
    #[arg(long, help = "Docker image (overrides profile, fallback: ubuntu:22.04)")]
    pub image: Option<String>,
    #[arg(long, help = "Disk size in GB (overrides profile, fallback: 200)")]
    pub disk: Option<u32>,
    #[arg(long, help = "Path to boot script (overrides profile)")]
    pub boot: Option<PathBuf>,
    #[arg(long = "env", help = "Env var KEY=VALUE to inject (repeatable, overrides profile)")]
    pub env: Vec<String>,
}

#[derive(Args, Debug)]
pub struct TunnelArgs {
    pub port: u16,
    #[arg(long)]
    pub remote_port: Option<u16>,
}
