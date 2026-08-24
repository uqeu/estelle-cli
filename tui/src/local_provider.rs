//! Local endpoint acquisition, informed by Goose local-inference's explicit
//! model registry and jcode's no-key localhost profiles (both MIT).

use std::fs;
use std::io;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;

use serde::Deserialize;
use serde::Serialize;
use zeroize::Zeroizing;

use crate::login;
use crate::provider_catalog;
use crate::provider_store;

#[derive(Serialize)]
struct LocalProviderSnapshot<'a> {
    provider: &'a str,
    base_url: &'a str,
    model: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    api_key: Option<&'a str>,
}

/// The snapshot as READ BACK, owned.
///
/// ⚠️ Deliberately a separate type from :struct:`LocalProviderSnapshot`, which borrows (`&'a str`) so
/// it can be written without copying a secret. A reader cannot borrow from a file it just closed, so
/// the two shapes are genuinely different and sharing one type would mean weakening the writer.
#[derive(Deserialize)]
pub(crate) struct StoredLocalProvider {
    pub(crate) base_url: String,
    #[serde(default)]
    pub(crate) api_key: Option<String>,
}

/// Load the configured local endpoint back, if there is one.
///
/// 🔴 Until this existed nothing in production could read this file. `configured_present()` answered
/// "is there a file", which is presence, not capability — and presence was the entire basis on which
/// `doctor` reported. See :mod:`crate::binding_probe`.
pub(crate) fn stored_endpoint() -> io::Result<Option<StoredLocalProvider>> {
    provider_store::read_private_json(&store_path()?)
}

#[derive(Deserialize, Serialize)]
struct LocalModelProfile {
    model: Option<estelle_machine::Model>,
    unavailable_reason: Option<LocalModelUnavailable>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum LocalModelUnavailable {
    NoModelSupplied,
    ExactMetadataUnavailable,
}

fn store_path() -> io::Result<PathBuf> {
    dirs::home_dir()
        .map(|home| home.join(".estelle/providers/local.json"))
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "home directory is unavailable"))
}

fn model_profile_path(store: &Path) -> io::Result<PathBuf> {
    let parent = store.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "local provider store has no parent",
        )
    })?;
    Ok(parent.join("local-model.json"))
}

pub(crate) fn configured_present() -> bool {
    store_path().is_ok_and(|path| path.is_file())
}

pub(crate) fn logout() -> io::Result<bool> {
    let path = store_path()?;
    let profile = model_profile_path(&path)?;
    let removed_store = remove_if_present(&path)?;
    let removed_profile = remove_if_present(&profile)?;
    Ok(removed_store || removed_profile)
}

fn remove_if_present(path: &Path) -> io::Result<bool> {
    match fs::remove_file(path) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

pub(crate) async fn run(
    provider: &str,
    supplied_base: Option<&str>,
    model: Option<&str>,
) -> io::Result<crate::binding_probe::Binding> {
    let descriptor = provider_catalog::resolve(provider)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "unknown local provider"))?;
    if descriptor.auth != provider_catalog::AuthKind::LocalEndpoint {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "provider is not a local/custom endpoint",
        ));
    }
    let prompted_base = if supplied_base.is_none() && descriptor.requires_base_url() {
        login::read_plain_value(b"Provider API base URL: ")?
    } else {
        None
    };
    let route =
        provider_catalog::login_route(provider, supplied_base.or(prompted_base.as_deref()))?;
    let secret = if route.requires_key {
        login::read_secret_value(b"Provider API key: ")?.ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::Interrupted,
                "this non-local endpoint requires an API key; nothing was stored",
            )
        })?
    } else {
        Zeroizing::new(String::new())
    };
    let secret = (!secret.is_empty()).then_some(secret);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(
            crate::binding_probe::PROBE_TIMEOUT_S,
        ))
        .build()
        .map_err(io::Error::other)?;
    persist_and_probe(
        &client,
        &route,
        model,
        secret.as_deref().map(String::as_str),
        &store_path()?,
        &mut io::stdout(),
    )
    .await
}

async fn persist_and_probe(
    client: &reqwest::Client,
    route: &provider_catalog::LoginRoute,
    model: Option<&str>,
    api_key: Option<&str>,
    destination: &Path,
    output: &mut impl Write,
) -> io::Result<crate::binding_probe::Binding> {
    persist_model_profile(model, destination)?;
    let snapshot = LocalProviderSnapshot {
        provider: route.provider.id,
        base_url: route.base_url.as_deref().unwrap_or_default(),
        model: model.unwrap_or_default(),
        api_key,
    };
    provider_store::write_private_json(&snapshot, destination, "local.json")?;

    writeln!(
        output,
        "Endpoint configured: {} · {}.",
        route.provider.display_name,
        route.base_url.as_deref().unwrap_or_default()
    )?;
    writeln!(
        output,
        "Credential: {}.",
        if api_key.is_some() {
            "stored privately"
        } else {
            "not required for this local endpoint"
        }
    )?;
    let binding = crate::binding_probe::probe_openai_compatible(
        client,
        route.base_url.as_deref().unwrap_or_default(),
        api_key,
    )
    .await;
    writeln!(output, "{}", binding.line(route.provider.display_name))?;
    output.flush()?;
    Ok(binding)
}

fn persist_model_profile(model: Option<&str>, credential_store: &Path) -> io::Result<()> {
    let requested_model = model.unwrap_or_default().trim().to_string();
    let (model, unavailable_reason) = if requested_model.is_empty() {
        (None, Some(LocalModelUnavailable::NoModelSupplied))
    } else {
        match estelle_machine::named_model(&requested_model) {
            Ok(model) => (Some(model), None),
            Err(_) => (None, Some(LocalModelUnavailable::ExactMetadataUnavailable)),
        }
    };
    provider_store::write_private_json(
        &LocalModelProfile {
            model,
            unavailable_reason,
        },
        &model_profile_path(credential_store)?,
        "local-model.json",
    )
}

pub(crate) fn capability_lines(machine: &estelle_machine::Machine) -> Vec<String> {
    let profile = store_path()
        .and_then(|path| model_profile_path(&path))
        .and_then(fs::read)
        .and_then(|bytes| {
            serde_json::from_slice::<LocalModelProfile>(&bytes).map_err(io::Error::other)
        });
    capability_lines_from(profile, machine)
}

fn capability_lines_from(
    profile: io::Result<LocalModelProfile>,
    machine: &estelle_machine::Machine,
) -> Vec<String> {
    let profile = match profile {
        Ok(profile) => profile,
        Err(error) => {
            return vec![format!(
                "Local fit  not measured · non-secret model profile unavailable ({})",
                error.kind()
            )];
        }
    };
    let Some(model) = profile.model else {
        let reason = match profile.unavailable_reason {
            Some(LocalModelUnavailable::NoModelSupplied) => "no local model name was supplied",
            Some(LocalModelUnavailable::ExactMetadataUnavailable) => {
                "exact bundled metadata unavailable for configured local model"
            }
            None => "model metadata unavailable",
        };
        return vec![format!("Local fit  not measured · {reason}")];
    };
    match estelle_machine::fit(&model, machine) {
        Ok(fit) => vec![
            format!(
                "Local fit  {} · {} · {} · {:.1}/{:.1} GB · estimated {:.1} tok/s",
                fit.model_name,
                fit.fit_level.label(),
                fit.run_mode.label(),
                fit.memory_required_gb,
                fit.memory_available_gb,
                fit.estimated_tokens_per_second,
            ),
            format!(
                "Local fit limit  {} Server Affinity still decides which model serves a task.",
                fit.estimate_notice
            ),
        ],
        Err(error) => vec![format!("Local fit  not measured · {error}")],
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    use tempfile::tempdir;
    use wiremock::Mock;
    use wiremock::MockServer;
    use wiremock::ResponseTemplate;
    use wiremock::matchers::header;
    use wiremock::matchers::method;
    use wiremock::matchers::path;
    use zeroize::Zeroizing;

    use super::*;

    #[tokio::test]
    async fn endpoint_snapshot_is_private_and_login_probes_the_wire() {
        let server = MockServer::start().await;
        let dir = tempdir().expect("tempdir");
        let destination = dir.path().join("providers/local.json");
        let secret = Zeroizing::new("local-test-secret".to_string());
        Mock::given(method("GET"))
            .and(path("/v1/models"))
            .and(header(
                "authorization",
                format!("Bearer {}", secret.as_str()).as_str(),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [{"id": "test-model"}]
            })))
            .expect(1)
            .mount(&server)
            .await;
        let route = crate::provider_catalog::login_route(
            "openai-compatible",
            Some(&format!("{}/v1", server.uri())),
        )
        .expect("custom route");
        let mut output = Vec::new();

        let binding = persist_and_probe(
            &reqwest::Client::new(),
            &route,
            Some("test-model"),
            Some(secret.as_str()),
            &destination,
            &mut output,
        )
        .await
        .expect("persist local endpoint");

        assert_eq!(
            fs::metadata(&destination)
                .expect("metadata")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        let receipt = String::from_utf8(output).expect("UTF-8 receipt");
        assert!(receipt.contains("Endpoint configured"));
        assert!(receipt.contains("BOUND"));
        assert!(!receipt.contains(secret.as_str()));
        assert!(matches!(
            binding,
            crate::binding_probe::Binding::Bound { .. }
        ));

        let profile = fs::read_to_string(dir.path().join("providers/local-model.json"))
            .expect("non-secret local model profile");
        assert!(profile.contains("exact_metadata_unavailable"));
        assert!(!profile.contains(secret.as_str()));

        let profile: LocalModelProfile = serde_json::from_str(&profile).expect("safe profile");
        let lines = capability_lines_from(Ok(profile), &estelle_machine::machine()).join("\n");
        assert!(lines.contains("Local fit  not measured"));
        assert!(lines.contains("exact bundled metadata unavailable"));
        assert!(!lines.contains("test-model"));
        assert!(!lines.contains(secret.as_str()));
    }

    #[tokio::test]
    async fn rejected_local_credential_is_stored_but_reported_as_refused() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/models"))
            .respond_with(ResponseTemplate::new(401))
            .expect(1)
            .mount(&server)
            .await;
        let dir = tempdir().expect("tempdir");
        let destination = dir.path().join("providers/local.json");
        let route = crate::provider_catalog::login_route(
            "openai-compatible",
            Some(&format!("{}/v1", server.uri())),
        )
        .expect("custom route");
        let mut output = Vec::new();

        let binding = persist_and_probe(
            &reqwest::Client::new(),
            &route,
            None,
            Some("rejected-key"),
            &destination,
            &mut output,
        )
        .await
        .expect("probe result");

        assert_eq!(
            binding,
            crate::binding_probe::Binding::Refused { status: 401 }
        );
        assert!(destination.is_file(), "the configured endpoint is retained");
        assert!(
            String::from_utf8(output)
                .expect("UTF-8 receipt")
                .contains("re-run the login")
        );
    }

    #[test]
    fn exact_bundled_model_profile_drives_a_fresh_machine_fit() {
        let dir = tempdir().expect("tempdir");
        let destination = dir.path().join("providers/local.json");

        persist_model_profile(Some("Fu01978/Nano-H"), &destination).expect("safe model profile");
        let bytes = fs::read(dir.path().join("providers/local-model.json")).expect("profile");
        let profile: LocalModelProfile = serde_json::from_slice(&bytes).expect("profile JSON");
        let lines = capability_lines_from(Ok(profile), &estelle_machine::machine()).join("\n");

        assert!(lines.contains("Local fit  Fu01978/Nano-H"));
        assert!(lines.contains("estimated"));
        assert!(lines.contains("Estimate-based"));
        assert!(lines.contains("Server Affinity still decides"));
    }
}
