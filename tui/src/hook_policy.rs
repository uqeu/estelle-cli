//! Explicit installation policy. Existing installations are not silently opted in.
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Policy {
    Required,
    Advisory,
    Disabled,
    /// Compatibility for an old opt-in only, never a saved or selectable new policy.
    #[serde(skip)]
    LegacyFlaggedOnly,
}

impl Policy {
    pub(crate) fn parse(text: &str) -> Result<Self, String> {
        match text.trim().to_ascii_lowercase().as_str() {
            "required" => Ok(Self::Required),
            "advisory" => Ok(Self::Advisory),
            "disabled" => Ok(Self::Disabled),
            _ => Err("hook policy must be required, advisory, or disabled".into()),
        }
    }
    pub(crate) fn legacy_refusal_enabled(self) -> bool {
        self == Self::LegacyFlaggedOnly
    }
    pub(crate) fn requests_answer_repair(self) -> bool {
        self == Self::Required
    }
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Required => "required",
            Self::Advisory => "advisory",
            Self::Disabled => "disabled",
            Self::LegacyFlaggedOnly => "legacy flagged-only (not required)",
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum ShellPolicy {
    Pause,
    Observe,
}
impl ShellPolicy {
    pub(crate) fn parse(text: &str) -> Result<Self, String> {
        match text.trim().to_ascii_lowercase().as_str() {
            "pause" => Ok(Self::Pause),
            "observe" => Ok(Self::Observe),
            _ => Err("shell policy must be pause or observe".into()),
        }
    }
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Pause => "pause",
            Self::Observe => "observe (unverified, not enforced)",
        }
    }
}
#[derive(Deserialize, Serialize)]
struct Settings {
    grounding: Policy,
    #[serde(default)]
    shell: Option<ShellPolicy>,
}
fn path() -> Result<PathBuf, String> {
    dirs::home_dir()
        .map(|p| p.join(".estelle/hook-policy.json"))
        .ok_or_else(|| "cannot locate the hook policy directory".into())
}
pub(crate) fn current() -> Result<Policy, String> {
    let explicit = std::env::var("ESTELLE_HOOK_POLICY").ok();
    let legacy = std::env::var(estelle_tui::ground_block::BLOCK_ENV).ok();
    if explicit.is_some()
        || legacy.as_deref().is_some_and(|v| {
            matches!(
                v.trim().to_ascii_lowercase().as_str(),
                "0" | "false" | "off"
            )
        })
    {
        return resolve(explicit.as_deref(), legacy.as_deref(), None);
    }
    let configured=match std::fs::read(path()?) {
        Ok(bytes)=>Some(serde_json::from_slice::<Settings>(&bytes)
            .map_err(|_|"hook policy is malformed; select required, advisory, or disabled explicitly")?.grounding),
        Err(e) if e.kind()==std::io::ErrorKind::NotFound=>None,
        Err(_)=>return Err("hook policy could not be read".into()),
    };
    resolve(explicit.as_deref(), legacy.as_deref(), configured)
}
fn resolve(
    explicit: Option<&str>,
    legacy: Option<&str>,
    configured: Option<Policy>,
) -> Result<Policy, String> {
    if let Some(policy) = explicit {
        return Policy::parse(policy);
    }
    // A legacy explicit opt-out remains advisory and is never reported as checked.
    if legacy.is_some_and(|v| {
        matches!(
            v.trim().to_ascii_lowercase().as_str(),
            "0" | "false" | "off"
        )
    }) {
        return Ok(Policy::Advisory);
    }
    if let Some(policy) = configured {
        return Ok(policy);
    }
    if estelle_tui::ground_block::blocking_enabled_from(legacy) {
        return Ok(Policy::LegacyFlaggedOnly);
    }
    Ok(Policy::Advisory)
}
pub(crate) fn current_shell() -> Result<Option<ShellPolicy>, String> {
    if let Ok(value) = std::env::var("ESTELLE_HOOK_SHELL_POLICY") {
        return ShellPolicy::parse(&value).map(Some);
    }
    match std::fs::read(path()?) {
        Ok(bytes) => serde_json::from_slice::<Settings>(&bytes)
            .map(|settings| settings.shell)
            .map_err(|_| "hook shell policy is malformed".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err("hook shell policy could not be read".into()),
    }
}
pub(crate) fn validate_install(policy: Policy, shell: Option<ShellPolicy>) -> Result<(), String> {
    if policy == Policy::LegacyFlaggedOnly {
        return Err("legacy flagged-only compatibility cannot be saved; select required, advisory, or disabled explicitly before changing hook policy".into());
    }
    if policy == Policy::Required && shell.is_none() {
        return Err("required direct-edit policy needs an explicit shell-policy choice: --shell-policy pause blocks unpreviewable shell operations; --shell-policy observe allows them as UNVERIFIED and is not whole-code-write enforcement. No hook files were changed".into());
    }
    Ok(())
}
pub(crate) fn install(policy: Policy, shell: Option<ShellPolicy>) -> Result<(), String> {
    validate_install(policy, shell)?;
    let path = path()?;
    if path.exists() {
        let previous =
            std::fs::read(&path).map_err(|_| "existing hook policy could not be backed up")?;
        serde_json::from_slice::<Settings>(&previous)
            .map_err(|_| "existing hook policy is malformed; refusing overwrite")?;
        std::fs::write(path.with_extension("json.bak"), previous)
            .map_err(|_| "existing hook policy could not be backed up")?;
    }
    crate::provider_store::write_private_json(
        &serde_json::json!({"grounding":policy,"shell":shell}),
        &path,
        "hook-policy",
    )
    .map_err(|_| "hook policy could not be written".into())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_opt_in_is_distinct_and_only_used_without_modern_policy() {
        for legacy in ["1", "true", " ON "] {
            assert_eq!(
                resolve(None, Some(legacy), None).unwrap(),
                Policy::LegacyFlaggedOnly
            );
            for policy in [Policy::Required, Policy::Advisory, Policy::Disabled] {
                assert_eq!(resolve(None, Some(legacy), Some(policy)).unwrap(), policy);
                assert_eq!(
                    resolve(Some(policy.label()), Some(legacy), Some(Policy::Required)).unwrap(),
                    policy
                );
            }
        }
        assert_eq!(
            resolve(None, Some("garbage"), None).unwrap(),
            Policy::Advisory
        );
    }
    #[test]
    fn compatibility_mode_is_not_a_new_installation_choice() {
        assert!(Policy::parse("legacyflaggedonly").is_err());
        assert!(serde_json::from_str::<Settings>(r#"{"grounding":"legacyflaggedonly"}"#).is_err());
        assert!(validate_install(Policy::LegacyFlaggedOnly, None).is_err());
        assert_ne!(Policy::LegacyFlaggedOnly.label(), Policy::Advisory.label());
    }
    #[test]
    fn only_required_repairs_answers_and_only_implicit_legacy_uses_old_refusal() {
        for policy in [
            Policy::Required,
            Policy::Advisory,
            Policy::Disabled,
            Policy::LegacyFlaggedOnly,
        ] {
            assert_eq!(policy.requests_answer_repair(), policy == Policy::Required);
            assert_eq!(
                policy.legacy_refusal_enabled(),
                policy == Policy::LegacyFlaggedOnly
            );
        }
    }
    #[test]
    fn no_silent_migration() {
        assert_eq!(resolve(None, None, None).unwrap(), Policy::Advisory);
    }
    #[test]
    fn selected_required_is_active() {
        assert_eq!(
            resolve(None, None, Some(Policy::Required)).unwrap(),
            Policy::Required
        );
    }
    #[test]
    fn explicit_opt_out_survives() {
        assert_eq!(
            resolve(None, Some("off"), Some(Policy::Required)).unwrap(),
            Policy::Advisory
        );
    }
    #[test]
    fn explicit_new_setting_is_authoritative() {
        assert_eq!(
            resolve(Some("required"), Some("off"), None).unwrap(),
            Policy::Required
        );
        assert_eq!(
            resolve(Some("disabled"), None, Some(Policy::Required)).unwrap(),
            Policy::Disabled
        );
    }
    #[test]
    fn invalid_policy_is_not_approval() {
        assert!(resolve(Some("best-effort-maybe"), None, None).is_err());
    }
    #[test]
    fn required_install_never_silently_selects_a_shell_tradeoff() {
        assert!(validate_install(Policy::Required, None).is_err());
        assert!(validate_install(Policy::Required, Some(ShellPolicy::Pause)).is_ok());
        assert!(validate_install(Policy::Required, Some(ShellPolicy::Observe)).is_ok());
        assert!(validate_install(Policy::Advisory, None).is_ok());
    }
}
