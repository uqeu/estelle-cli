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
    let estelle = estelle_status();
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
    let local_configured = local_provider::configured_present();
    let local = if local_configured {
        "configured · endpoint metadata stored; runtime binding not yet proven"
    } else {
        "missing"
    };
    let copilot = if copilot_login::credential_present() {
        "present · GitHub device-flow store; entitlement/runtime not yet proven"
    } else {
        "missing"
    };
    let machine = estelle_machine::machine();
    let machine_summary = machine.summary_line();
    let mut lines = render_lines(
        estelle,
        chatgpt,
        claude,
        copilot,
        local,
        login_command,
        &machine_summary,
    );
    if local_configured {
        lines.extend(local_provider::capability_lines(&machine));
    }
    lines
}

fn estelle_status() -> &'static str {
    let env_present = std::env::var_os("ESTELLE_API_KEY").is_some();
    let stored_file_present =
        CredentialStore::default_location().is_ok_and(|store| store.path().is_file());
    render_estelle_status(env_present, stored_file_present)
}

fn render_estelle_status(env_present: bool, stored_file_present: bool) -> &'static str {
    if env_present {
        "present · ESTELLE_API_KEY environment; runtime binding not yet proven"
    } else if stored_file_present {
        "present · mode-0600 fallback store; runtime binding not yet proven"
    } else {
        "secure-store presence not probed · run a live command or estelle login"
    }
}

fn render_lines(
    estelle: &str,
    chatgpt: &str,
    claude: &str,
    copilot: &str,
    local: &str,
    login_command: &str,
    machine: &str,
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
        machine.to_string(),
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
            "This machine · 32.0 GB RAM (24.0 GB available) · 12 CPU cores · no GPU detected",
        )
        .join("\n");
        assert!(!rendered.contains("estelle_live_"));
        assert!(!rendered.contains("accessToken"));
        assert!(!rendered.contains("refreshToken"));
        assert!(rendered.contains("Local model"));
        assert!(rendered.contains("This machine"));
        assert!(rendered.contains("repair with /login"));
    }

    #[test]
    fn doctor_does_not_turn_an_unprobed_secure_store_into_missing() {
        assert_eq!(
            render_estelle_status(false, false),
            "secure-store presence not probed · run a live command or estelle login"
        );
        assert_eq!(
            render_estelle_status(true, false),
            "present · ESTELLE_API_KEY environment; runtime binding not yet proven"
        );
    }
}
