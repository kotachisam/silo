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
    pub tp_size: Option<u32>,
    pub ready_probe: Option<String>,
}

impl AnyWorkload {
    pub fn from_name(name: &str, inputs: WorkloadInputs) -> Result<Self> {
        match name {
            "inference" => Ok(Self::Inference(inference::InferenceWorkload {
                block_arch: inputs.block_arch.to_vec(),
                model_id: inputs.model_id,
                tp_size: inputs.tp_size,
            })),
            "mining" => Ok(Self::Mining(mining::MiningWorkload {
                ready_probe: inputs.ready_probe,
            })),
            other => anyhow::bail!(
                "unknown workload '{other}' (known: inference, mining); set [up.profiles.<name>].workload"
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
            WorkloadInputs {
                block_arch: &[],
                model_id: None,
                tp_size: None,
                ready_probe: None,
            },
        )
        .unwrap();
        assert_eq!(infer.name(), "inference");

        let mining = AnyWorkload::from_name(
            "mining",
            WorkloadInputs {
                block_arch: &[],
                model_id: None,
                tp_size: None,
                ready_probe: Some("true".into()),
            },
        )
        .unwrap();
        assert_eq!(mining.name(), "mining");
    }

    #[test]
    fn from_name_rejects_unknown() {
        let err = AnyWorkload::from_name(
            "quantum",
            WorkloadInputs {
                block_arch: &[],
                model_id: None,
                tp_size: None,
                ready_probe: None,
            },
        );
        assert!(err.is_err());
    }
}
