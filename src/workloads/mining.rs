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
