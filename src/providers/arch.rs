//! GPU model name → architecture family lookup, plus a curated model+arch
//! compatibility table. Used by silo's compatibility check at `silo up`.
//!
//! `arch_for` maps vast.ai's GPU model strings to architecture families.
//! `compat_check` consults a hand-curated rules table for known model+arch
//! incompatibilities. Both are personal-tool curation: extend as you encounter
//! new failures or new GPU generations.

#[derive(Debug, Clone, PartialEq)]
pub enum Compat {
    /// Known to work or no rule against the combo.
    Ok,
    /// Works but with a caveat the user should know.
    Unstable(&'static str),
    /// Known to fail. Should bail unless explicitly overridden.
    Broken(&'static str),
}

/// Curated table of (model_substring, arch, status) rules. Match is
/// case-insensitive substring on model id. First match wins.
const COMPAT_RULES: &[(&str, &str, Compat)] = &[
    // gpt-oss family is MXFP4-quantized, dispatched through vLLM's MARLIN MoE
    // backend. As of vLLM 0.20.1 the MARLIN path has incomplete Blackwell
    // sm_120 support — worker crashes during weight load. Hopper/Ada/Ampere
    // work; Blackwell needs newer vLLM (0.22+ rumoured).
    (
        "gpt-oss",
        "Blackwell",
        Compat::Broken(
            "vLLM <0.22 MARLIN MXFP4 backend has incomplete Blackwell (sm_120) support. \
             Crashes during multi-GPU weight load. Use Hopper (H100/H200) or test with newer vLLM.",
        ),
    ),
    // Ampere lacks native FP8 hardware (Hopper introduced sm_90 FP8 tensor cores).
    // vLLM's --quantization fp8 falls back to software paths — works but slow.
    (
        "fp8",
        "Ampere",
        Compat::Unstable(
            "Ampere has no native FP8 tensor cores. vLLM falls back to software FP8 paths — \
             functional but materially slower than Hopper/Ada.",
        ),
    ),
];

pub fn compat_check(model_id: &str, arch: &str) -> Compat {
    let model_lower = model_id.to_lowercase();
    for (model_pattern, rule_arch, status) in COMPAT_RULES {
        if model_lower.contains(&model_pattern.to_lowercase())
            && arch.eq_ignore_ascii_case(rule_arch)
        {
            return status.clone();
        }
    }
    Compat::Ok
}

pub fn arch_for(gpu_model: &str) -> Option<&'static str> {
    let upper = gpu_model.to_uppercase().replace(' ', "_");

    if upper.contains("RTX_PRO_6000") || upper.contains("B100") || upper.contains("B200") {
        return Some("Blackwell");
    }
    if upper.starts_with("RTX_50") {
        return Some("Blackwell");
    }
    if upper.contains("H100") || upper.contains("H200") {
        return Some("Hopper");
    }
    if upper.contains("L40") {
        return Some("Ada");
    }
    if upper.starts_with("RTX_40") {
        return Some("Ada");
    }
    if upper.contains("A100") || upper.contains("A40") || upper.contains("A30") {
        return Some("Ampere");
    }
    if upper.starts_with("RTX_30") {
        return Some("Ampere");
    }
    if upper.contains("V100") {
        return Some("Volta");
    }
    if upper.contains("T4") || upper.starts_with("RTX_20") || upper.contains("QUADRO_RTX") {
        return Some("Turing");
    }
    if upper.starts_with("GTX_16") {
        return Some("Turing");
    }
    if upper.starts_with("GTX_10") || upper.contains("QUADRO_P") {
        return Some("Pascal");
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blackwell_pro_cards() {
        assert_eq!(arch_for("RTX_PRO_6000_S"), Some("Blackwell"));
        assert_eq!(arch_for("RTX_PRO_6000_WS"), Some("Blackwell"));
        assert_eq!(arch_for("B200"), Some("Blackwell"));
        assert_eq!(arch_for("RTX 5060 Ti"), Some("Blackwell"));
    }

    #[test]
    fn hopper_cards() {
        assert_eq!(arch_for("H100"), Some("Hopper"));
        assert_eq!(arch_for("H200"), Some("Hopper"));
        assert_eq!(arch_for("H200_NVL"), Some("Hopper"));
    }

    #[test]
    fn ada_cards() {
        assert_eq!(arch_for("RTX 4090"), Some("Ada"));
        assert_eq!(arch_for("RTX_4070_Ti"), Some("Ada"));
        assert_eq!(arch_for("L40S"), Some("Ada"));
    }

    #[test]
    fn ampere_cards() {
        assert_eq!(arch_for("A100_SXM4"), Some("Ampere"));
        assert_eq!(arch_for("RTX 3090"), Some("Ampere"));
        assert_eq!(arch_for("RTX_3060"), Some("Ampere"));
    }

    #[test]
    fn older_cards() {
        assert_eq!(arch_for("V100"), Some("Volta"));
        assert_eq!(arch_for("Tesla T4"), Some("Turing"));
        assert_eq!(arch_for("GTX_1660_Ti"), Some("Turing"));
        assert_eq!(arch_for("GTX_1070"), Some("Pascal"));
    }

    #[test]
    fn unknown_returns_none() {
        assert_eq!(arch_for("FutureGPU 9000"), None);
    }

    #[test]
    fn case_and_space_insensitive() {
        assert_eq!(arch_for("rtx pro 6000 s"), Some("Blackwell"));
        assert_eq!(arch_for("RTX_PRO_6000_S"), Some("Blackwell"));
    }

    #[test]
    fn compat_flags_gpt_oss_on_blackwell() {
        match compat_check("openai/gpt-oss-120b", "Blackwell") {
            Compat::Broken(_) => {}
            other => panic!("expected Broken, got {other:?}"),
        }
    }

    #[test]
    fn compat_ok_for_qwen_on_blackwell() {
        assert_eq!(
            compat_check("Qwen/Qwen2.5-72B-Instruct", "Blackwell"),
            Compat::Ok
        );
    }

    #[test]
    fn compat_ok_for_gpt_oss_on_hopper() {
        assert_eq!(compat_check("openai/gpt-oss-120b", "Hopper"), Compat::Ok);
    }

    #[test]
    fn compat_unstable_for_fp8_on_ampere() {
        match compat_check("meta-llama/Llama-3.3-70B-FP8", "Ampere") {
            Compat::Unstable(_) => {}
            other => panic!("expected Unstable, got {other:?}"),
        }
    }

    #[test]
    fn compat_case_insensitive_model_match() {
        match compat_check("OPENAI/GPT-OSS-20B", "blackwell") {
            Compat::Broken(_) => {}
            other => panic!("expected Broken, got {other:?}"),
        }
    }
}
