use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CachedOffer {
    pub gpu_name: String,
    pub num_gpus: u32,
    pub vram_gb: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActiveInstance {
    pub instance_id: String,
    pub ssh_host: Option<String>,
    pub ssh_port: Option<u16>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct State {
    #[serde(default)]
    pub default_provider: Option<String>,
    #[serde(default)]
    pub instances: HashMap<String, ActiveInstance>,
    #[serde(default)]
    pub last_search_filters: Option<crate::providers::SearchFilters>,
    #[serde(default)]
    pub last_verified_only: bool,
    #[serde(default)]
    pub last_include_deverified: bool,
    #[serde(default)]
    pub last_search_results: HashMap<String, CachedOffer>,
}

impl State {
    pub fn default_path() -> Result<PathBuf> {
        let dirs =
            ProjectDirs::from("", "", "silo").context("could not determine state directory")?;
        Ok(dirs.data_local_dir().join("active.json"))
    }

    pub fn load_from(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let bytes = fs::read(path).with_context(|| format!("reading {}", path.display()))?;
        let state: Self = serde_json::from_slice(&bytes)
            .with_context(|| format!("parsing {}", path.display()))?;
        Ok(state)
    }

    pub fn save_to(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
        }
        let bytes = serde_json::to_vec_pretty(self)?;
        fs::write(path, bytes).with_context(|| format!("writing {}", path.display()))?;
        Ok(())
    }

    pub fn load() -> Result<Self> {
        Self::load_from(&Self::default_path()?)
    }

    pub fn save(&self) -> Result<()> {
        self.save_to(&Self::default_path()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use tempfile::tempdir;

    fn sample_instance() -> ActiveInstance {
        ActiveInstance {
            instance_id: "12345678".into(),
            ssh_host: Some("ssh4.vast.ai".into()),
            ssh_port: Some(12345),
            created_at: Utc.with_ymd_and_hms(2026, 5, 6, 10, 0, 0).unwrap(),
        }
    }

    #[test]
    fn load_returns_default_when_missing() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("active.json");
        let state = State::load_from(&path).unwrap();
        assert_eq!(state, State::default());
    }

    #[test]
    fn save_then_load_round_trips() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("active.json");
        let mut state = State {
            default_provider: Some("vast".into()),
            ..Default::default()
        };
        state.instances.insert("vast".into(), sample_instance());
        state.save_to(&path).unwrap();

        let loaded = State::load_from(&path).unwrap();
        assert_eq!(loaded, state);
    }

    #[test]
    fn save_creates_parent_dirs() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("nested/sub/active.json");
        let state = State::default();
        state.save_to(&path).unwrap();
        assert!(path.exists());
    }

    #[test]
    fn supports_multiple_providers_simultaneously() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("active.json");
        let mut state = State::default();
        state.instances.insert("vast".into(), sample_instance());
        state.instances.insert(
            "runpod".into(),
            ActiveInstance {
                instance_id: "abc".into(),
                ssh_host: None,
                ssh_port: None,
                created_at: Utc.with_ymd_and_hms(2026, 5, 6, 11, 0, 0).unwrap(),
            },
        );
        state.save_to(&path).unwrap();

        let loaded = State::load_from(&path).unwrap();
        assert_eq!(loaded.instances.len(), 2);
    }
}
