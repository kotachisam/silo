use super::{CreateConfig, InstanceRef, InstanceStatus, Offer, Provider, SearchFilters};
use anyhow::{Context, Result};
use reqwest::Client;
use serde::Deserialize;
use serde_json::{json, Value};

pub struct VastProvider {
    client: Client,
    base_url: String,
    api_key: String,
}

impl VastProvider {
    pub fn resolve(config: &crate::config::Config) -> Result<Self> {
        let api_key = std::env::var("VAST_API_KEY")
            .ok()
            .or_else(|| config.vast_api_key.clone())
            .context("vast.ai API key not found: set VAST_API_KEY env var or add vast_api_key to ~/.config/silo/config.toml")?;
        Ok(Self {
            client: Client::new(),
            base_url: "https://console.vast.ai/api/v0".into(),
            api_key,
        })
    }

    #[cfg(test)]
    pub fn with_base_url(base_url: String) -> Self {
        Self {
            client: Client::new(),
            base_url,
            api_key: "test-key".into(),
        }
    }
}

fn filters_to_vast_body(f: &SearchFilters) -> Value {
    let mut body = serde_json::Map::new();
    if let Some(n) = f.num_gpus {
        body.insert("num_gpus".into(), json!({"eq": n}));
    }
    if let Some(v) = f.vram_min_gb {
        body.insert("gpu_ram".into(), json!({"gte": v * 1024}));
    }
    if let Some(d) = f.disk_min_gb {
        body.insert("disk_space".into(), json!({"gte": d}));
    }
    if let Some(p) = f.max_price_per_hour_usd {
        body.insert("dph_total".into(), json!({"lte": p}));
    }
    if let Some(r) = &f.region {
        body.insert("geolocation".into(), json!({"eq": r}));
    }
    if let Some(rel) = f.reliability_min {
        body.insert("reliability2".into(), json!({"gte": rel}));
    }
    if let Some(g) = &f.gpu_name {
        body.insert("gpu_name".into(), json!({"eq": g}));
    }
    body.insert("order".into(), json!([["dph_total", "asc"]]));
    if let Some(l) = f.limit {
        body.insert("limit".into(), json!(l));
    }
    Value::Object(body)
}

#[derive(Deserialize)]
struct VastSearchResponse {
    offers: Vec<VastOffer>,
}

#[derive(Deserialize)]
struct VastOffer {
    id: u64,
    gpu_name: String,
    num_gpus: u32,
    gpu_ram: f64,
    disk_space: f64,
    dph_total: f32,
    #[serde(default)]
    geolocation: Option<String>,
    #[serde(default)]
    reliability2: Option<f32>,
    #[serde(default)]
    cuda_max_good: Option<f32>,
    #[serde(default)]
    pcie_bw: Option<f32>,
    #[serde(default)]
    cpu_ghz: Option<f32>,
    #[serde(default)]
    cpu_cores_effective: Option<f32>,
    #[serde(default)]
    cpu_ram: Option<f64>,
    #[serde(default)]
    dlperf: Option<f32>,
    #[serde(default)]
    dlperf_per_dphtotal: Option<f32>,
    #[serde(default)]
    score: Option<f32>,
    #[serde(default)]
    driver_version: Option<String>,
    #[serde(default)]
    inet_up: Option<f32>,
    #[serde(default)]
    inet_down: Option<f32>,
    #[serde(default, alias = "duration")]
    max_days_in_use: Option<f32>,
    #[serde(default)]
    machine_id: Option<u64>,
    #[serde(default)]
    host_id: Option<u64>,
    #[serde(default)]
    verification: Option<String>,
    #[serde(default)]
    direct_port_count: Option<u32>,
}

impl Provider for VastProvider {
    fn name(&self) -> &'static str {
        "vast"
    }

    async fn search(&self, filters: &SearchFilters) -> Result<Vec<Offer>> {
        let body = filters_to_vast_body(filters);
        let url = format!("{}/bundles/", self.base_url);
        let resp: VastSearchResponse = self
            .client
            .post(&url)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .with_context(|| format!("POST {url}"))?
            .error_for_status()?
            .json()
            .await
            .context("decoding /bundles/ response")?;

        Ok(resp
            .offers
            .into_iter()
            .map(|o| Offer {
                id: o.id.to_string(),
                gpu_name: o.gpu_name,
                num_gpus: o.num_gpus,
                vram_gb: (o.gpu_ram / 1024.0) as f32,
                disk_gb: o.disk_space as u32,
                price_per_hour_usd: o.dph_total,
                region: o.geolocation.clone(),
                reliability: o.reliability2,
                cuda: o.cuda_max_good.map(|c| format!("{c:.1}")),
                pcie_bw: o.pcie_bw,
                cpu_ghz: o.cpu_ghz,
                vcpus: o.cpu_cores_effective,
                ram_gb: o.cpu_ram.map(|r| (r / 1024.0) as f32),
                dlp: o.dlperf,
                dlp_per_dollar: o.dlperf_per_dphtotal,
                score: o.score,
                driver: o.driver_version,
                net_up_mbps: o.inet_up,
                net_down_mbps: o.inet_down,
                max_days: o.max_days_in_use.map(|s| s / 86400.0),
                machine_id: o.machine_id,
                host_id: o.host_id,
                status: o.verification,
                ports: o.direct_port_count,
                country: o.geolocation,
            })
            .collect())
    }

    async fn create(&self, offer_id: &str, cfg: &CreateConfig) -> Result<InstanceRef> {
        let url = format!("{}/asks/{}/", self.base_url, offer_id);
        let mut body = json!({
            "image": cfg.image,
            "disk": cfg.disk_gb,
        });
        if let Some(boot) = &cfg.boot_script {
            body["onstart"] = json!(boot);
        }
        let resp: Value = self
            .client
            .put(&url)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .with_context(|| format!("PUT {url}"))?
            .error_for_status()?
            .json()
            .await
            .context("decoding /asks/{id}/ response")?;

        let new_id = resp
            .get("new_contract")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| anyhow::anyhow!("vast response missing new_contract: {resp}"))?;
        Ok(InstanceRef {
            instance_id: new_id.to_string(),
        })
    }

    async fn status(&self, instance_id: &str) -> Result<InstanceStatus> {
        let url = format!("{}/instances/{}/", self.base_url, instance_id);
        let resp: Value = self
            .client
            .get(&url)
            .bearer_auth(&self.api_key)
            .send()
            .await
            .with_context(|| format!("GET {url}"))?
            .error_for_status()?
            .json()
            .await
            .context("decoding /instances/{id}/ response")?;

        let inst = resp.get("instances").unwrap_or(&resp);
        Ok(InstanceStatus {
            state: inst
                .get("actual_status")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string(),
            ssh_host: inst
                .get("ssh_host")
                .and_then(|v| v.as_str())
                .map(String::from),
            ssh_port: inst
                .get("ssh_port")
                .and_then(|v| v.as_u64())
                .map(|p| p as u16),
            cost_per_hour_usd: inst
                .get("dph_total")
                .and_then(|v| v.as_f64())
                .map(|p| p as f32),
        })
    }

    async fn destroy(&self, instance_id: &str) -> Result<()> {
        let url = format!("{}/instances/{}/", self.base_url, instance_id);
        self.client
            .delete(&url)
            .bearer_auth(&self.api_key)
            .send()
            .await
            .with_context(|| format!("DELETE {url}"))?
            .error_for_status()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockito::Server;
    use serde_json::json;

    #[tokio::test]
    async fn search_translates_filters_to_vast_body() {
        let filters = SearchFilters {
            vram_min_gb: Some(90),
            disk_min_gb: Some(200),
            region: Some("US".into()),
            reliability_min: Some(0.99),
            ..Default::default()
        };
        let body = filters_to_vast_body(&filters);

        assert_eq!(body["gpu_ram"]["gte"].as_u64().unwrap(), 92160);
        assert_eq!(body["disk_space"]["gte"].as_u64().unwrap(), 200);
        assert_eq!(body["geolocation"]["eq"].as_str().unwrap(), "US");
        let rel = body["reliability2"]["gte"].as_f64().unwrap();
        assert!((rel - 0.99).abs() < 0.001, "reliability2 = {rel}");

        let mut server = Server::new_async().await;
        let mock = server
            .mock("POST", "/bundles/")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"offers": []}"#)
            .create_async()
            .await;

        let provider = VastProvider::with_base_url(server.url());
        let offers = provider.search(&filters).await.unwrap();
        assert_eq!(offers.len(), 0);
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn search_parses_offer_response() {
        let mut server = Server::new_async().await;
        server
            .mock("POST", "/bundles/")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{
                "offers": [{
                    "id": 12345,
                    "gpu_name": "RTX 4090",
                    "num_gpus": 1,
                    "gpu_ram": 24576.0,
                    "disk_space": 250.0,
                    "dph_total": 0.45,
                    "geolocation": "US",
                    "reliability2": 0.995
                }]
            }"#)
            .create_async()
            .await;

        let provider = VastProvider::with_base_url(server.url());
        let offers = provider.search(&SearchFilters::default()).await.unwrap();

        assert_eq!(offers.len(), 1);
        assert_eq!(offers[0].id, "12345");
        assert_eq!(offers[0].gpu_name, "RTX 4090");
        assert!((offers[0].vram_gb - 24.0).abs() < 0.001);
        assert_eq!(offers[0].disk_gb, 250);
        assert!((offers[0].price_per_hour_usd - 0.45).abs() < 0.001);
    }

    #[tokio::test]
    async fn create_sends_image_and_disk_returns_instance_id() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("PUT", "/asks/55555/")
            .match_body(mockito::Matcher::PartialJson(json!({
                "image": "ubuntu:22.04",
                "disk": 200,
            })))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"success": true, "new_contract": 99887766}"#)
            .create_async()
            .await;

        let provider = VastProvider::with_base_url(server.url());
        let cfg = CreateConfig {
            image: "ubuntu:22.04".into(),
            disk_gb: 200,
            boot_script: None,
        };
        let inst = provider.create("55555", &cfg).await.unwrap();

        assert_eq!(inst.instance_id, "99887766");
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn create_with_boot_script_includes_onstart() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("PUT", "/asks/55555/")
            .match_body(mockito::Matcher::PartialJson(json!({
                "onstart": "echo hello",
            })))
            .with_status(200)
            .with_body(r#"{"success": true, "new_contract": 1}"#)
            .create_async()
            .await;

        let provider = VastProvider::with_base_url(server.url());
        let cfg = CreateConfig {
            image: "ubuntu:22.04".into(),
            disk_gb: 200,
            boot_script: Some("echo hello".into()),
        };
        provider.create("55555", &cfg).await.unwrap();
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn status_extracts_ssh_host_and_port_when_running() {
        let mut server = Server::new_async().await;
        server
            .mock("GET", "/instances/12345/")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{
                "instances": {
                    "actual_status": "running",
                    "ssh_host": "ssh4.vast.ai",
                    "ssh_port": 12345,
                    "dph_total": 0.45
                }
            }"#)
            .create_async()
            .await;

        let provider = VastProvider::with_base_url(server.url());
        let status = provider.status("12345").await.unwrap();

        assert_eq!(status.state, "running");
        assert_eq!(status.ssh_host.as_deref(), Some("ssh4.vast.ai"));
        assert_eq!(status.ssh_port, Some(12345));
    }

    #[tokio::test]
    async fn status_handles_loading_state_with_no_ssh_yet() {
        let mut server = Server::new_async().await;
        server
            .mock("GET", "/instances/12345/")
            .with_status(200)
            .with_body(r#"{
                "instances": {
                    "actual_status": "loading"
                }
            }"#)
            .create_async()
            .await;

        let provider = VastProvider::with_base_url(server.url());
        let status = provider.status("12345").await.unwrap();

        assert_eq!(status.state, "loading");
        assert!(status.ssh_host.is_none());
        assert!(status.ssh_port.is_none());
    }

    #[tokio::test]
    async fn destroy_sends_delete_request() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("DELETE", "/instances/12345/")
            .with_status(200)
            .with_body(r#"{"success": true}"#)
            .create_async()
            .await;

        let provider = VastProvider::with_base_url(server.url());
        provider.destroy("12345").await.unwrap();
        mock.assert_async().await;
    }
}
