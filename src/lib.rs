pub mod cli;
pub mod config;
pub mod format;
pub mod models;
pub mod providers;
pub mod ssh;
pub mod state;

use anyhow::{Context, Result};
use chrono::Utc;
use clap::Parser;
use cli::{Cli, Command, ConfigAction, ConfigArgs, ModelsArgs, SearchArgs, TunnelArgs, UpArgs};
use providers::{AnyProvider, CreateConfig, Offer, SearchFilters};
use ssh::SshTarget;
use state::{ActiveInstance, State};
use std::fs;

pub async fn run() -> Result<()> {
    let cli = Cli::parse();

    if let Command::Config(args) = &cli.command {
        return cmd_config(args).await;
    }
    if let Command::Models(args) = &cli.command {
        return cmd_models(args).await;
    }

    let config = config::Config::load()?;
    let state = State::load()?;
    let provider_name = resolve_provider(&cli.provider, &state);
    let provider = AnyProvider::from_name(&provider_name, &config)?;

    match cli.command {
        Command::Search(args) => cmd_search(&provider, &config, args).await,
        Command::Up(args) => cmd_up(&provider, &provider_name, &config, args).await,
        Command::Status => cmd_status(&provider, &provider_name).await,
        Command::Ssh { remote } => cmd_ssh(&provider_name, remote).await,
        Command::Tunnel(args) => cmd_tunnel(&provider_name, args).await,
        Command::Down => cmd_down(&provider, &provider_name).await,
        Command::Cost => cmd_cost(&provider, &provider_name).await,
        Command::Config(_) | Command::Models(_) => unreachable!("handled above"),
    }
}

async fn cmd_models(args: &ModelsArgs) -> Result<()> {
    let client = models::HfClient::new();
    let raw = client
        .trending_text_generation(args.limit, args.search.as_deref())
        .await?;
    let raw_count = raw.len();
    let filtered = filter_models(raw, args.min_params, args.max_params);
    let filtered_out = raw_count - filtered.len();
    format::render_models(&filtered, filtered_out);
    Ok(())
}

fn filter_models(
    raw: Vec<models::HfModel>,
    min_params: Option<f32>,
    max_params: Option<f32>,
) -> Vec<models::HfModel> {
    raw.into_iter()
        .filter(|m| {
            let p = m.params_billions();
            if let Some(min) = min_params
                && p.map(|v| v < min).unwrap_or(true)
            {
                return false;
            }
            if let Some(max) = max_params
                && p.map(|v| v > max).unwrap_or(false)
            {
                return false;
            }
            true
        })
        .collect()
}

fn resolve_provider(flag: &Option<String>, state: &State) -> String {
    flag.clone()
        .or_else(|| state.default_provider.clone())
        .unwrap_or_else(|| "vast".into())
}

async fn cmd_search(provider: &AnyProvider, config: &config::Config, args: SearchArgs) -> Result<()> {
    let s = &config.search;
    let filters = SearchFilters {
        num_gpus: Some(args.gpus.or(s.default_gpus).unwrap_or(1)),
        vram_min_gb: Some(args.vram.or(s.default_vram_gb).unwrap_or(90)),
        disk_min_gb: Some(args.disk.or(s.default_disk_gb).unwrap_or(200)),
        max_price_per_hour_usd: args.max_price.or(s.default_max_price),
        region: Some(
            args.region
                .or_else(|| s.default_region.clone())
                .unwrap_or_else(|| "US".into()),
        ),
        reliability_min: Some(args.reliability.or(s.default_reliability).unwrap_or(0.99)),
        gpu_name: args.gpu_name,
        limit: Some(args.limit.or(s.default_limit).unwrap_or(20)),
    };
    let verified_only = args.verified_only || s.default_verified_only.unwrap_or(false);
    let include_deverified = args.include_deverified || s.default_include_deverified.unwrap_or(false);

    let mut state = State::load()?;
    state.last_search_filters = Some(filters.clone());
    state.last_verified_only = verified_only;
    state.last_include_deverified = include_deverified;
    state.save()?;
    let offers = provider.search(&filters).await?;
    let filtered = filter_by_status(offers, verified_only, include_deverified);
    format::render_offers(&filtered);
    Ok(())
}

async fn cmd_config(args: &ConfigArgs) -> Result<()> {
    match args.action {
        ConfigAction::Show => cmd_config_show(),
        ConfigAction::Edit => cmd_config_edit(),
    }
}

fn cmd_config_show() -> Result<()> {
    let path = config::Config::default_path()?;
    if !path.exists() {
        println!("(no config file at {})", path.display());
        println!("(run `silo config edit` to create one)");
        return Ok(());
    }
    let contents = fs::read_to_string(&path)
        .with_context(|| format!("reading {}", path.display()))?;
    let masked = config::mask_secrets(&contents);
    println!("# {}\n{masked}", path.display());
    Ok(())
}

fn cmd_config_edit() -> Result<()> {
    let path = config::Config::default_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }
    if !path.exists() {
        let template = "\
# silo config — uncomment and edit to apply
# vast_api_key = \"...\"

[search]
# default_gpus = 1
# default_vram_gb = 90
# default_disk_gb = 200
# default_max_price = 1.5
# default_region = \"US\"
# default_reliability = 0.99
# default_limit = 20
# default_verified_only = true
# default_include_deverified = false

[up]
# default_profile = \"vllm\"

# [up.profiles.vllm]
# image = \"vllm/vllm-openai:latest\"
# disk = 50
# boot = \"/Users/you/bin/vps-vllm-boot.sh\"

# [up.profiles.vllm.env]
# MODEL = \"Qwen/Qwen2.5-72B-Instruct\"
# TP_SIZE = \"1\"
# HF_TOKEN = \"...\"

# [up.profiles.ollama]
# image = \"ubuntu:22.04\"
# disk = 200
# boot = \"/Users/you/bin/vps-ollama-boot.sh\"
";
        fs::write(&path, template)
            .with_context(|| format!("writing template to {}", path.display()))?;
    }
    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".into());
    let status = std::process::Command::new(&editor)
        .arg(&path)
        .status()
        .with_context(|| format!("spawning {editor}"))?;
    if !status.success() {
        anyhow::bail!("{editor} exited with {status}");
    }
    Ok(())
}

fn filter_by_status(offers: Vec<Offer>, verified_only: bool, include_deverified: bool) -> Vec<Offer> {
    if !verified_only && include_deverified {
        return offers;
    }
    let allowed: &[&str] = if verified_only {
        &["verified"]
    } else {
        &["verified", "unverified"]
    };
    offers
        .into_iter()
        .filter(|o| {
            o.status
                .as_deref()
                .map(|s| allowed.contains(&s))
                .unwrap_or(false)
        })
        .collect()
}

async fn cmd_up(
    provider: &AnyProvider,
    provider_name: &str,
    config: &config::Config,
    args: UpArgs,
) -> Result<()> {
    let resolved = resolve_up(&config.up, &args)?;

    let boot_script = match &resolved.boot {
        Some(path) => Some(
            fs::read_to_string(path)
                .with_context(|| format!("reading boot script {}", path.display()))?,
        ),
        None => None,
    };

    let cfg = CreateConfig {
        image: resolved.image,
        disk_gb: resolved.disk,
        boot_script,
        env: resolved.env,
    };

    let inst = match provider.create(&args.offer_id, &cfg).await {
        Ok(i) => i,
        Err(e) if is_stale_offer_error(&e) => {
            println!("offer {} is stale (no_such_ask); trying next-cheapest", args.offer_id);
            retry_with_fresh_offer(provider, &args.offer_id, &cfg).await?
        }
        Err(e) => return Err(e),
    };

    println!("created instance {} on {provider_name}", inst.instance_id);

    let mut state = State::load()?;
    state.default_provider = Some(provider_name.to_string());
    state.instances.insert(
        provider_name.to_string(),
        ActiveInstance {
            instance_id: inst.instance_id.clone(),
            ssh_host: None,
            ssh_port: None,
            created_at: Utc::now(),
        },
    );
    state.save()?;
    println!("(run `silo status` to poll until SSH is ready)");
    Ok(())
}

fn is_stale_offer_error(e: &anyhow::Error) -> bool {
    e.to_string().contains("no_such_ask")
}

struct ResolvedUp {
    image: String,
    disk: u32,
    boot: Option<std::path::PathBuf>,
    env: std::collections::HashMap<String, String>,
}

fn resolve_up(up_config: &config::UpConfig, args: &UpArgs) -> Result<ResolvedUp> {
    let profile_name = args
        .profile
        .clone()
        .or_else(|| up_config.default_profile.clone());
    let profile = match &profile_name {
        Some(name) => match up_config.profiles.get(name) {
            Some(p) => p.clone(),
            None => anyhow::bail!(
                "profile '{name}' not found in config (define [up.profiles.{name}] or pass --image/--disk/--boot directly)"
            ),
        },
        None => config::UpProfile::default(),
    };

    let image = args
        .image
        .clone()
        .or(profile.image)
        .unwrap_or_else(|| "ubuntu:22.04".to_string());
    let disk = args.disk.or(profile.disk).unwrap_or(200);
    let boot = args.boot.clone().or(profile.boot);

    let mut env = profile.env.clone();
    for raw in &args.env {
        let (k, v) = raw.split_once('=').ok_or_else(|| {
            anyhow::anyhow!("--env expects KEY=VALUE, got '{raw}'")
        })?;
        env.insert(k.to_string(), v.to_string());
    }

    Ok(ResolvedUp {
        image,
        disk,
        boot,
        env,
    })
}

async fn retry_with_fresh_offer(
    provider: &AnyProvider,
    failed_id: &str,
    cfg: &CreateConfig,
) -> Result<providers::InstanceRef> {
    let state = State::load()?;
    let filters = state
        .last_search_filters
        .ok_or_else(|| anyhow::anyhow!("no saved search filters; run `silo search` first"))?;
    let raw = provider.search(&filters).await?;
    let candidates = filter_by_status(raw, state.last_verified_only, state.last_include_deverified);

    let mut last_err: Option<anyhow::Error> = None;
    for next in candidates.into_iter().filter(|o| o.id != failed_id).take(3) {
        println!("retrying with offer {} ({})", next.id, next.gpu_name);
        match provider.create(&next.id, cfg).await {
            Ok(inst) => return Ok(inst),
            Err(e) if is_stale_offer_error(&e) => {
                println!("  → {} also stale", next.id);
                last_err = Some(e);
                continue;
            }
            Err(e) => return Err(e),
        }
    }
    Err(last_err.unwrap_or_else(|| {
        anyhow::anyhow!(
            "no alternative offers available; vast.ai's bundles cache appears stuck — try `silo search` again in a minute"
        )
    }))
}

async fn cmd_status(provider: &AnyProvider, provider_name: &str) -> Result<()> {
    let mut state = State::load()?;
    let active = state
        .instances
        .get(provider_name)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("no active instance for provider {provider_name}"))?;

    let status = provider.status(&active.instance_id).await?;
    println!("provider:    {provider_name}");
    println!("instance:    {}", active.instance_id);
    println!("state:       {}", status.state);
    if let Some(host) = &status.ssh_host {
        println!("ssh_host:    {host}");
    }
    if let Some(port) = status.ssh_port {
        println!("ssh_port:    {port}");
    }
    if let Some(cost) = status.cost_per_hour_usd {
        println!("usd/hr:      {cost:.4}");
    }

    let updated = ActiveInstance {
        instance_id: active.instance_id.clone(),
        ssh_host: status.ssh_host,
        ssh_port: status.ssh_port,
        created_at: active.created_at,
    };
    state.instances.insert(provider_name.to_string(), updated);
    state.save()?;
    Ok(())
}

async fn cmd_ssh(provider_name: &str, remote: Vec<String>) -> Result<()> {
    let state = State::load()?;
    let active = state
        .instances
        .get(provider_name)
        .ok_or_else(|| anyhow::anyhow!("no active instance for {provider_name}"))?;
    let host = active
        .ssh_host
        .clone()
        .ok_or_else(|| anyhow::anyhow!("ssh_host unknown — run `silo status` first"))?;
    let port = active
        .ssh_port
        .ok_or_else(|| anyhow::anyhow!("ssh_port unknown — run `silo status` first"))?;
    let target = SshTarget::new(host, port);
    target.run_ssh(&remote)
}

async fn cmd_tunnel(provider_name: &str, args: TunnelArgs) -> Result<()> {
    let state = State::load()?;
    let active = state
        .instances
        .get(provider_name)
        .ok_or_else(|| anyhow::anyhow!("no active instance for {provider_name}"))?;
    let host = active
        .ssh_host
        .clone()
        .ok_or_else(|| anyhow::anyhow!("ssh_host unknown — run `silo status` first"))?;
    let port = active
        .ssh_port
        .ok_or_else(|| anyhow::anyhow!("ssh_port unknown — run `silo status` first"))?;
    let remote_port = args.remote_port.unwrap_or(args.port);
    let target = SshTarget::new(host, port);
    println!(
        "tunnel: localhost:{} -> {}:{remote_port} (Ctrl-C to close)",
        args.port, target.host
    );
    target.run_tunnel(args.port, remote_port)
}

async fn cmd_down(provider: &AnyProvider, provider_name: &str) -> Result<()> {
    let mut state = State::load()?;
    let active = state
        .instances
        .remove(provider_name)
        .ok_or_else(|| anyhow::anyhow!("no active instance for {provider_name}"))?;
    provider.destroy(&active.instance_id).await?;
    println!("destroyed {} on {provider_name}", active.instance_id);
    state.save()?;
    Ok(())
}

async fn cmd_cost(provider: &AnyProvider, provider_name: &str) -> Result<()> {
    let state = State::load()?;
    let active = state
        .instances
        .get(provider_name)
        .ok_or_else(|| anyhow::anyhow!("no active instance for {provider_name}"))?;

    let status = provider.status(&active.instance_id).await?;
    let elapsed = Utc::now() - active.created_at;
    let elapsed_hours = elapsed.num_seconds() as f32 / 3600.0;

    println!("instance:    {}", active.instance_id);
    println!("provider:    {provider_name}");
    println!("state:       {}", status.state);
    println!("started:     {}", active.created_at.format("%Y-%m-%d %H:%M:%S UTC"));
    println!("elapsed:     {}", humanize_elapsed(elapsed));
    if let Some(rate) = status.cost_per_hour_usd {
        let total = rate * elapsed_hours;
        println!("rate:        ${rate:.4}/hr");
        println!("running:     ${total:.4}");
    } else {
        println!("rate:        unknown");
    }
    Ok(())
}

fn humanize_elapsed(d: chrono::Duration) -> String {
    let total_secs = d.num_seconds().max(0);
    let hours = total_secs / 3600;
    let mins = (total_secs % 3600) / 60;
    let secs = total_secs % 60;
    if hours > 0 {
        format!("{hours}h {mins}m {secs}s")
    } else if mins > 0 {
        format!("{mins}m {secs}s")
    } else {
        format!("{secs}s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_no_such_ask_error() {
        let e = anyhow::anyhow!("PUT /asks/123/ returned 400: {{\"error\":\"invalid_args\",\"msg\":\"no_such_ask Instance type by id 123 is not available.\"}}");
        assert!(is_stale_offer_error(&e));
    }

    #[test]
    fn does_not_match_unrelated_errors() {
        let e = anyhow::anyhow!("connection refused");
        assert!(!is_stale_offer_error(&e));
    }

    #[test]
    fn humanize_elapsed_formats() {
        use chrono::Duration;
        assert_eq!(humanize_elapsed(Duration::seconds(45)), "45s");
        assert_eq!(humanize_elapsed(Duration::seconds(125)), "2m 5s");
        assert_eq!(humanize_elapsed(Duration::seconds(3725)), "1h 2m 5s");
    }

    #[test]
    fn flag_overrides_state_default() {
        let state = State {
            default_provider: Some("runpod".into()),
            ..Default::default()
        };
        let resolved = resolve_provider(&Some("vast".into()), &state);
        assert_eq!(resolved, "vast");
    }

    #[test]
    fn falls_back_to_state_default() {
        let state = State {
            default_provider: Some("runpod".into()),
            ..Default::default()
        };
        let resolved = resolve_provider(&None, &state);
        assert_eq!(resolved, "runpod");
    }

    #[test]
    fn ultimate_fallback_is_vast() {
        let resolved = resolve_provider(&None, &State::default());
        assert_eq!(resolved, "vast");
    }

    fn offer(id: &str, status: Option<&str>) -> Offer {
        Offer {
            id: id.into(),
            status: status.map(String::from),
            ..Default::default()
        }
    }

    #[test]
    fn filter_default_excludes_deverified() {
        let offers = vec![
            offer("a", Some("verified")),
            offer("b", Some("unverified")),
            offer("c", Some("deverified")),
        ];
        let result = filter_by_status(offers, false, false);
        let ids: Vec<_> = result.iter().map(|o| o.id.as_str()).collect();
        assert_eq!(ids, vec!["a", "b"]);
    }

    #[test]
    fn filter_verified_only_keeps_only_verified() {
        let offers = vec![
            offer("a", Some("verified")),
            offer("b", Some("unverified")),
            offer("c", Some("deverified")),
        ];
        let result = filter_by_status(offers, true, false);
        let ids: Vec<_> = result.iter().map(|o| o.id.as_str()).collect();
        assert_eq!(ids, vec!["a"]);
    }

    #[test]
    fn filter_include_deverified_keeps_everything() {
        let offers = vec![
            offer("a", Some("verified")),
            offer("b", Some("unverified")),
            offer("c", Some("deverified")),
        ];
        let result = filter_by_status(offers, false, true);
        let ids: Vec<_> = result.iter().map(|o| o.id.as_str()).collect();
        assert_eq!(ids, vec!["a", "b", "c"]);
    }

    #[test]
    fn filter_verified_only_wins_over_include_deverified() {
        let offers = vec![
            offer("a", Some("verified")),
            offer("b", Some("unverified")),
            offer("c", Some("deverified")),
        ];
        let result = filter_by_status(offers, true, true);
        let ids: Vec<_> = result.iter().map(|o| o.id.as_str()).collect();
        assert_eq!(ids, vec!["a"]);
    }

    #[test]
    fn filter_drops_offers_with_unknown_status() {
        let offers = vec![offer("a", None), offer("b", Some("verified"))];
        let result = filter_by_status(offers, false, false);
        let ids: Vec<_> = result.iter().map(|o| o.id.as_str()).collect();
        assert_eq!(ids, vec!["b"]);
    }

    fn model(id: &str, params_billions: Option<u64>) -> models::HfModel {
        models::HfModel {
            id: id.into(),
            downloads: 0,
            likes: 0,
            last_modified: None,
            pipeline_tag: None,
            tags: vec![],
            safetensors: params_billions.map(|b| models::SafetensorsInfo {
                total: Some(b * 1_000_000_000),
            }),
        }
    }

    #[test]
    fn filter_models_min_params_drops_smaller_and_unknown() {
        let raw = vec![
            model("big", Some(120)),
            model("medium", Some(70)),
            model("small", Some(8)),
            model("unknown", None),
        ];
        let result = filter_models(raw, Some(70.0), None);
        let ids: Vec<_> = result.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids, vec!["big", "medium"]);
    }

    #[test]
    fn filter_models_max_params_drops_larger() {
        let raw = vec![
            model("big", Some(120)),
            model("medium", Some(70)),
            model("small", Some(8)),
            model("unknown", None),
        ];
        let result = filter_models(raw, None, Some(70.0));
        let ids: Vec<_> = result.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids, vec!["medium", "small", "unknown"]);
    }

    #[test]
    fn filter_models_no_filter_keeps_everything() {
        let raw = vec![model("a", Some(70)), model("b", None)];
        let result = filter_models(raw, None, None);
        assert_eq!(result.len(), 2);
    }
}
