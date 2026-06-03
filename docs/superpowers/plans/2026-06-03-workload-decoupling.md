# Workload Decoupling Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Decouple silo's `up` flow from its baked-in vLLM/inference assumptions, so the same orchestration spine can run inference, mining, or future workloads, selected per-profile.

**Architecture:** Introduce a `Workload` trait (mirroring the existing `Provider` trait) that owns the three inference-coupled behaviours — readiness, pre-flight compatibility, and (later) interaction. `AnyWorkload` is a thin enum dispatcher. Inference is workload #1 (current behaviour, unchanged). Mining is workload #2, with readiness driven by a generic `ready_probe` remote command so most future workloads need a profile line, not Rust.

**Tech Stack:** Rust, anyhow, serde, clap, existing `SshTarget` + `providers::arch`.

---

## Design decision flagged for review

**`ready_probe` as the generic readiness mechanism.** A workload is "ready" when a remote command exits 0. Inference defaults this to the vLLM `/health` curl; mining sets its own; an unforeseen workload sets a profile line and needs no Rust impl. If you'd rather every workload be an explicit Rust impl, say so and Task 2's `MiningWorkload` changes from "generic probe" to "hardcoded miner check."

## Scope

This plan covers **only** the workload axis. Multi-provider/arbitrage (the `Offer`-model surgery) is explicitly out of scope — a separate plan. No behaviour change for existing inference configs is a hard requirement: every current `config.toml` must keep working untouched (the `workload` field defaults to `"inference"`).

## File Structure

- Create: `src/workloads/mod.rs` — `Workload` trait, `AnyWorkload` enum, `from_resolved` constructor
- Create: `src/workloads/inference.rs` — `InferenceWorkload` (health-curl readiness + arch preflight)
- Create: `src/workloads/mining.rs` — `MiningWorkload` (probe readiness + no-op preflight)
- Modify: `src/lib.rs:1-7` — add `pub mod workloads;`
- Modify: `src/config.rs:39-49` — add `workload` + `ready_probe` to `UpProfile`
- Modify: `src/lib.rs:697-744` — `ResolvedUp` carries `workload` + `ready_probe`; `resolve_up` populates them
- Modify: `src/lib.rs:501-546` — `poll_until_vllm_ready` → `poll_until_ready`, readiness delegated to workload
- Modify: `src/lib.rs:615-691` — `cmd_up` builds `AnyWorkload`, calls `preflight` then `poll_until_ready`
- Delete: `check_arch_compat` from `src/lib.rs:556-613` (logic moves into `inference.rs`)

---

## Task 1: Profile gains `workload` + `ready_probe` fields

**Files:**
- Modify: `src/config.rs:39-49`
- Test: `src/config.rs` (tests module)

- [ ] **Step 1: Write the failing test**

In `src/config.rs` tests module:

```rust
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
    fs::write(&path, "[up.profiles.vllm]\nimage = \"vllm/vllm-openai:latest\"\n").unwrap();
    let cfg = Config::load_from(&path).unwrap();
    assert_eq!(cfg.up.profiles.get("vllm").unwrap().workload, None);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib config::tests::parses_workload_and_ready_probe`
Expected: FAIL — `no field 'workload' on UpProfile`

- [ ] **Step 3: Add the fields**

In `src/config.rs`, `UpProfile`:

```rust
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
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib config::tests`
Expected: PASS (all config tests, including the two new ones)

- [ ] **Step 5: Commit**

```bash
git add src/config.rs
git commit -m "feat(config): add workload + ready_probe profile fields"
```

---

## Task 2: `Workload` trait, dispatch, and the two impls

**Files:**
- Create: `src/workloads/mod.rs`
- Create: `src/workloads/inference.rs`
- Create: `src/workloads/mining.rs`
- Modify: `src/lib.rs:1-7`
- Test: in `src/workloads/inference.rs` and `src/workloads/mod.rs`

- [ ] **Step 1: Register the module**

In `src/lib.rs`, add after `pub mod state;`:

```rust
pub mod workloads;
```

- [ ] **Step 2: Write `src/workloads/mod.rs`**

```rust
pub mod inference;
pub mod mining;

use crate::ssh::SshTarget;
use crate::state::State;
use anyhow::Result;

pub trait Workload {
    fn name(&self) -> &'static str;
    fn preflight(&self, state: &State, offer_id: &str, skip: bool) -> Result<()>;
    fn is_ready(&self, target: &SshTarget) -> bool;
}

pub enum AnyWorkload {
    Inference(inference::InferenceWorkload),
    Mining(mining::MiningWorkload),
}

pub struct WorkloadInputs<'a> {
    pub block_arch: &'a [String],
    pub model_id: Option<String>,
    pub ready_probe: Option<String>,
}

impl AnyWorkload {
    pub fn from_name(name: &str, inputs: WorkloadInputs) -> Result<Self> {
        match name {
            "inference" => Ok(Self::Inference(inference::InferenceWorkload {
                block_arch: inputs.block_arch.to_vec(),
                model_id: inputs.model_id,
            })),
            "mining" => Ok(Self::Mining(mining::MiningWorkload {
                ready_probe: inputs.ready_probe,
            })),
            other => anyhow::bail!(
                "unknown workload '{other}' (known: inference, mining); set [up.profiles.<n>].workload"
            ),
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Inference(w) => w.name(),
            Self::Mining(w) => w.name(),
        }
    }

    pub fn preflight(&self, state: &State, offer_id: &str, skip: bool) -> Result<()> {
        match self {
            Self::Inference(w) => w.preflight(state, offer_id, skip),
            Self::Mining(w) => w.preflight(state, offer_id, skip),
        }
    }

    pub fn is_ready(&self, target: &SshTarget) -> bool {
        match self {
            Self::Inference(w) => w.is_ready(target),
            Self::Mining(w) => w.is_ready(target),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_name_defaults_and_known() {
        let infer = AnyWorkload::from_name(
            "inference",
            WorkloadInputs { block_arch: &[], model_id: None, ready_probe: None },
        )
        .unwrap();
        assert_eq!(infer.name(), "inference");

        let mining = AnyWorkload::from_name(
            "mining",
            WorkloadInputs { block_arch: &[], model_id: None, ready_probe: Some("true".into()) },
        )
        .unwrap();
        assert_eq!(mining.name(), "mining");
    }

    #[test]
    fn from_name_rejects_unknown() {
        let err = AnyWorkload::from_name(
            "quantum",
            WorkloadInputs { block_arch: &[], model_id: None, ready_probe: None },
        );
        assert!(err.is_err());
    }
}
```

- [ ] **Step 3: Write `src/workloads/inference.rs` (preflight = moved arch logic)**

```rust
use super::Workload;
use crate::providers::arch;
use crate::ssh::SshTarget;
use crate::state::State;
use anyhow::Result;

pub struct InferenceWorkload {
    pub block_arch: Vec<String>,
    pub model_id: Option<String>,
}

impl Workload for InferenceWorkload {
    fn name(&self) -> &'static str {
        "inference"
    }

    fn preflight(&self, state: &State, offer_id: &str, skip: bool) -> Result<()> {
        if skip {
            return Ok(());
        }
        let Some(offer) = state.last_search_results.get(offer_id) else {
            if !self.block_arch.is_empty() || self.model_id.is_some() {
                eprintln!(
                    "(compat check skipped: offer {offer_id} not in last search cache — run `silo search` first to enable)"
                );
            }
            return Ok(());
        };
        let Some(arch) = arch::arch_for(&offer.gpu_name) else {
            eprintln!(
                "(compat check skipped: unknown architecture for {} — extend providers::arch::arch_for)",
                offer.gpu_name
            );
            return Ok(());
        };

        if self.block_arch.iter().any(|b| b.eq_ignore_ascii_case(arch)) {
            anyhow::bail!(
                "profile blocks '{}', but offer {} is {} ({}). Override with --skip-compat-check, edit profile.block_arch, or pick a different offer.",
                arch, offer_id, offer.gpu_name, arch
            );
        }

        if let Some(model) = self.model_id.as_deref() {
            match arch::compat_check(model, arch) {
                arch::Compat::Ok => {}
                arch::Compat::Unstable(note) => {
                    eprintln!("warning: model '{model}' on {} ({}): {note}", offer.gpu_name, arch);
                    eprintln!("(proceeding; pass --skip-compat-check to silence)");
                }
                arch::Compat::Broken(note) => {
                    anyhow::bail!(
                        "known-bad combo: model '{model}' on {} ({}): {note}\n(override with --skip-compat-check if you want to test anyway)",
                        offer.gpu_name, arch
                    );
                }
            }
        }
        Ok(())
    }

    fn is_ready(&self, target: &SshTarget) -> bool {
        let cmd = vec![
            "curl".into(),
            "-sf".into(),
            "-o".into(),
            "/dev/null".into(),
            "http://localhost:8000/health".into(),
        ];
        target.run_ssh_with_stdin(&cmd, &[]).is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{CachedOffer, State};

    fn state_with_offer(id: &str, gpu: &str) -> State {
        let mut s = State::default();
        s.last_search_results.insert(
            id.into(),
            CachedOffer { gpu_name: gpu.into(), num_gpus: 1, vram_gb: 90.0 },
        );
        s
    }

    #[test]
    fn preflight_bails_on_blocked_arch() {
        let w = InferenceWorkload { block_arch: vec!["Blackwell".into()], model_id: None };
        let s = state_with_offer("42", "RTX PRO 6000");
        assert!(w.preflight(&s, "42", false).is_err());
    }

    #[test]
    fn preflight_skip_short_circuits() {
        let w = InferenceWorkload { block_arch: vec!["Blackwell".into()], model_id: None };
        let s = state_with_offer("42", "RTX PRO 6000");
        assert!(w.preflight(&s, "42", true).is_ok());
    }

    #[test]
    fn preflight_ok_when_offer_uncached() {
        let w = InferenceWorkload { block_arch: vec![], model_id: None };
        assert!(w.preflight(&State::default(), "999", false).is_ok());
    }
}
```

> Note: `arch_for("RTX PRO 6000")` must resolve to `Blackwell` for the first test. If `providers::arch::arch_for` doesn't map that string, pick a GPU name from `arch.rs`'s own tests that resolves to a blockable arch, and adjust the literal.

- [ ] **Step 4: Write `src/workloads/mining.rs`**

```rust
use super::Workload;
use crate::ssh::SshTarget;
use crate::state::State;
use anyhow::Result;

pub struct MiningWorkload {
    pub ready_probe: Option<String>,
}

impl Workload for MiningWorkload {
    fn name(&self) -> &'static str {
        "mining"
    }

    fn preflight(&self, _state: &State, _offer_id: &str, _skip: bool) -> Result<()> {
        Ok(())
    }

    fn is_ready(&self, target: &SshTarget) -> bool {
        let probe = self.ready_probe.as_deref().unwrap_or("true");
        let cmd = vec!["sh".into(), "-c".into(), probe.to_string()];
        target.run_ssh_with_stdin(&cmd, &[]).is_ok()
    }
}
```

- [ ] **Step 5: Run tests**

Run: `cargo test --lib workloads`
Expected: PASS (dispatch + inference preflight tests)

- [ ] **Step 6: Commit**

```bash
git add src/workloads/ src/lib.rs
git commit -m "feat(workloads): Workload trait + inference/mining impls"
```

---

## Task 3: Wire `cmd_up` to use the workload (HIGH-risk path — manual checklist)

**Files:**
- Modify: `src/lib.rs:697-744` (`ResolvedUp` + `resolve_up`)
- Modify: `src/lib.rs:615-691` (`cmd_up`)
- Modify: `src/lib.rs:501-546` (`poll_until_vllm_ready` → `poll_until_ready`)
- Delete: `src/lib.rs:556-613` (`check_arch_compat`)

> This is the money-burning path (a broken readiness loop = silent "never ready" = you pay rental). It is SSH/network-bound and **has no unit coverage** — verified by manual checklist in Step 6, not by `cargo test`.

- [ ] **Step 1: Extend `ResolvedUp` and `resolve_up`**

Add two fields to `ResolvedUp`:

```rust
struct ResolvedUp {
    image: String,
    disk: u32,
    boot: Option<std::path::PathBuf>,
    env: std::collections::HashMap<String, String>,
    block_arch: Vec<String>,
    workload: String,
    ready_probe: Option<String>,
}
```

At the end of `resolve_up`, before the `Ok(ResolvedUp { ... })`:

```rust
    let workload = profile.workload.clone().unwrap_or_else(|| "inference".to_string());
    let ready_probe = profile.ready_probe.clone();
```

And add `workload,` and `ready_probe,` to the returned struct literal.

- [ ] **Step 2: Replace `check_arch_compat` call + delete the fn**

In `cmd_up`, replace the `check_arch_compat(...)` block (lib.rs:622-630) with workload construction + preflight:

```rust
    let resolved = resolve_up(&config.up, &args)?;
    let state_pre = State::load()?;
    let model_id = resolved.env.get("MODEL").cloned();
    let workload = workloads::AnyWorkload::from_name(
        &resolved.workload,
        workloads::WorkloadInputs {
            block_arch: &resolved.block_arch,
            model_id,
            ready_probe: resolved.ready_probe.clone(),
        },
    )?;
    workload.preflight(&state_pre, &args.offer_id, args.skip_compat_check)?;
```

Then delete the whole `check_arch_compat` function (lib.rs:556-613).

- [ ] **Step 3: Generalise the readiness loop**

Rename `poll_until_vllm_ready` to `poll_until_ready` and thread the workload through. Replace the hardcoded curl block (lib.rs:524-536) with the workload call:

```rust
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
```

- [ ] **Step 4: Update the `cmd_up` wait call**

Replace lib.rs:679-680:

```rust
    println!("waiting for {} readiness (polls every 60s, 30m timeout)…", workload.name());
    let final_status = poll_until_ready(provider, &inst.instance_id, &workload).await?;
```

Update `UpArgs.wait` help text in `src/cli.rs:282` from `"Block until vLLM /health responds..."` to `"Block until the workload reports ready (polls every 60s, 30m timeout)"`.

- [ ] **Step 5: Verify it compiles and existing tests pass**

Run: `cargo test`
Expected: PASS — all existing tests green, no `check_arch_compat` references remain.
Run: `cargo clippy --all-targets -- -D warnings`
Expected: clean (watch for an unused-import warning if `arch` is no longer referenced in `lib.rs`).

- [ ] **Step 6: Manual test checklist (the part `cargo test` cannot cover)**

Inference regression (must behave exactly as before):

```bash
silo search --gpus 1 -l 3
silo up <offer_id> --profile qwen-72b --wait
# EXPECT: polls, prints "[Nm Ns] inference ready" on vLLM health, chimes
silo prompt "test" && silo down
```

- [ ] Inference `--wait` still reaches ready and chimes
- [ ] `--skip-compat-check` still bypasses preflight
- [ ] A blocked-arch offer still bails before creating the instance
- [ ] Mining smoke (no real miner needed): a profile with `workload = "mining"` and `ready_probe = "test -f /tmp/ready"` reports not-ready, then ready once you `silo ssh -- touch /tmp/ready`
- [ ] `silo down` still auto-captures logs and destroys

- [ ] **Step 7: Commit**

```bash
git add src/lib.rs src/cli.rs
git commit -m "refactor(up): drive readiness + preflight through Workload abstraction"
```

---

## Task 4: Config template + docs + verify trio

**Files:**
- Modify: `src/lib.rs:267-303` (config template in `cmd_config_edit`)
- Modify: `README.md` (profile docs, if it enumerates profile keys)

- [ ] **Step 1: Add a mining profile to the commented template**

In `cmd_config_edit`'s template string, after the ollama profile block:

```rust
# [up.profiles.miner]
# image = \"ubuntu:22.04\"
# disk = 50
# boot = \"/Users/you/bin/vps-miner-boot.sh\"
# workload = \"mining\"
# ready_probe = \"pgrep -x ccminer\"   # exit-0 means ready; omit to treat SSH-up as ready
# log_path = \"/var/log/miner.log\"
```

- [ ] **Step 2: Run the verify trio**

Run: `cargo fmt --all -- --check`
Expected: clean (no diff)
Run: `cargo clippy --all-targets -- -D warnings`
Expected: clean
Run: `cargo test`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add src/lib.rs README.md
git commit -m "docs(config): document mining workload profile"
```

---

## Self-Review Notes

- **Spec coverage:** workload-first ✅ (Tasks 1–3); open/extensible without Rust per-workload ✅ (`ready_probe`); interaction verb deferred ✅ (out of scope, `silo prompt` untouched and still works for inference); no behaviour change for legacy configs ✅ (`workload` defaults to `"inference"`).
- **HIGH-risk path** (`poll_until_ready`) has no automated coverage by nature — Task 3 Step 6 is the gate.
- **Out of scope, tracked for next plan:** multi-provider `Offer` normalisation, cross-provider offer identity, `silo prompt` becoming workload-aware, mining interaction verbs (`silo hashrate`/`stats`).
- **Backlog — readiness probe blocks on SSH host-key prompt:** `is_ready`'s SSH call hits an interactive `Are you sure you want to continue connecting (yes/no)?` on first contact with a host, which stalls an unattended `--wait` indefinitely while billing. Fix: pass `-o StrictHostKeyChecking=accept-new` (or `BatchMode=yes`) in the SSH invocation so probes never block. Pre-existing (not introduced by this plan); own small fix.
- **Backlog — autonomous `arch.rs` compat sourcing:** keep the curated `compat_check` table authoritative (only it may return `Broken`/hard-block); add an optional enrichment layer (LLM/web-search) that fires only on a cache-miss combo and is capped at `Unstable` (advisory, never auto-greenlights or hard-blocks a paid launch). Automation feeds curation, doesn't replace it.

## Money-safety findings from 2026-06-03 live test (next plan)

Surfaced renting a real box; each one cost or risked real money. Ordered by value.

1. **Preflight: reject `TP_SIZE > offer.num_gpus`.** A `qwen-72b` profile (`TP_SIZE=4`) launched on a 1-GPU offer; vLLM crashed instantly with `World size (4) is larger than the number of available GPUs (1)`, then the readiness loop would have polled the corpse for 30 min while billing. silo had both facts at preflight (`offer.num_gpus`, `profile.env["TP_SIZE"]`). Add the check to `InferenceWorkload::preflight` — bail before creating the instance. Highest value; would have prevented the entire wasted run.
2. **Readiness: detect a dead process, don't poll a corpse.** `poll_until_ready` can't distinguish "still loading" from "crashed and never coming." Add a liveness signal — vllm PID alive, or log tail contains a fatal `Traceback` — so `--wait` fails fast instead of billing to the 30-min timeout.
3. **`down`: stop the meter before saving logs.** `cmd_down` runs `try_auto_capture_logs` (SSH `cat /var/log/*.log`) *before* `provider.destroy()`, with no timeout. A slow/hung SSH blocks the destroy that stops billing. Reorder destroy-first, or cap log-capture with a hard timeout. Also relates to the host-key-prompt backlog item (same SSH-blocks-money path).
4. **`down`: clear local state on `no_such_instance`/404.** Repro: kill an instance externally (dashboard), then `silo down` → `provider.destroy()` returns 404 `no_such_instance` → the `?` aborts `cmd_down` before `state.save()` → the instance lingers in `active.json` forever and `silo status` shows a ghost (`state: unknown`). Fix: add `is_missing_instance_error` (mirror `is_stale_offer_error`); when destroy returns it, log and proceed to `state.save()` — the instance being gone *is* the goal. Cleanest fix of the four.
- **Type consistency:** `WorkloadInputs`, `AnyWorkload::from_name`, `is_ready(&SshTarget) -> bool`, `preflight(&State, &str, bool) -> Result<()>` used identically in mod.rs, inference.rs, mining.rs, and the `cmd_up` call site.
