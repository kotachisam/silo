# silo Multi-Provider Architecture (SkyPilot-modeled, foundation-first)

## Context

silo is a single-user Rust CLI for renting ephemeral GPUs, today vast.ai-only. The goal is to make it **provider-agnostic** — search and rent across vast + Clore + RunPod, with cross-provider price/value ranking — by borrowing SkyPilot's proven patterns at personal-tool weight.

**Strategic frame (decided):** *Foundation-first.* Build the provider abstraction (normalized catalog + cost-optimizer + per-provider adaptors) as a **single-user CLI** — immediately useful for the user's own renting, and the exact foundation a future product would need — **without** the multi-tenant/SaaS rebuild (auth, billing, per-tenant secrets, web). The productise-or-not decision stays open and downstream.

**Decisions (approved 2026-06-03):** foundation-first scope; neutral-catalog offer model; integrate **Clore AND RunPod both at once** (marketplace vs on-demand — pressure-tests the abstraction).

**Why now:** the `Provider` trait boundary is already clean, but three things block multi-provider: `Offer` is ~half vast-specific, `format.rs`'s table is hardcoded to vast fields, and **an offer doesn't carry its provider** — so `silo up <id>` assumes one provider.

**Patterns borrowed from SkyPilot** (stripped of enterprise weight): **Catalog** → one normalized `Offer` schema; **Optimizer** → greedy sort-by-price; **Resources** → provider-agnostic GPU request; **Cloud adaptor** → the `Provider` trait (exists). Refs: skypilot-catalog repo, NodeProvider base class, the optimizer.

**Landscape (researched):** RunPod = most mature REST API (`rest.runpod.io/v1/pods`), on-demand pod model, mining historically restricted. Clore.ai = P2P marketplace like vast (on-demand/interruptible/reserved), cheapest (1.6% take), mining-native, Python SDK to mirror. Both have full create/SSH/destroy APIs.

## Architecture

### 1. Neutral catalog `Offer` (the crux) — `src/providers/mod.rs`
Split the vast-shaped `Offer` into provider-neutral core + provider-specific blob:

```rust
pub enum ProviderId { Vast, Clore, RunPod }

pub struct Offer {
    pub provider: ProviderId,          // NEW — fixes up-routing
    pub id: String,
    pub gpu_raw: String,               // provider raw name
    pub gpu_canonical: Option<String>, // normalized via gpu::canonical() (Phase C)
    pub num_gpus: u32, pub vram_gb: f32, pub price_per_hour_usd: f32,
    pub region: Option<String>, pub reliability: Option<f32>, pub disk_gb: u32,
    pub cpu_ghz: Option<f32>, pub vcpus: Option<f32>, pub ram_gb: Option<f32>,
    pub net_up_mbps: Option<f32>, pub net_down_mbps: Option<f32>,
    pub extra: ProviderExtra,
}

pub enum ProviderExtra {
    Vast(VastExtra),  // dlp, dlp_per_dollar, score, machine_id, host_id, status, ports, driver, cuda, max_days
    Clore(CloreExtra), RunPod(RunPodExtra),
}
```
`verified`/`deverified` filtering moves into `VastExtra` → a vast-scoped filter, not global. NOTE the tricky bit: `filter_by_status` (lib.rs) and the `offer()` test helper both read `o.status` today; both must change to read from `ProviderExtra::Vast`.

### 2. GPU normalization — `src/providers/gpu.rs` (new)
Curated raw→canonical lookup (copy the `arch.rs` table pattern) so "cheapest H100 across providers" is comparable.

### 3. Offer→provider identity — `src/state.rs`
`CachedOffer` gains `provider`; `cmd_up` resolves provider from the cached offer (not `resolve_provider`'s guess). NOTE: in Phase A (vast-only) this is a no-op; the actual `cmd_up` routing rewire can land in Phase F where multiple providers exist — Phase A just adds the `provider` data to the cache.

### 4. Cost-optimizer (MVP arbitrage) — `src/lib.rs` `cmd_search`
Query all configured providers in parallel (`tokio`), normalize into `Vec<Offer>`, sort by `price_per_hour_usd` (`--sort price|value`). `--provider <name>` scopes to one. Missing key → skip with a note.

### 5. Per-provider credentials — `src/config.rs`
`[providers.vast|clore|runpod].api_key`. Back-compat: top-level `vast_api_key` still read as `providers.vast.api_key` (+ `config show` deprecation note).

### 6. Adaptors — `src/providers/clore.rs`, `src/providers/runpod.rs` (new)
Implement the `Provider` trait; map each API into neutral `Offer` + its `ProviderExtra`. RunPod: `search`→GPU types, `create`→`POST /v1/pods`, `status`→`GET /v1/pods/<id>`, `destroy`→terminate. Mirror `vast.rs`'s mockito test structure.

## Phased implementation (each phase = working software)

- **Phase A — Offer refactor + provider identity (vast-only, no behavior change).** Restructure `Offer`; add `provider`; rewrite `format.rs` into a neutral table (+ `provider` column, vast extras conditional); fix `filter_by_status` + the `offer()` helper. **Highest blast radius; do with fresh context.** New unit tests for the neutral schema + a live vast smoke (`search`→`up`→`down`) confirming zero change. Ship before any new provider.
- **Phase B — Per-provider credentials** (`[providers.x]` + vast back-compat).
- **Phase C — GPU normalization** (`gpu.rs`).
- **Phase D — Clore adaptor.**
- **Phase E — RunPod adaptor.**
- **Phase F — Multi-provider parallel search + ranking** (the arbitrage; + `cmd_up` provider-from-cache routing).

## Critical files
Modify: `src/providers/mod.rs` (schema, `ProviderId`, `ProviderExtra`, `AnyProvider` variants, parallel-search helper), `src/providers/vast.rs` (map → neutral Offer + VastExtra), `src/format.rs` (neutral table), `src/state.rs` (`CachedOffer.provider`), `src/config.rs` (`[providers.x]`), `src/lib.rs` (`cmd_search` fan-out, `cmd_up` route-from-cache, `filter_by_status`), `src/cli.rs` (`search --provider`, `--sort`). Create: `src/providers/clore.rs`, `src/providers/runpod.rs`, `src/providers/gpu.rs`.

## Reuse
`Provider` trait + `AnyProvider` enum-dispatch; `arch.rs` curated-table pattern for `gpu.rs`; `vast.rs` mockito tests as the adaptor test template; `tokio` (already a dep) for parallel search.

## Risks (ranked)
- **HIGH — Phase A Offer refactor.** Touches providers/format/state/lib; `Offer→CachedOffer→up` path + `format.rs` rendering are untested. Needs new schema tests + manual vast smoke. The `filter_by_status`/`offer()`-helper/`o.status`→extra interaction is the sharp edge.
- **MEDIUM — config schema.** `[providers.x]` must not break existing `config.toml` (`#[serde(default)]`, keep `vast_api_key`). Test both shapes.
- **MEDIUM — RunPod pod model ≠ marketplace.** `search` = GPU types not machine offers; may need synthetic offer-ids. This is *why* both providers go in together.
- **LOW — RunPod mining restriction.** Doc that mining → vast/Clore, not RunPod.

## Verification
- Per phase: verify trio (`cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`) + new unit tests (catalog merge, `gpu::canonical`, `rank_offers` are pure/unit-testable).
- Phase A live smoke (vast, ~$0.03): `search --provider vast` → `up <id> --profile smoke --wait` → `prompt` → `down`; identical behavior.
- Phases D/E live smoke: funded Clore + RunPod keys; per-provider `search`, rent cheapest small box, `up`→ready→`down` (mock with mockito first; live run only confirms wiring).
- Phase F: bare `silo search` → single ranked table across all three, sorted by price; `silo up <id>` routes to the right provider.

## Out of scope (deferred)
- Multi-tenant/SaaS/billing/web (productise decision — open).
- Observability "tail-and-clean" + death-detection (#2/#8/#9, 2026-06-03 backlog) — its own plan.
- `#10` persist-launch-context (`prompt`/`logs` wrong-profile) — pre-existing; fix near Phase A (both touch state).
- Spot-preemption modeling, historical pricing, auto-failover (SkyPilot has them; overkill).
