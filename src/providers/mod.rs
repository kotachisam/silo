pub mod vast;

use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default)]
pub struct SearchFilters {
    pub num_gpus: Option<u32>,
    pub vram_min_gb: Option<u32>,
    pub disk_min_gb: Option<u32>,
    pub max_price_per_hour_usd: Option<f32>,
    pub region: Option<String>,
    pub reliability_min: Option<f32>,
    pub gpu_name: Option<String>,
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Offer {
    pub id: String,
    pub gpu_name: String,
    pub num_gpus: u32,
    pub vram_gb: f32,
    pub disk_gb: u32,
    pub price_per_hour_usd: f32,
    pub region: Option<String>,
    pub reliability: Option<f32>,
    pub cuda: Option<String>,
    pub pcie_bw: Option<f32>,
    pub cpu_ghz: Option<f32>,
    pub vcpus: Option<f32>,
    pub ram_gb: Option<f32>,
    pub dlp: Option<f32>,
    pub dlp_per_dollar: Option<f32>,
    pub score: Option<f32>,
    pub driver: Option<String>,
    pub net_up_mbps: Option<f32>,
    pub net_down_mbps: Option<f32>,
    pub max_days: Option<f32>,
    pub machine_id: Option<u64>,
    pub host_id: Option<u64>,
    pub status: Option<String>,
    pub ports: Option<u32>,
    pub country: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CreateConfig {
    pub image: String,
    pub disk_gb: u32,
    pub boot_script: Option<String>,
}

#[derive(Debug, Clone)]
pub struct InstanceRef {
    pub instance_id: String,
}

#[derive(Debug, Clone)]
pub struct InstanceStatus {
    pub state: String,
    pub ssh_host: Option<String>,
    pub ssh_port: Option<u16>,
    pub cost_per_hour_usd: Option<f32>,
}

#[allow(async_fn_in_trait)]
pub trait Provider {
    fn name(&self) -> &'static str;
    async fn search(&self, filters: &SearchFilters) -> Result<Vec<Offer>>;
    async fn create(&self, offer_id: &str, cfg: &CreateConfig) -> Result<InstanceRef>;
    async fn status(&self, instance_id: &str) -> Result<InstanceStatus>;
    async fn destroy(&self, instance_id: &str) -> Result<()>;
}

pub enum AnyProvider {
    Vast(vast::VastProvider),
}

impl AnyProvider {
    pub fn from_name(name: &str, config: &crate::config::Config) -> Result<Self> {
        match name {
            "vast" => Ok(Self::Vast(vast::VastProvider::resolve(config)?)),
            other => anyhow::bail!("unknown provider: {other}"),
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Vast(p) => p.name(),
        }
    }

    pub async fn search(&self, filters: &SearchFilters) -> Result<Vec<Offer>> {
        match self {
            Self::Vast(p) => p.search(filters).await,
        }
    }

    pub async fn create(&self, offer_id: &str, cfg: &CreateConfig) -> Result<InstanceRef> {
        match self {
            Self::Vast(p) => p.create(offer_id, cfg).await,
        }
    }

    pub async fn status(&self, instance_id: &str) -> Result<InstanceStatus> {
        match self {
            Self::Vast(p) => p.status(instance_id).await,
        }
    }

    pub async fn destroy(&self, instance_id: &str) -> Result<()> {
        match self {
            Self::Vast(p) => p.destroy(instance_id).await,
        }
    }
}
