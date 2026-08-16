use estelle_client::CredentialSource;
use estelle_client::CredentialStore;

use crate::claude_import;
use crate::copilot_login;
use crate::local_provider;
use crate::login;

#[derive(Clone, Copy)]
pub(crate) enum Context {
    Shell,
    Tui,
}

pub(crate) fn lines(context: Context) -> Vec<String> {
    let login_command = match context {
        Context::Shell => "estelle login",
        Context::Tui => "/login",
    };
    let estelle = match CredentialStore::default_location().and_then(|store| store.resolve()) {
        Ok(credential) => match credential.source {
            CredentialSource::Environment => "present · ESTELLE_API_KEY environment",
            CredentialSource::SecureStore => "present · secure store",
            CredentialSource::Stored => "present · mode-0600 fallback store",
        },
        Err(_) => "missing",
    };
    let chatgpt = if login::chatgpt_credential_present() {
        "present · device-code store"
    } else {
        "missing"
    };
    let claude = if claude_import::imported_credential_present() {
        "imported · Estelle snapshot; source remains Claude Code-owned"
    } else {
        "missing"
    };
    let local = if local_provider::configured_present() {
        "configured · endpoint metadata stored; runtime binding not yet proven"
    } else {
        "missing"
    };
    let copilot = if copilot_login::credential_present() {
        "present · GitHub device-flow store; entitlement/runtime not yet proven"
    } else {
        "missing"
    };
    render_lines(estelle, chatgpt, claude, copilot, local, login_command)
}

fn render_lines(
    estelle: &str,
    chatgpt: &str,
    claude: &str,
    copilot: &str,
    local: &str,
    login_command: &str,
) -> Vec<String> {
    let row = |label: &str, status: &str| {
        if status == "missing" {
            format!("{label}  missing · repair with {login_command}")
        } else {
            format!("{label}  {status}")
        }
    };
    vec![
        row("Estelle account", estelle),
        row("ChatGPT plan", chatgpt),
        row("Claude plan", claude),
        row("GitHub Copilot", copilot),
        "Provider API keys  server-owned · inspect names with /whoami; values never render"
            .to_string(),
        row("Local model", local),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doctor_never_contains_credential_values() {
        // Rendering is deliberately tested without consulting the customer's live Keychain.
        let rendered = render_lines(
            "present · secure store",
            "missing",
            "imported",
            "present · runtime binding not yet proven",
            "configured · runtime binding not yet proven",
            "/login",
        )
        .join("\n");
        assert!(!rendered.contains("estelle_live_"));
        assert!(!rendered.contains("accessToken"));
        assert!(!rendered.contains("refreshToken"));
        assert!(rendered.contains("Local model"));
        assert!(rendered.contains("repair with /login"));
    }
}
