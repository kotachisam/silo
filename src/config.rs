use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub vast_api_key: Option<String>,
    #[serde(default)]
    pub search: SearchConfig,
    #[serde(default)]
    pub up: UpConfig,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SearchConfig {
    pub default_gpus: Option<u32>,
    pub default_vram_gb: Option<u32>,
    pub default_disk_gb: Option<u32>,
    pub default_max_price: Option<f32>,
    pub default_region: Option<String>,
    pub default_reliability: Option<f32>,
    pub default_limit: Option<u32>,
    pub default_verified_only: Option<bool>,
    pub default_include_deverified: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct UpConfig {
    pub default_profile: Option<String>,
    #[serde(default)]
    pub profiles: HashMap<String, UpProfile>,
    pub chime_command: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct UpProfile {
    pub image: Option<String>,
    pub disk: Option<u32>,
    pub boot: Option<PathBuf>,
    pub log_path: Option<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default)]
    pub block_arch: Vec<String>,
    pub workload: Option<String>,
    pub ready_probe: Option<String>,
}

pub fn mask_secrets(toml_text: &str) -> String {
    let masked: Vec<String> = toml_text
        .lines()
        .map(|line| {
            if line.trim_start().starts_with('#') {
                return line.to_string();
            }
            let Some(eq_idx) = line.find('=') else {
                return line.to_string();
            };
            let key_part = &line[..eq_idx];
            let key_lower = key_part.to_lowercase();
            if !(key_lower.contains("_token")
                || key_lower.contains("_key")
                || key_lower.contains("_secret"))
            {
                return line.to_string();
            }
            let value_part = line[eq_idx + 1..].trim();
            let unquoted = value_part.trim_matches('"').trim_matches('\'');
            if unquoted.is_empty() {
                return line.to_string();
            }
            let masked_value = if unquoted.chars().count() >= 8 {
                let chars: Vec<char> = unquoted.chars().collect();
                let head: String = chars.iter().take(4).collect();
                let tail: String = chars
                    .iter()
                    .rev()
                    .take(4)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect();
                format!("\"{head}...{tail}\"")
            } else {
                "\"<set>\"".to_string()
            };
            format!("{key_part}= {masked_value}")
        })
        .collect();
    let mut result = masked.join("\n");
    if toml_text.ends_with('\n') && !result.ends_with('\n') {
        result.push('\n');
    }
    result
}

impl Config {
    pub fn default_path() -> Result<PathBuf> {
        let dirs =
            ProjectDirs::from("", "", "silo").context("could not determine config directory")?;
        Ok(dirs.config_dir().join("config.toml"))
    }

    pub fn load_from(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let s = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        let cfg: Self =
            toml::from_str(&s).with_context(|| format!("parsing {}", path.display()))?;
        Ok(cfg)
    }

    pub fn load() -> Result<Self> {
        Self::load_from(&Self::default_path()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn missing_file_returns_default() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let cfg = Config::load_from(&path).unwrap();
        assert_eq!(cfg, Config::default());
    }

    #[test]
    fn parses_vast_api_key() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(&path, r#"vast_api_key = "abc123""#).unwrap();
        let cfg = Config::load_from(&path).unwrap();
        assert_eq!(cfg.vast_api_key.as_deref(), Some("abc123"));
    }

    #[test]
    fn parses_search_section() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(
            &path,
            r#"
vast_api_key = "abc"

[search]
default_vram_gb = 180
default_region = "EU"
default_verified_only = true
"#,
        )
        .unwrap();
        let cfg = Config::load_from(&path).unwrap();
        assert_eq!(cfg.search.default_vram_gb, Some(180));
        assert_eq!(cfg.search.default_region.as_deref(), Some("EU"));
        assert_eq!(cfg.search.default_verified_only, Some(true));
        assert_eq!(cfg.search.default_disk_gb, None);
    }

    #[test]
    fn missing_search_section_yields_default_search_config() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(&path, r#"vast_api_key = "abc""#).unwrap();
        let cfg = Config::load_from(&path).unwrap();
        assert_eq!(cfg.search, SearchConfig::default());
    }

    #[test]
    fn parses_workload_and_ready_probe() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(
            &path,
            r#"
[up.profiles.miner]
image = "ubuntu:22.04"
workload = "mining"
ready_probe = "pgrep ccminer"
"#,
        )
        .unwrap();
        let cfg = Config::load_from(&path).unwrap();
        let p = cfg.up.profiles.get("miner").unwrap();
        assert_eq!(p.workload.as_deref(), Some("mining"));
        assert_eq!(p.ready_probe.as_deref(), Some("pgrep ccminer"));
    }

    #[test]
    fn workload_defaults_to_none_for_legacy_profiles() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(
            &path,
            "[up.profiles.vllm]\nimage = \"vllm/vllm-openai:latest\"\n",
        )
        .unwrap();
        let cfg = Config::load_from(&path).unwrap();
        assert_eq!(cfg.up.profiles.get("vllm").unwrap().workload, None);
    }

    #[test]
    fn parses_up_profiles() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(
            &path,
            r#"
[up]
default_profile = "vllm"

[up.profiles.vllm]
image = "vllm/vllm-openai:latest"
disk = 50

[up.profiles.vllm.env]
MODEL = "Qwen/Qwen2.5-72B-Instruct"
TP_SIZE = "1"

[up.profiles.ollama]
image = "ubuntu:22.04"
disk = 200
"#,
        )
        .unwrap();
        let cfg = Config::load_from(&path).unwrap();
        assert_eq!(cfg.up.default_profile.as_deref(), Some("vllm"));
        let vllm = cfg.up.profiles.get("vllm").unwrap();
        assert_eq!(vllm.image.as_deref(), Some("vllm/vllm-openai:latest"));
        assert_eq!(vllm.disk, Some(50));
        assert_eq!(
            vllm.env.get("MODEL").map(String::as_str),
            Some("Qwen/Qwen2.5-72B-Instruct")
        );
        let ollama = cfg.up.profiles.get("ollama").unwrap();
        assert_eq!(ollama.image.as_deref(), Some("ubuntu:22.04"));
        assert!(ollama.env.is_empty());
    }

    #[test]
    fn mask_secrets_masks_token_and_key_lines() {
        let input = r#"vast_api_key = "deadbeefcafebabe1234567890abcdef0123456789abcdeffedcba9876543210"

[up.profiles.vllm.env]
MODEL = "Qwen/Qwen2.5-72B-Instruct"
HF_TOKEN = "hf_supersecretvalue123456"
"#;
        let masked = mask_secrets(input);
        assert!(masked.contains("\"dead...3210\""));
        assert!(masked.contains("\"hf_s...3456\""));
        assert!(!masked.contains("cafebabe12345678"));
        assert!(!masked.contains("supersecretvalue"));
        assert!(masked.contains(r#"MODEL = "Qwen/Qwen2.5-72B-Instruct""#));
    }

    #[test]
    fn mask_secrets_leaves_comments_alone() {
        let input = "# vast_api_key = \"realsecret\"\nvast_api_key = \"actualkey12345\"\n";
        let masked = mask_secrets(input);
        assert!(masked.contains("# vast_api_key = \"realsecret\""));
        assert!(masked.contains("\"actu...2345\""));
    }

    #[test]
    fn mask_secrets_handles_short_values() {
        let input = "vast_api_key = \"abc\"\n";
        let masked = mask_secrets(input);
        assert!(masked.contains("\"<set>\""));
    }
}
