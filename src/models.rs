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
        if let Some(t) = self.safetensors.as_ref().and_then(|s| s.total) {
            return Some((t as f64 / 1e9) as f32);
        }
        params_from_name(&self.id)
    }
}

pub(crate) fn params_from_name(name: &str) -> Option<f32> {
    let bytes = name.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if !bytes[i].is_ascii_digit() {
            i += 1;
            continue;
        }
        let start = i;
        while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
            i += 1;
        }
        let num_end = i;
        if num_end < bytes.len() && (bytes[num_end] == b'B' || bytes[num_end] == b'b') {
            let after_b = num_end + 1;
            let boundary_ok = after_b == bytes.len()
                || matches!(bytes[after_b], b'-' | b'_' | b'.' | b'/');
            if boundary_ok {
                let raw = std::str::from_utf8(&bytes[start..num_end]).ok()?;
                if let Ok(n) = raw.parse::<f32>()
                    && (0.05..=10_000.0).contains(&n)
                {
                    return Some(n);
                }
            }
            i = after_b;
        }
    }
    None
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

    pub async fn fetch_detail(&self, id: &str) -> Result<HfModel> {
        let url = format!("{}/api/models/{}", self.base_url, id);
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .with_context(|| format!("GET {url}"))?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("HF API returned {status}: {body}");
        }
        resp.json().await.context("decoding /api/models/{id}")
    }

    pub async fn enrich_missing_params(&self, mut models: Vec<HfModel>) -> Vec<HfModel> {
        let mut set: tokio::task::JoinSet<(usize, Result<HfModel>)> =
            tokio::task::JoinSet::new();

        for (i, m) in models.iter().enumerate() {
            if m.params_billions().is_some() {
                continue;
            }
            let id = m.id.clone();
            let base_url = self.base_url.clone();
            let client = self.client.clone();
            set.spawn(async move {
                let url = format!("{}/api/models/{}", base_url, id);
                let result = async {
                    let resp = client
                        .get(&url)
                        .send()
                        .await
                        .with_context(|| format!("GET {url}"))?;
                    if !resp.status().is_success() {
                        anyhow::bail!("HF API returned {}", resp.status());
                    }
                    resp.json::<HfModel>()
                        .await
                        .context("decoding /api/models/{id}")
                }
                .await;
                (i, result)
            });
        }

        while let Some(joined) = set.join_next().await {
            if let Ok((i, Ok(detail))) = joined
                && let Some(s) = detail.safetensors
                && s.total.is_some()
            {
                models[i].safetensors = Some(s);
            }
        }

        models
    }

    pub async fn list_text_generation(
        &self,
        limit: u32,
        search: Option<&str>,
        sort_field: &str,
    ) -> Result<Vec<HfModel>> {
        let url = format!("{}/api/models", self.base_url);
        let limit_str = limit.to_string();
        let mut params: Vec<(&str, &str)> = vec![
            ("pipeline_tag", "text-generation"),
            ("sort", sort_field),
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
    fn params_billions_falls_back_to_name_when_safetensors_missing() {
        let m = HfModel {
            id: "Qwen/Qwen2.5-Coder-32B-Instruct".into(),
            downloads: 0,
            likes: 0,
            last_modified: None,
            pipeline_tag: None,
            tags: vec![],
            safetensors: None,
        };
        assert!((m.params_billions().unwrap() - 32.0).abs() < 0.01);
    }

    #[test]
    fn params_billions_returns_none_when_no_size_anywhere() {
        let m = HfModel {
            id: "Qwen/Qwen3-Coder-Next".into(),
            downloads: 0,
            likes: 0,
            last_modified: None,
            pipeline_tag: None,
            tags: vec![],
            safetensors: None,
        };
        assert!(m.params_billions().is_none());
    }

    #[test]
    fn params_from_name_extracts_first_b_match() {
        assert_eq!(
            params_from_name("Qwen/Qwen2.5-Coder-32B-Instruct"),
            Some(32.0)
        );
        assert_eq!(
            params_from_name("unsloth/Qwen3-Coder-30B-A3B-Instruct-GGUF"),
            Some(30.0)
        );
        assert_eq!(
            params_from_name("bigatuna/Qwen3.5-9b-Sushi-Coder-RL-GGUF"),
            Some(9.0)
        );
        assert_eq!(
            params_from_name("meta-llama/Llama-3.3-70B-Instruct"),
            Some(70.0)
        );
        assert_eq!(
            params_from_name("HuggingFaceTB/nanowhale-100m"),
            None,
            "no B/b suffix should yield None"
        );
        assert_eq!(
            params_from_name("Qwen/Qwen3-Coder-Next"),
            None,
            "no size token at all"
        );
    }

    #[test]
    fn params_from_name_decimal_sizes() {
        assert_eq!(
            params_from_name("microsoft/phi-1.5B-instruct"),
            Some(1.5)
        );
    }

    #[test]
    fn params_from_name_rejects_implausibly_large() {
        assert_eq!(
            params_from_name("some/Model-99999999B-thing"),
            None
        );
    }

    #[test]
    fn safetensors_wins_over_name_heuristic() {
        let m = HfModel {
            id: "Qwen/Qwen2.5-Coder-7B".into(),
            downloads: 0,
            likes: 0,
            last_modified: None,
            pipeline_tag: None,
            tags: vec![],
            safetensors: Some(SafetensorsInfo {
                total: Some(7_240_000_000),
            }),
        };
        let p = m.params_billions().unwrap();
        assert!((p - 7.24).abs() < 0.05);
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
        let models = client.list_text_generation(5, None, "trendingScore").await.unwrap();
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
        let models = client.list_text_generation(5, None, "trendingScore").await.unwrap();
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].id, "deepseek-ai/DeepSeek-V4-Pro");
        let p0 = models[0].params_billions().unwrap();
        assert!((p0 - 862.0).abs() < 0.5);
        let p1 = models[1].params_billions().unwrap();
        assert!((p1 - 8.0).abs() < 0.01, "name-fallback should extract 8B from granite-4.1-8b");
    }

    #[tokio::test]
    async fn enrich_skips_models_with_known_params() {
        let server = Server::new_async().await;
        let client = HfClient::with_base_url(server.url());
        let models = vec![
            HfModel {
                id: "x/already-known-7b".into(),
                downloads: 0,
                likes: 0,
                last_modified: None,
                pipeline_tag: None,
                tags: vec![],
                safetensors: Some(SafetensorsInfo {
                    total: Some(7_000_000_000),
                }),
            },
            HfModel {
                id: "y/Qwen2.5-72B-Instruct".into(),
                downloads: 0,
                likes: 0,
                last_modified: None,
                pipeline_tag: None,
                tags: vec![],
                safetensors: None,
            },
        ];
        let result = client.enrich_missing_params(models).await;
        assert_eq!(result.len(), 2);
        assert!((result[0].params_billions().unwrap() - 7.0).abs() < 0.01);
        assert!((result[1].params_billions().unwrap() - 72.0).abs() < 0.01);
    }

    #[tokio::test]
    async fn enrich_fetches_detail_for_unknown_params() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("GET", "/api/models/deepseek-ai/DeepSeek-V4-Pro")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
                    "id": "deepseek-ai/DeepSeek-V4-Pro",
                    "downloads": 0,
                    "likes": 0,
                    "tags": [],
                    "safetensors": {
                        "parameters": {"FP8": 862000000000},
                        "total": 862000000000
                    }
                }"#,
            )
            .create_async()
            .await;

        let client = HfClient::with_base_url(server.url());
        let models = vec![HfModel {
            id: "deepseek-ai/DeepSeek-V4-Pro".into(),
            downloads: 0,
            likes: 0,
            last_modified: None,
            pipeline_tag: None,
            tags: vec![],
            safetensors: None,
        }];
        let result = client.enrich_missing_params(models).await;
        assert_eq!(result.len(), 1);
        let p = result[0].params_billions().unwrap();
        assert!((p - 862.0).abs() < 0.5);
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn enrich_passes_through_when_detail_fails() {
        let mut server = Server::new_async().await;
        server
            .mock("GET", "/api/models/missing/model")
            .with_status(404)
            .with_body("not found")
            .create_async()
            .await;

        let client = HfClient::with_base_url(server.url());
        let models = vec![HfModel {
            id: "missing/model".into(),
            downloads: 0,
            likes: 0,
            last_modified: None,
            pipeline_tag: None,
            tags: vec![],
            safetensors: None,
        }];
        let result = client.enrich_missing_params(models).await;
        assert_eq!(result.len(), 1);
        assert!(result[0].params_billions().is_none());
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
        let _ = client.list_text_generation(5, Some("coder"), "trendingScore").await.unwrap();
        mock.assert_async().await;
    }
}
