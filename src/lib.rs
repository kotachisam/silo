pub mod cli;
pub mod config;
pub mod format;
pub mod models;
pub mod providers;
pub mod ssh;
pub mod state;
pub mod workloads;

use anyhow::{Context, Result};
use chrono::Utc;
use clap::Parser;
use cli::{
    Cli, Command, ConfigAction, ConfigArgs, LogsArgs, ModelsArgs, PromptArgs, SearchArgs,
    TunnelArgs, UpArgs,
};
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
        Command::Down => cmd_down(&provider, &provider_name, &config).await,
        Command::Prompt(args) => cmd_prompt(&provider_name, &config, args).await,
        Command::Logs(args) => cmd_logs(&provider_name, &config, args).await,
        Command::Config(_) | Command::Models(_) => unreachable!("handled above"),
    }
}

fn resolve_model_from_config(config: &config::Config) -> Option<String> {
    let profile_name = config.up.default_profile.as_deref()?;
    let profile = config.up.profiles.get(profile_name)?;
    profile.env.get("MODEL").cloned()
}

async fn cmd_prompt(provider_name: &str, config: &config::Config, args: PromptArgs) -> Result<()> {
    let state = State::load()?;
    let active = state.instances.get(provider_name).ok_or_else(|| {
        anyhow::anyhow!("no active instance for {provider_name}; run `silo up <id>` first")
    })?;
    let host = active
        .ssh_host
        .clone()
        .ok_or_else(|| anyhow::anyhow!("ssh_host unknown — run `silo status` first"))?;
    let port = active
        .ssh_port
        .ok_or_else(|| anyhow::anyhow!("ssh_port unknown — run `silo status` first"))?;

    let model = args
        .model
        .clone()
        .or_else(|| resolve_model_from_config(config))
        .ok_or_else(|| {
            anyhow::anyhow!(
                "no model specified — pass --model or set [up.profiles.<name>.env].MODEL in config"
            )
        })?;

    let mut messages = Vec::new();
    if let Some(sys) = &args.system {
        messages.push(serde_json::json!({"role": "system", "content": sys}));
    }
    messages.push(serde_json::json!({"role": "user", "content": args.prompt}));

    let body = serde_json::json!({
        "model": model,
        "messages": messages,
        "max_tokens": args.max_tokens,
    });
    let body_bytes = serde_json::to_vec(&body)?;

    let target = SshTarget::new(host, port);
    let remote_cmd: Vec<String> = vec![
        "curl".into(),
        "-s".into(),
        "http://127.0.0.1:8000/v1/chat/completions".into(),
        "-H".into(),
        "Content-Type: application/json".into(),
        "-d".into(),
        "@-".into(),
    ];

    let stdout = target.run_ssh_with_stdin(&remote_cmd, &body_bytes)?;

    if args.json {
        println!("{stdout}");
        return Ok(());
    }

    let parsed: serde_json::Value = serde_json::from_str(&stdout).with_context(|| {
        format!(
            "decoding API response (vLLM may not be ready yet — try `silo ssh -- 'curl -sf http://127.0.0.1:8000/health'`):\n{stdout}"
        )
    })?;

    if let Some(content) = parsed
        .pointer("/choices/0/message/content")
        .and_then(|v| v.as_str())
    {
        println!("{content}");
    } else if let Some(error) = parsed.get("error") {
        anyhow::bail!("API returned error: {error}");
    } else {
        println!("{stdout}");
    }
    Ok(())
}

async fn cmd_models(args: &ModelsArgs) -> Result<()> {
    let client = models::HfClient::new();
    let raw = client
        .list_text_generation(args.limit, args.search.as_deref(), args.sort.to_hf_field())
        .await?;
    let enriched = client.enrich_missing_params(raw).await;
    let total = enriched.len();
    let filtered = filter_models(
        enriched,
        args.min_params,
        args.max_params,
        args.min_downloads,
    );
    let filtered_out = total - filtered.len();
    format::render_models(&filtered, filtered_out);
    Ok(())
}

fn filter_models(
    raw: Vec<models::HfModel>,
    min_params: Option<f32>,
    max_params: Option<f32>,
    min_downloads: Option<u64>,
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
            if let Some(min_dl) = min_downloads
                && m.downloads < min_dl
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

async fn cmd_search(
    provider: &AnyProvider,
    config: &config::Config,
    args: SearchArgs,
) -> Result<()> {
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
    let include_deverified =
        args.include_deverified || s.default_include_deverified.unwrap_or(false);

    let mut state = State::load()?;
    state.last_search_filters = Some(filters.clone());
    state.last_verified_only = verified_only;
    state.last_include_deverified = include_deverified;
    let offers = provider.search(&filters).await?;
    let filtered = filter_by_status(offers, verified_only, include_deverified);

    state.last_search_results = filtered
        .iter()
        .map(|o| {
            (
                o.id.clone(),
                state::CachedOffer {
                    gpu_name: o.gpu_name.clone(),
                    num_gpus: o.num_gpus,
                    vram_gb: o.vram_gb,
                },
            )
        })
        .collect();
    state.save()?;

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
    let contents =
        fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let masked = config::mask_secrets(&contents);
    println!("# {}\n{masked}", path.display());

    match config::Config::load_from(&path) {
        Err(e) => {
            eprintln!();
            eprintln!(
                "warning: config does not parse cleanly — silo will fail on every command except `silo config show/edit` until fixed:"
            );
            eprintln!("  {e:#}");
            eprintln!("(run `silo config edit` to fix)");
        }
        Ok(cfg) => {
            if let Some(issue) = cfg.default_profile_issue() {
                eprintln!();
                eprintln!("warning: {issue}");
            }
        }
    }
    Ok(())
}

fn cmd_config_edit() -> Result<()> {
    let path = config::Config::default_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
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
# chime_command = \"afplay /System/Library/Sounds/Glass.aiff\"   # macOS; runs on `silo up --wait` success

# [up.profiles.vllm]
# image = \"vllm/vllm-openai:latest\"
# disk = 50
# boot = \"/Users/you/bin/vps-vllm-boot.sh\"
# log_path = \"/var/log/vllm.log\"   # used by `silo logs`
# block_arch = [\"Blackwell\"]   # bail out of `silo up` if offer is on a blocked GPU arch

# [up.profiles.vllm.env]
# MODEL = \"Qwen/Qwen2.5-72B-Instruct\"
# TP_SIZE = \"1\"
# HF_TOKEN = \"...\"

# [up.profiles.ollama]
# image = \"ubuntu:22.04\"
# disk = 200
# boot = \"/Users/you/bin/vps-ollama-boot.sh\"
# log_path = \"/var/log/ollama.log\"

# [up.profiles.miner]
# image = \"ubuntu:22.04\"
# disk = 50
# boot = \"/Users/you/bin/vps-miner-boot.sh\"
# workload = \"mining\"   # default is \"inference\"; sets how `silo up --wait` decides ready
# ready_probe = \"pgrep -x ccminer\"   # exit-0 means ready; omit to treat SSH-up as ready
# log_path = \"/var/log/miner.log\"
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

    if let Err(e) = config::Config::load_from(&path) {
        eprintln!();
        eprintln!("warning: edited config does not parse cleanly — silo will fail until fixed:");
        eprintln!("  {e:#}");
        eprintln!("(re-run `silo config edit` to fix; original file is unchanged on disk)");
        return Err(e);
    }
    Ok(())
}

fn filter_by_status(
    offers: Vec<Offer>,
    verified_only: bool,
    include_deverified: bool,
) -> Vec<Offer> {
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

fn profile_log_path(config: &config::Config) -> Option<String> {
    config
        .up
        .default_profile
        .as_deref()
        .and_then(|name| config.up.profiles.get(name))
        .and_then(|p| p.log_path.clone())
}

fn list_remote_logs(target: &SshTarget) -> Result<Vec<String>> {
    let cmd = vec![
        "sh".into(),
        "-c".into(),
        "ls -1t /var/log/*.log 2>/dev/null".into(),
    ];
    let stdout = target.run_ssh_with_stdin(&cmd, &[])?;
    Ok(stdout
        .lines()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect())
}

fn discover_log_path(target: &SshTarget) -> Result<String> {
    let logs = list_remote_logs(target)?;
    match logs.len() {
        0 => anyhow::bail!(
            "no .log files found in /var/log/ on the instance. Pass --path or set log_path in profile config."
        ),
        1 => {
            eprintln!("(auto-detected log: {})", logs[0]);
            Ok(logs[0].clone())
        }
        _ => {
            eprintln!("multiple logs available — pick one with --path:");
            for log in &logs {
                eprintln!("  {log}");
            }
            anyhow::bail!(
                "ambiguous; specify --path or set log_path in [up.profiles.<name>] config"
            )
        }
    }
}

async fn cmd_logs(provider_name: &str, config: &config::Config, args: LogsArgs) -> Result<()> {
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

    if args.list {
        let logs = list_remote_logs(&target)?;
        if logs.is_empty() {
            println!("(no .log files found in /var/log/)");
        } else {
            println!("available logs (most recent first):");
            for log in logs {
                println!("  {log}");
            }
        }
        return Ok(());
    }

    if args.all {
        let n = args.tail;
        let follow_flag = if args.follow { "-F" } else { "" };
        if let Some(save) = args.save {
            let local_path = save_path_for(&save, provider_name, &active.instance_id);
            let remote_cmd = vec!["sh".into(), "-c".into(), "cat /var/log/*.log".into()];
            target.run_ssh_to_file(&remote_cmd, &local_path)?;
            println!("saved to {}", local_path.display());
            return Ok(());
        }
        let remote_cmd = vec![
            "sh".into(),
            "-c".into(),
            format!("tail -n {n} {follow_flag} /var/log/*.log"),
        ];
        return target.run_ssh(&remote_cmd);
    }

    let log_path = if let Some(p) = args.path.as_deref() {
        p.to_string()
    } else if let Some(p) = profile_log_path(config) {
        p
    } else {
        discover_log_path(&target)?
    };

    if let Some(save) = args.save {
        let local_path = save_path_for(&save, provider_name, &active.instance_id);
        let remote_cmd = vec!["cat".into(), log_path];
        target.run_ssh_to_file(&remote_cmd, &local_path)?;
        println!("saved to {}", local_path.display());
        return Ok(());
    }

    let n = args.tail.to_string();
    let remote_cmd: Vec<String> = if args.follow {
        vec!["tail".into(), "-n".into(), n, "-f".into(), log_path]
    } else {
        vec!["tail".into(), "-n".into(), n, log_path]
    };
    target.run_ssh(&remote_cmd)
}

fn save_path_for(save_arg: &str, provider_name: &str, instance_id: &str) -> std::path::PathBuf {
    if save_arg.is_empty() {
        let ts = Utc::now().format("%Y%m%dT%H%M%SZ");
        std::path::PathBuf::from(format!("silo-{provider_name}-{instance_id}-{ts}.log"))
    } else {
        std::path::PathBuf::from(save_arg)
    }
}

fn auto_capture_logs_dir() -> Result<std::path::PathBuf> {
    let dirs = directories::ProjectDirs::from("", "", "silo")
        .ok_or_else(|| anyhow::anyhow!("could not determine state directory"))?;
    let logs_dir = dirs.data_local_dir().join("logs");
    std::fs::create_dir_all(&logs_dir)
        .with_context(|| format!("creating {}", logs_dir.display()))?;
    Ok(logs_dir)
}

fn try_auto_capture_logs(
    host: String,
    port: u16,
    provider_name: &str,
    instance_id: &str,
    config: &config::Config,
) -> Result<std::path::PathBuf> {
    let target = SshTarget::new(host, port);
    let log_path = profile_log_path(config).unwrap_or_else(|| "/var/log/*.log".to_string());
    let logs_dir = auto_capture_logs_dir()?;
    let ts = Utc::now().format("%Y%m%dT%H%M%SZ");
    let local_path = logs_dir.join(format!("silo-{provider_name}-{instance_id}-{ts}.log"));
    let cmd = vec!["sh".into(), "-c".into(), format!("cat {log_path}")];
    target.run_ssh_to_file(&cmd, &local_path)?;
    Ok(local_path)
}

async fn poll_until_ready(
    provider: &AnyProvider,
    instance_id: &str,
    workload: &workloads::AnyWorkload,
) -> Result<providers::InstanceStatus> {
    let timeout = std::time::Duration::from_secs(1800);
    let interval = std::time::Duration::from_secs(60);
    let start = std::time::Instant::now();

    loop {
        if start.elapsed() > timeout {
            anyhow::bail!(
                "timed out after 30 minutes waiting for {} readiness; instance still running. Check `silo status` and `silo down` if you want to stop billing",
                workload.name()
            );
        }

        let elapsed = chrono::Duration::from_std(start.elapsed()).unwrap_or_default();
        let elapsed_str = humanize_elapsed(elapsed);

        match provider.status(instance_id).await {
            Ok(s) => {
                if s.state == "running"
                    && let (Some(host), Some(port)) = (s.ssh_host.clone(), s.ssh_port)
                {
                    let target = SshTarget::new(host, port);
                    if workload.is_ready(&target) {
                        println!("[{elapsed_str}] {} ready", workload.name());
                        return Ok(s);
                    }
                    println!("[{elapsed_str}] running, {} not yet ready", workload.name());
                } else {
                    println!("[{elapsed_str}] state={}", s.state);
                }
            }
            Err(e) => println!("[{elapsed_str}] status error: {e}"),
        }

        tokio::time::sleep(interval).await;
    }
}

fn run_chime(config: &config::Config) {
    if let Some(cmd) = &config.up.chime_command
        && !cmd.trim().is_empty()
    {
        let _ = std::process::Command::new("sh").arg("-c").arg(cmd).status();
    }
}

async fn cmd_up(
    provider: &AnyProvider,
    provider_name: &str,
    config: &config::Config,
    args: UpArgs,
) -> Result<()> {
    let resolved = resolve_up(&config.up, &args)?;
    let state_pre = State::load()?;
    let model_id = resolved.env.get("MODEL").cloned();
    let tp_size = resolved
        .env
        .get("TP_SIZE")
        .and_then(|s| s.parse::<u32>().ok());
    let workload = workloads::AnyWorkload::from_name(
        &resolved.workload,
        workloads::WorkloadInputs {
            block_arch: &resolved.block_arch,
            model_id,
            tp_size,
            ready_probe: resolved.ready_probe.clone(),
        },
    )?;
    workload.preflight(&state_pre, &args.offer_id, args.skip_compat_check)?;

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
            println!(
                "offer {} is stale (no_such_ask); trying next-cheapest",
                args.offer_id
            );
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

    if !args.wait {
        println!("(run `silo status` to poll until SSH is ready, or pass --wait next time)");
        return Ok(());
    }

    println!(
        "waiting for {} readiness (polls every 60s, 30m timeout)…",
        workload.name()
    );
    let final_status = poll_until_ready(provider, &inst.instance_id, &workload).await?;

    let mut state = State::load()?;
    if let Some(active) = state.instances.get_mut(provider_name) {
        active.ssh_host = final_status.ssh_host;
        active.ssh_port = final_status.ssh_port;
    }
    state.save()?;

    run_chime(config);
    Ok(())
}

fn is_stale_offer_error(e: &anyhow::Error) -> bool {
    e.to_string().contains("no_such_ask")
}

fn is_missing_instance_error(e: &anyhow::Error) -> bool {
    e.to_string().contains("no_such_instance")
}

struct ResolvedUp {
    image: String,
    disk: u32,
    boot: Option<std::path::PathBuf>,
    env: std::collections::HashMap<String, String>,
    block_arch: Vec<String>,
    workload: String,
    ready_probe: Option<String>,
}

fn resolve_up(up_config: &config::UpConfig, args: &UpArgs) -> Result<ResolvedUp> {
    let profile_name = args
        .profile
        .clone()
        .or_else(|| up_config.default_profile.clone());
    let has_inline = args.image.is_some() || args.disk.is_some() || args.boot.is_some();
    let profile = match &profile_name {
        Some(name) => match up_config.profiles.get(name) {
            Some(p) => p.clone(),
            None if has_inline => {
                eprintln!("(profile '{name}' not found; using inline --image/--disk/--boot)");
                config::UpProfile::default()
            }
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
    let block_arch = profile.block_arch.clone();
    let workload = profile
        .workload
        .clone()
        .unwrap_or_else(|| "inference".to_string());
    let ready_probe = profile.ready_probe.clone();

    let mut env = profile.env.clone();
    for raw in &args.env {
        let (k, v) = raw
            .split_once('=')
            .ok_or_else(|| anyhow::anyhow!("--env expects KEY=VALUE, got '{raw}'"))?;
        env.insert(k.to_string(), v.to_string());
    }

    Ok(ResolvedUp {
        image,
        disk,
        boot,
        env,
        block_arch,
        workload,
        ready_probe,
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
    let elapsed = Utc::now() - active.created_at;
    let elapsed_hours = elapsed.num_seconds() as f32 / 3600.0;

    println!("provider:    {provider_name}");
    println!("instance:    {}", active.instance_id);
    println!("state:       {}", status.state);
    if let Some(host) = &status.ssh_host {
        println!("ssh_host:    {host}");
    }
    if let Some(port) = status.ssh_port {
        println!("ssh_port:    {port}");
    }
    println!(
        "started:     {}",
        active.created_at.format("%Y-%m-%d %H:%M:%S UTC")
    );
    println!("elapsed:     {}", humanize_elapsed(elapsed));
    if let Some(rate) = status.cost_per_hour_usd {
        let total = rate * elapsed_hours;
        println!("rate:        ${rate:.4}/hr");
        println!("running:     ${total:.4}");
    } else {
        println!("rate:        unknown");
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

async fn cmd_down(
    provider: &AnyProvider,
    provider_name: &str,
    config: &config::Config,
) -> Result<()> {
    let mut state = State::load()?;
    let active = state
        .instances
        .remove(provider_name)
        .ok_or_else(|| anyhow::anyhow!("no active instance for {provider_name}"))?;

    if let (Some(host), Some(port)) = (active.ssh_host.clone(), active.ssh_port) {
        match try_auto_capture_logs(host, port, provider_name, &active.instance_id, config) {
            Ok(path) => println!("captured logs → {}", path.display()),
            Err(e) => eprintln!("(log capture failed, continuing destroy: {e})"),
        }
    }

    match provider.destroy(&active.instance_id).await {
        Ok(()) => println!("destroyed {} on {provider_name}", active.instance_id),
        Err(e) if is_missing_instance_error(&e) => {
            println!(
                "instance {} already gone on {provider_name}; clearing local state",
                active.instance_id
            );
        }
        Err(e) => return Err(e),
    }
    state.save()?;
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
        let e = anyhow::anyhow!(
            "PUT /asks/123/ returned 400: {{\"error\":\"invalid_args\",\"msg\":\"no_such_ask Instance type by id 123 is not available.\"}}"
        );
        assert!(is_stale_offer_error(&e));
    }

    #[test]
    fn does_not_match_unrelated_errors() {
        let e = anyhow::anyhow!("connection refused");
        assert!(!is_stale_offer_error(&e));
    }

    #[test]
    fn detects_no_such_instance_error() {
        let e = anyhow::anyhow!(
            "DELETE https://console.vast.ai/api/v0/instances/39270787/ returned 404 Not Found: {{\"success\": false, \"error\": \"no_such_instance\", \"msg\": \"Instance 39270787 not found.\"}}"
        );
        assert!(is_missing_instance_error(&e));
    }

    #[test]
    fn missing_instance_does_not_match_unrelated() {
        let e = anyhow::anyhow!("connection refused");
        assert!(!is_missing_instance_error(&e));
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

    fn up_args(profile: Option<&str>, image: Option<&str>) -> UpArgs {
        UpArgs {
            offer_id: "1".into(),
            profile: profile.map(String::from),
            image: image.map(String::from),
            disk: image.map(|_| 30),
            boot: None,
            env: vec![],
            wait: false,
            skip_compat_check: false,
        }
    }

    #[test]
    fn resolve_up_falls_back_to_default_when_profile_missing_but_inline_given() {
        let up = config::UpConfig {
            default_profile: Some("ghost".into()),
            ..Default::default()
        };
        let resolved = resolve_up(&up, &up_args(None, Some("ubuntu:22.04"))).unwrap();
        assert_eq!(resolved.image, "ubuntu:22.04");
    }

    #[test]
    fn resolve_up_bails_when_profile_missing_and_no_inline() {
        let up = config::UpConfig {
            default_profile: Some("ghost".into()),
            ..Default::default()
        };
        assert!(resolve_up(&up, &up_args(None, None)).is_err());
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
        let result = filter_models(raw, Some(70.0), None, None);
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
        let result = filter_models(raw, None, Some(70.0), None);
        let ids: Vec<_> = result.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids, vec!["medium", "small", "unknown"]);
    }

    #[test]
    fn filter_models_no_filter_keeps_everything() {
        let raw = vec![model("a", Some(70)), model("b", None)];
        let result = filter_models(raw, None, None, None);
        assert_eq!(result.len(), 2);
    }

    fn model_with_downloads(id: &str, downloads: u64) -> models::HfModel {
        models::HfModel {
            id: id.into(),
            downloads,
            likes: 0,
            last_modified: None,
            pipeline_tag: None,
            tags: vec![],
            safetensors: None,
        }
    }

    #[test]
    fn profile_log_path_returns_path_when_configured() {
        let profile = config::UpProfile {
            log_path: Some("/var/log/custom.log".into()),
            ..Default::default()
        };
        let mut profiles = std::collections::HashMap::new();
        profiles.insert("vllm".into(), profile);
        let cfg = config::Config {
            vast_api_key: None,
            search: config::SearchConfig::default(),
            up: config::UpConfig {
                default_profile: Some("vllm".into()),
                profiles,
                chime_command: None,
            },
        };
        assert_eq!(profile_log_path(&cfg), Some("/var/log/custom.log".into()));
    }

    #[test]
    fn profile_log_path_returns_none_when_unconfigured() {
        let cfg = config::Config::default();
        assert_eq!(profile_log_path(&cfg), None);
    }

    #[test]
    fn resolve_model_from_config_uses_default_profile() {
        let mut profile = config::UpProfile::default();
        profile
            .env
            .insert("MODEL".into(), "openai/gpt-oss-120b".into());
        let mut profiles = std::collections::HashMap::new();
        profiles.insert("vllm".into(), profile);
        let cfg = config::Config {
            vast_api_key: None,
            search: config::SearchConfig::default(),
            up: config::UpConfig {
                default_profile: Some("vllm".into()),
                profiles,
                chime_command: None,
            },
        };
        assert_eq!(
            resolve_model_from_config(&cfg),
            Some("openai/gpt-oss-120b".into())
        );
    }

    #[test]
    fn resolve_model_from_config_returns_none_when_unset() {
        let cfg = config::Config::default();
        assert_eq!(resolve_model_from_config(&cfg), None);
    }

    #[test]
    fn resolve_model_from_config_returns_none_when_profile_missing() {
        let cfg = config::Config {
            vast_api_key: None,
            search: config::SearchConfig::default(),
            up: config::UpConfig {
                default_profile: Some("vllm".into()),
                profiles: std::collections::HashMap::new(),
                chime_command: None,
            },
        };
        assert_eq!(resolve_model_from_config(&cfg), None);
    }

    #[test]
    fn filter_models_min_downloads_drops_amateur_reuploads() {
        let raw = vec![
            model_with_downloads("real/model", 50_000),
            model_with_downloads("Pskumar91/DeepSeek-V4-Pro", 0),
            model_with_downloads("kiseokshforg/gpt-oss-120b", 0),
            model_with_downloads("legit/quant", 5_800),
        ];
        let result = filter_models(raw, None, None, Some(1000));
        let ids: Vec<_> = result.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids, vec!["real/model", "legit/quant"]);
    }
}
