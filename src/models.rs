use anyhow::{Context, Result};
use reqwest::Client;
use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct HfModel {
    pub id: String,
    #[serde(default)]
    pub downloads: u64,
    #[serde(default)]
    pub likes: u64,
    #[serde(rename = "lastModified", default)]
    pub last_modified: Option<String>,
    #[serde(default)]
    pub pipeline_tag: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub safetensors: Option<SafetensorsInfo>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct SafetensorsInfo {
    #[serde(default)]
    pub total: Option<u64>,
}

impl HfModel {
    pub fn params_billions(&self) -> Option<f32> {
        self.safetensors
            .as_ref()
            .and_then(|s| s.total)
            .map(|n| (n as f64 / 1e9) as f32)
    }
}

pub struct HfClient {
    client: Client,
    base_url: String,
}

impl Default for HfClient {
    fn default() -> Self {
        Self::new()
    }
}

impl HfClient {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
            base_url: "https://huggingface.co".to_string(),
        }
    }

    #[cfg(test)]
    pub fn with_base_url(base_url: String) -> Self {
        Self {
            client: Client::new(),
            base_url,
        }
    }

    pub async fn trending_text_generation(
        &self,
        limit: u32,
        search: Option<&str>,
    ) -> Result<Vec<HfModel>> {
        let url = format!("{}/api/models", self.base_url);
        let limit_str = limit.to_string();
        let mut params: Vec<(&str, &str)> = vec![
            ("pipeline_tag", "text-generation"),
            ("sort", "trendingScore"),
            ("direction", "-1"),
            ("limit", &limit_str),
            ("full", "true"),
        ];
        if let Some(s) = search {
            params.push(("search", s));
        }
        let resp = self
            .client
            .get(&url)
            .query(&params)
            .send()
            .await
            .with_context(|| format!("GET {url}"))?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("HF API returned {status}: {body}");
        }
        let models: Vec<HfModel> = resp
            .json()
            .await
            .context("decoding HF /api/models response")?;
        Ok(models)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockito::Server;

    #[test]
    fn params_billions_from_safetensors_total() {
        let m = HfModel {
            id: "x/y".into(),
            downloads: 0,
            likes: 0,
            last_modified: None,
            pipeline_tag: None,
            tags: vec![],
            safetensors: Some(SafetensorsInfo {
                total: Some(72_000_000_000),
            }),
        };
        let p = m.params_billions().unwrap();
        assert!((p - 72.0).abs() < 0.001, "got {p}");
    }

    #[test]
    fn params_billions_returns_none_when_missing() {
        let m = HfModel {
            id: "x/y".into(),
            downloads: 0,
            likes: 0,
            last_modified: None,
            pipeline_tag: None,
            tags: vec![],
            safetensors: None,
        };
        assert!(m.params_billions().is_none());
    }

    #[tokio::test]
    async fn trending_passes_filter_params() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("GET", "/api/models")
            .match_query(mockito::Matcher::AllOf(vec![
                mockito::Matcher::UrlEncoded("pipeline_tag".into(), "text-generation".into()),
                mockito::Matcher::UrlEncoded("sort".into(), "trendingScore".into()),
                mockito::Matcher::UrlEncoded("limit".into(), "5".into()),
            ]))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body("[]")
            .create_async()
            .await;

        let client = HfClient::with_base_url(server.url());
        let models = client.trending_text_generation(5, None).await.unwrap();
        assert!(models.is_empty());
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn trending_parses_response() {
        let mut server = Server::new_async().await;
        server
            .mock("GET", mockito::Matcher::Regex("^/api/models".into()))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"[
                    {
                        "id": "deepseek-ai/DeepSeek-V4-Pro",
                        "downloads": 787000,
                        "likes": 1500,
                        "lastModified": "2026-05-06T10:00:00.000Z",
                        "pipeline_tag": "text-generation",
                        "tags": ["text-generation", "moe"],
                        "safetensors": {"total": 862000000000}
                    },
                    {
                        "id": "ibm-granite/granite-4.1-8b",
                        "downloads": 21800,
                        "likes": 50,
                        "lastModified": "2026-05-04T10:00:00.000Z",
                        "pipeline_tag": "text-generation",
                        "tags": ["text-generation"]
                    }
                ]"#,
            )
            .create_async()
            .await;

        let client = HfClient::with_base_url(server.url());
        let models = client.trending_text_generation(5, None).await.unwrap();
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].id, "deepseek-ai/DeepSeek-V4-Pro");
        let p = models[0].params_billions().unwrap();
        assert!((p - 862.0).abs() < 0.5);
        assert!(models[1].params_billions().is_none());
    }

    #[tokio::test]
    async fn trending_includes_search_when_provided() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("GET", "/api/models")
            .match_query(mockito::Matcher::AnyOf(vec![mockito::Matcher::UrlEncoded(
                "search".into(),
                "coder".into(),
            )]))
            .with_status(200)
            .with_body("[]")
            .create_async()
            .await;

        let client = HfClient::with_base_url(server.url());
        let _ = client.trending_text_generation(5, Some("coder")).await.unwrap();
        mock.assert_async().await;
    }
}
