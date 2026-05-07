# Changelog

Format: date-ordered, descending. Personal-tool conventions; not semver.

## 2026-05-07

### Added

- `silo prompt <text>` — one-shot OpenAI-compatible prompt against active vLLM
  instance. SSHes in, curls localhost:8000, returns the message content (or
  full JSON with `--json`). Resolves model from active profile's `MODEL` env.
- `silo up --wait` — blocks until vLLM `/health` responds. Polls every 60s
  with 30m timeout. Chimes via `[up].chime_command` on ready.
- `silo logs` — tail/follow/save the active instance's vLLM log without manual
  SSH. `--all` for multi-source `tail -F /var/log/*.log`, `--list` to enumerate
  available logs, `--save` for full local capture, `--path` to override
  remote path.
- `silo logs --save` and **auto-capture on `silo down`** — destruction now
  fetches the log file to `~/Library/Application Support/silo/logs/` before
  the destroy API call. Survives the box reclaim.
- Built-in model+arch **compatibility check** at `silo up`. Curated rules
  table in `providers/arch.rs::COMPAT_RULES` flags known-bad combos
  (currently: gpt-oss + Blackwell, fp8 + Ampere). Bails on `Broken`, warns
  on `Unstable`, silently allows `Ok`. `--skip-compat-check` overrides.
- `UpProfile.block_arch` — user-defined hard-block list, runs before the
  built-in table for combos silo's curation hasn't reached yet.
- `UpProfile.log_path` — per-profile remote log path (default
  `/var/log/vllm.log`); used by `silo logs`.
- `UpConfig.chime_command` — shell command run on `silo up --wait` ready.
- Last search results cached in state — `silo up` looks up the offer's GPU
  model name to apply arch checks.

### Changed

- `silo status` and `silo cost` merged into a single `silo status`. Cost
  fields (started, elapsed, rate, running) now show alongside SSH details.
- `silo config show` now warns if the file doesn't parse cleanly (catches
  duplicate-section TOML errors before the next unrelated command fails).
- `silo config edit` validates the file post-edit; returns non-zero on parse
  errors so the user knows to re-edit.
- `silo config show` masks any field whose name contains `_token`/`_key`/
  `_secret` (first 4 + last 4 chars).
- vast.ai env field now sent as JSON object `{KEY: "value"}` not Docker-flag
  string `-e KEY=val` (was rejected with "env must be a dict").
- `silo search` server-side filters on `rentable: true, rented: false` so the
  cache-vs-availability mismatch that caused mass `no_such_ask` errors is
  resolved. Was the actual fix; client-side retry from earlier this session
  was treating a symptom.

### Fixed

- Reqwest 0.13's `query()` requires the `query` cargo feature (was implicit
  in 0.12). Added.
- vast.ai's `/bundles/` endpoint omits safetensors metadata even with
  `full=true`; `silo models` now enriches via per-model `/api/models/{id}`
  calls in parallel via `tokio::JoinSet`.
- `params_billions()` falls back to model-name parsing when safetensors
  metadata is missing — covers GGUF and quantized variants where the API
  doesn't surface params.
- `silo up` no longer prompts duplicate retry attempts when the search cache
  has stable offers.

### Documentation

- `.docs/lessons.md` (new) — catalogued failure modes with diagnoses.
  First entry: gpt-oss-120b on RTX PRO 6000 S (Blackwell) crashed in MARLIN
  MXFP4 MoE backend during weight load on vLLM 0.20.1. ~$2 to learn.
- `.docs/session-runbook.md` (updated) — full end-to-end runbook with
  `silo up --wait` and auto-capture flow.
- `.docs/smoke-test.md` (updated) — reflects current command surface.

## 2026-05-06

### Added

- v1 command surface: `silo search / up / status / ssh / tunnel / down`.
- Provider abstraction (`Provider` trait + `AnyProvider` enum) with vast.ai
  implementation. State persisted as JSON.
- `silo models` — Hugging Face trending text-generation browser. Sort by
  trending/downloads/likes/modified/created. Filter by `--min`/`--max`
  params (in billions) and `--min-downloads`. Enriches missing params from
  per-model HF endpoints.
- `silo config show/edit` — config inspection and `$EDITOR`-driven edit.
- `silo cost` — running cost tracking. Later merged into `silo status`.
- vastai-style 3-block search output (perf/infra/country) with column
  units, humanized Max_Days, full host status (verified/unverified/
  deverified). Tightened to fit half-screen 16" MBP terminals.
- `--verified-only` and `--include-deverified` filters; default excludes
  deverified hosts.
- Auto-retry on `no_such_ask` errors (later superseded by server-side
  rentable filter).

### Initial release

- Cargo project scaffolded as `silo`. 17 tests covering state, vast HTTP
  client, ssh subprocess wrappers, CLI dispatch.
