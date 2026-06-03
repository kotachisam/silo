# silo

_Last updated: 2026-05-07_

Personal CLI for renting GPUs on vast.ai and running inference servers (vLLM,
Ollama) on them. Wraps the spin-up / wait-for-ready / SSH / prompt / save-logs
/ destroy loop with a single binary, profile-based config, and a built-in
hardware-software compatibility check.

Built as a learning artefact (Rust + multi-GPU vLLM + HF model ecosystem) and
as a portable workflow for "rent a GPU, run a real model, capture the logs,
destroy the box" cycles. Vendor-locked to vast.ai today; provider abstraction
is in place for adding others (RunPod et al) when the workflow demands it.

## Install

```bash
cargo install --path .
```

## Quickstart

```bash
# 1. Set up config (one time)
silo config edit
# Set vast_api_key, define an [up.profiles.<name>] block per model you'll run

# 2. Search and rent a GPU offer
silo search --gpus 4 --vram 90 --verified-only -l 5
silo up <offer_id> --wait              # polls until vLLM /health responds; chimes on ready

# 3. Use the model
silo prompt "What is the capital of France?"
silo logs -f                            # live tail of vLLM log
silo status                             # state, SSH details, elapsed cost

# 4. Destroy (logs auto-captured to ~/Library/Application Support/silo/logs/)
silo down
```

## Subcommands

| Command | Purpose |
|---|---|
| `silo search` | List vast.ai offers matching filters; caches results for `silo up` arch checks |
| `silo up <id>` | Rent an offer using the active profile; `--wait` until the workload reports ready, `--profile X` to switch |
| `silo status` | Provider status + SSH details + elapsed time + running cost |
| `silo prompt <text>` | One-shot OpenAI-compatible prompt against the active vLLM instance |
| `silo ssh [-- cmd]` | Interactive shell or one-shot remote command |
| `silo tunnel <port>` | Local port forward to the rented box |
| `silo logs` | Tail/follow/save vLLM log; `--all` for multi-source, `--list` to discover |
| `silo down` | Destroy active instance; auto-captures logs first |
| `silo models` | Browse trending Hugging Face text-generation models |
| `silo config show/edit` | Inspect (with secrets masked) or edit `config.toml` |

## Config

`~/Library/Application Support/silo/config.toml` (macOS path; XDG-equivalent on Linux).

```toml
vast_api_key = "..."

[search]
default_gpus = 1
default_vram_gb = 90
default_verified_only = true
# … and more

[up]
default_profile = "vllm"
chime_command = "afplay /System/Library/Sounds/Glass.aiff"   # runs on `silo up --wait` ready

[up.profiles.qwen-72b]
image = "vllm/vllm-openai:latest"
disk = 50
boot = "/Users/you/bin/vps-vllm-boot.sh"
log_path = "/var/log/vllm.log"
block_arch = []          # optional hard-block list (e.g. ["Blackwell"])

[up.profiles.qwen-72b.env]
MODEL = "Qwen/Qwen2.5-72B-Instruct"
TP_SIZE = "4"
QUANTIZATION = "fp8"
HF_TOKEN = "..."
```

One profile per model. Switch with `silo up <id> --profile <name>`. Run
`silo config edit` to bootstrap; `silo config show` displays it with secrets
masked.

Profiles default to `workload = "inference"`, where `silo up --wait` polls the
vLLM `/health` endpoint to decide readiness. Other workloads set `workload`
explicitly and a `ready_probe` — a remote shell command whose exit-0 means
ready — instead of the health check (e.g. a `mining` profile with
`ready_probe = "pgrep -x ccminer"`). The `silo config edit` template ships a
commented `miner` profile to copy.

## Built-in compatibility check

`silo up` consults a curated table of known-broken model+arch combos before
sending the rental API call. Example: trying `gpt-oss-120b` on a Blackwell GPU
bails out citing the vLLM <0.22 MARLIN MXFP4 backend issue. Override with
`--skip-compat-check` if you want to test anyway.

The table lives in `src/providers/arch.rs::COMPAT_RULES`. Extend as you
encounter new failures. See `.docs/lessons.md` for what's been catalogued.

## State

- `~/Library/Application Support/silo/active.json` — active instance state, last search results
- `~/Library/Application Support/silo/logs/` — auto-captured logs from `silo down`
- `~/Library/Application Support/silo/config.toml` — config

## Docs

- `.docs/session-runbook.md` — end-to-end session walkthrough with failure modes
- `.docs/lessons.md` — catalogued failures and their diagnoses
- `.docs/roadmap.md` — deferred features and known limitations
- `CHANGELOG.md` — feature evolution by date

## License

Personal-use OSS. No warranty — built for one user's workflow, public for
transparency. SkyPilot is the canonical answer for production multi-cloud GPU
orchestration.
