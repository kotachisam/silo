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
                arch,
                offer_id,
                offer.gpu_name,
                arch
            );
        }

        if let Some(model) = self.model_id.as_deref() {
            match arch::compat_check(model, arch) {
                arch::Compat::Ok => {}
                arch::Compat::Unstable(note) => {
                    eprintln!(
                        "warning: model '{model}' on {} ({}): {note}",
                        offer.gpu_name, arch
                    );
                    eprintln!("(proceeding; pass --skip-compat-check to silence)");
                }
                arch::Compat::Broken(note) => {
                    anyhow::bail!(
                        "known-bad combo: model '{model}' on {} ({}): {note}\n(override with --skip-compat-check if you want to test anyway)",
                        offer.gpu_name,
                        arch
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
            CachedOffer {
                gpu_name: gpu.into(),
                num_gpus: 1,
                vram_gb: 90.0,
            },
        );
        s
    }

    #[test]
    fn preflight_bails_on_blocked_arch() {
        let w = InferenceWorkload {
            block_arch: vec!["Blackwell".into()],
            model_id: None,
        };
        let s = state_with_offer("42", "RTX PRO 6000");
        assert!(w.preflight(&s, "42", false).is_err());
    }

    #[test]
    fn preflight_skip_short_circuits() {
        let w = InferenceWorkload {
            block_arch: vec!["Blackwell".into()],
            model_id: None,
        };
        let s = state_with_offer("42", "RTX PRO 6000");
        assert!(w.preflight(&s, "42", true).is_ok());
    }

    #[test]
    fn preflight_ok_when_offer_uncached() {
        let w = InferenceWorkload {
            block_arch: vec![],
            model_id: None,
        };
        assert!(w.preflight(&State::default(), "999", false).is_ok());
    }
}
