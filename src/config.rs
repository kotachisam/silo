use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub vast_api_key: Option<String>,
    #[serde(default)]
    pub search: SearchConfig,
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

impl Config {
    pub fn default_path() -> Result<PathBuf> {
        let dirs = ProjectDirs::from("", "", "silo")
            .context("could not determine config directory")?;
        Ok(dirs.config_dir().join("config.toml"))
    }

    pub fn load_from(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let s = fs::read_to_string(path)
            .with_context(|| format!("reading {}", path.display()))?;
        let cfg: Self = toml::from_str(&s)
            .with_context(|| format!("parsing {}", path.display()))?;
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
}
