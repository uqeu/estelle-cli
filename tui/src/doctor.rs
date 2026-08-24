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
    match std::env::current_dir()
        .map_err(|error| error.to_string())
        .and_then(|root| crate::top_level::language_preflight_lines(&root))
    {
        Ok(preflight) => lines.extend(preflight),
        Err(error) => lines.push(format!(
            "Repository ingest preflight  FAIL · local inventory unavailable: {error}"
        )),
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

    #[test]
    fn mixed_repo_reports_each_language_and_cannot_hide_a_blocked_go_side() {
        let root = tempfile::tempdir().expect("repo");
        std::fs::write(
            root.path().join("worker.ts"),
            "export class RetryScheduler {}\n",
        )
        .expect("typescript");
        std::fs::write(root.path().join("worker.go"), vec![b'x'; 400_001]).expect("oversize go");
        let lines = crate::top_level::language_preflight_lines(root.path()).expect("preflight");
        assert!(lines.iter().any(|line| {
            line == "Repository TypeScript ingest preflight  ready · 1/1 files cross the local ingest boundary"
        }));
        assert!(lines.iter().any(|line| {
            line == "Repository Go ingest preflight  FAIL · 0/1 files cross the local ingest boundary"
        }));
        assert!(
            lines
                .iter()
                .any(|line| line.contains("server index/runtime not proven"))
        );
    }
}

/// `doctor`, but it can be WRONG about you — it makes a real call. Returns the lines plus whether any
/// configured provider failed its binding probe.
///
/// 🔴 **`lines()` ABOVE CANNOT FAIL, AND THAT WAS THE PROBLEM.** Every status it renders is a presence
/// check on a local file, so it reported the same thing whether the provider worked or was dead — and
/// four of its own strings simply repeated *"runtime binding not yet proven"*, which is what the login
/// paths had already said. A user told to run `doctor` was handed back the sentence that sent them
/// there. This wrapper is the exit from that loop: it asks the endpoint.
///
/// ⚠️ The bool is NOT "is anything missing". An unconfigured provider is a normal state and must never
/// make a fresh install report broken; only a provider that is configured AND does not answer counts.
pub(crate) async fn lines_with_binding(context: Context) -> (Vec<String>, bool) {
    let mut rendered = lines(context);
    let binding = probe_local_binding().await;
    rendered.push(binding.line("Local model"));
    let failed = binding.is_failure();
    (rendered, failed)
}

async fn probe_local_binding() -> crate::binding_probe::Binding {
    use crate::binding_probe::Binding;
    let stored = match local_provider::stored_endpoint() {
        // ⚠️ An unreadable store is NOT "not configured". Reporting a corrupt file as absent would send
        // the user through a login that had already succeeded, chasing a state they cannot see.
        Err(error) => {
            return Binding::Unreachable {
                reason: format!("the stored endpoint could not be read: {error}"),
            }
        }
        Ok(None) => return Binding::NotConfigured,
        Ok(Some(stored)) => stored,
    };
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(
            crate::binding_probe::PROBE_TIMEOUT_S,
        ))
        .build()
    {
        Ok(client) => client,
        Err(error) => {
            return Binding::Unreachable {
                reason: format!("no HTTP client could be built: {error}"),
            }
        }
    };
    crate::binding_probe::probe_openai_compatible(
        &client,
        &stored.base_url,
        stored.api_key.as_deref(),
    )
    .await
}
