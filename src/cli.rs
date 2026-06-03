use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

pub const VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), " (", env!("SILO_GIT_SHA"), ")");

#[derive(Parser, Debug)]
#[command(
    name = "silo",
    version = VERSION,
    disable_version_flag = true,
    about = "Ephemeral GPU rental orchestration"
)]
pub struct Cli {
    #[arg(
        short = 'v',
        long = "version",
        action = clap::ArgAction::Version,
        help = "Print version"
    )]
    pub version: Option<bool>,

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
    Config(ConfigArgs),
    /// List trending text-generation models from Hugging Face
    Models(ModelsArgs),
    /// Send a one-shot prompt to the active instance's vLLM endpoint
    Prompt(PromptArgs),
    /// Tail or save vLLM log from the active instance
    Logs(LogsArgs),
}

#[derive(Args, Debug)]
pub struct LogsArgs {
    #[arg(
        short = 'n',
        long,
        default_value_t = 200,
        help = "Number of lines to tail"
    )]
    pub tail: u32,
    #[arg(short = 'f', long, help = "Follow (live tail, Ctrl-C to stop)")]
    pub follow: bool,
    #[arg(short = 's', long, num_args = 0..=1, default_missing_value = "", help = "Save full log to local file (auto-named if no path given)")]
    pub save: Option<String>,
    #[arg(
        long,
        help = "Override remote log path (else: profile.log_path → auto-discover /var/log/*.log)"
    )]
    pub path: Option<String>,
    #[arg(long, help = "List available .log files on the instance and exit")]
    pub list: bool,
    #[arg(
        short = 'a',
        long,
        help = "Tail all /var/log/*.log files together (with file headers)"
    )]
    pub all: bool,
}

#[derive(Args, Debug)]
pub struct PromptArgs {
    /// The prompt text. Use quotes for multi-word prompts.
    pub prompt: String,
    #[arg(long, help = "Model name (defaults to active profile's MODEL env)")]
    pub model: Option<String>,
    #[arg(long, default_value_t = 1024, help = "Maximum tokens in response")]
    pub max_tokens: u32,
    #[arg(
        long,
        help = "Print full JSON response instead of just message content"
    )]
    pub json: bool,
    #[arg(long, help = "Optional system prompt")]
    pub system: Option<String>,
}

#[derive(Args, Debug)]
pub struct ModelsArgs {
    #[arg(
        short = 'l',
        long,
        default_value_t = 20,
        help = "Maximum number of models to fetch (HF allows up to 1000)"
    )]
    pub limit: u32,
    #[arg(
        short = 's',
        long,
        help = "Substring filter on model name (e.g. 'coder', 'reasoning')"
    )]
    pub search: Option<String>,
    #[arg(
        long = "min",
        help = "Filter to models with at least N billion parameters"
    )]
    pub min_params: Option<f32>,
    #[arg(
        long = "max",
        help = "Filter to models with at most N billion parameters"
    )]
    pub max_params: Option<f32>,
    #[arg(
        long,
        help = "Filter to models with at least N downloads (cuts amateur re-uploads)"
    )]
    pub min_downloads: Option<u64>,
    #[arg(long, value_enum, default_value = "trending", help = "Sort order")]
    pub sort: SortOrder,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum SortOrder {
    /// Currently trending (default — what's hot today)
    Trending,
    /// All-time popular (cumulative downloads)
    Downloads,
    /// Community favourites (HF likes)
    Likes,
    /// Most recently updated weights
    Modified,
    /// Most recently uploaded
    Created,
}

impl SortOrder {
    pub fn to_hf_field(self) -> &'static str {
        match self {
            SortOrder::Trending => "trendingScore",
            SortOrder::Downloads => "downloads",
            SortOrder::Likes => "likes",
            SortOrder::Modified => "lastModified",
            SortOrder::Created => "createdAt",
        }
    }
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
    #[arg(
        short = 'g',
        long,
        help = "Number of GPUs per offer (config: search.default_gpus, fallback: 1)"
    )]
    pub gpus: Option<u32>,
    #[arg(
        short = 'v',
        long,
        help = "Minimum VRAM per GPU in GB (config: search.default_vram_gb, fallback: 90)"
    )]
    pub vram: Option<u32>,
    #[arg(
        short = 'd',
        long,
        help = "Minimum disk space in GB (config: search.default_disk_gb, fallback: 200)"
    )]
    pub disk: Option<u32>,
    #[arg(
        long,
        help = "Maximum hourly price in USD (config: search.default_max_price)"
    )]
    pub max_price: Option<f32>,
    #[arg(
        short = 'r',
        long,
        help = "Geographic region (config: search.default_region, fallback: US)"
    )]
    pub region: Option<String>,
    #[arg(
        long,
        help = "Minimum host reliability 0-1 (config: search.default_reliability, fallback: 0.99)"
    )]
    pub reliability: Option<f32>,
    #[arg(long, help = "GPU model exact match (e.g. 'RTX 4090')")]
    pub gpu_name: Option<String>,
    #[arg(
        short = 'l',
        long,
        help = "Maximum offers to return (config: search.default_limit, fallback: 20)"
    )]
    pub limit: Option<u32>,
    #[arg(
        long,
        help = "Force verified-only filter (presence overrides config to true)"
    )]
    pub verified_only: bool,
    #[arg(
        long,
        help = "Force include-deverified (presence overrides config to true)"
    )]
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
    #[arg(
        short = 'i',
        long,
        help = "Docker image (overrides profile, fallback: ubuntu:22.04)"
    )]
    pub image: Option<String>,
    #[arg(
        short = 'd',
        long,
        help = "Disk size in GB (overrides profile, fallback: 200)"
    )]
    pub disk: Option<u32>,
    #[arg(short = 'b', long, help = "Path to boot script (overrides profile)")]
    pub boot: Option<PathBuf>,
    #[arg(
        short = 'e',
        long = "env",
        help = "Env var KEY=VALUE to inject (repeatable, overrides profile)"
    )]
    pub env: Vec<String>,
    #[arg(
        short = 'w',
        long,
        help = "Block until the workload reports ready (polls every 60s, 30m timeout)"
    )]
    pub wait: bool,
    #[arg(
        long,
        help = "Skip the GPU-arch compatibility check against the profile's block_arch list"
    )]
    pub skip_compat_check: bool,
}

#[derive(Args, Debug)]
pub struct TunnelArgs {
    pub port: u16,
    #[arg(short = 'r', long)]
    pub remote_port: Option<u16>,
}
