//! Provider login metadata adapted from jcode's provider catalog (MIT).
//!
//! The behavioral tests are written first: provider identity, aliases, auth
//! acquisition, and endpoint policy must remain data rather than UI branches.

use std::io;
use std::net::IpAddr;

use url::Host;
use url::Url;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AuthKind {
    ProviderOAuth,
    CopilotDevice,
    ApiKey,
    LocalEndpoint,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Surface {
    ProviderKey,
    Local,
    Hidden,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BaseUrlKind {
    None,
    Optional,
    Required,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ProviderDescriptor {
    pub id: &'static str,
    pub display_name: &'static str,
    pub aliases: &'static [&'static str],
    pub detail: &'static str,
    pub auth: AuthKind,
    pub surface: Surface,
    pub server_provider: Option<&'static str>,
    default_base_url: Option<&'static str>,
    base_url: BaseUrlKind,
}

impl ProviderDescriptor {
    pub(crate) fn requires_base_url(self) -> bool {
        self.base_url == BaseUrlKind::Required
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LoginRoute {
    pub provider: &'static ProviderDescriptor,
    pub base_url: Option<String>,
    pub requires_key: bool,
}

const PROVIDERS: &[ProviderDescriptor] = &[
    ProviderDescriptor {
        id: "claude",
        display_name: "Claude subscription",
        aliases: &["anthropic-subscription"],
        detail: "browser sign-in · server-held OAuth · Pro, Max or Team",
        auth: AuthKind::ProviderOAuth,
        surface: Surface::Hidden,
        server_provider: None,
        default_base_url: None,
        base_url: BaseUrlKind::None,
    },
    ProviderDescriptor {
        id: "anthropic-api",
        display_name: "Anthropic",
        aliases: &["anthropic", "claude-api", "anthropic-key"],
        detail: "Anthropic API key · masked input",
        auth: AuthKind::ApiKey,
        surface: Surface::ProviderKey,
        server_provider: Some("anthropic"),
        default_base_url: None,
        base_url: BaseUrlKind::Optional,
    },
    ProviderDescriptor {
        id: "openai-api",
        display_name: "OpenAI API",
        aliases: &["openai-key", "openai-platform"],
        detail: "OpenAI API key · masked input",
        auth: AuthKind::ApiKey,
        surface: Surface::ProviderKey,
        server_provider: Some("openai"),
        default_base_url: None,
        base_url: BaseUrlKind::Optional,
    },
    ProviderDescriptor {
        id: "gemini",
        display_name: "Google Gemini",
        aliases: &["gemini-api", "google-gemini"],
        detail: "Google AI Studio API key · masked input",
        auth: AuthKind::ApiKey,
        surface: Surface::ProviderKey,
        server_provider: Some("gemini"),
        default_base_url: None,
        base_url: BaseUrlKind::Optional,
    },
    ProviderDescriptor {
        id: "copilot",
        display_name: "GitHub Copilot",
        aliases: &["github-copilot"],
        detail: "GitHub device code",
        auth: AuthKind::CopilotDevice,
        surface: Surface::Hidden,
        server_provider: None,
        default_base_url: None,
        base_url: BaseUrlKind::None,
    },
    ProviderDescriptor {
        id: "azure",
        display_name: "Azure OpenAI",
        aliases: &["azure-openai", "aoai"],
        detail: "API base and Azure OpenAI API key",
        auth: AuthKind::ApiKey,
        surface: Surface::ProviderKey,
        server_provider: Some("azure"),
        default_base_url: None,
        base_url: BaseUrlKind::Required,
    },
    ProviderDescriptor {
        id: "bedrock",
        display_name: "AWS Bedrock",
        aliases: &["aws-bedrock", "amazon-bedrock"],
        detail: "Bedrock API key · masked input",
        auth: AuthKind::ApiKey,
        surface: Surface::ProviderKey,
        server_provider: Some("bedrock"),
        default_base_url: None,
        base_url: BaseUrlKind::Optional,
    },
    ProviderDescriptor {
        id: "openrouter",
        display_name: "OpenRouter",
        aliases: &[],
        detail: "API key · 200+ models",
        auth: AuthKind::ApiKey,
        surface: Surface::ProviderKey,
        server_provider: Some("openrouter"),
        default_base_url: None,
        base_url: BaseUrlKind::Optional,
    },
    ProviderDescriptor {
        id: "deepseek",
        display_name: "DeepSeek",
        aliases: &[],
        detail: "API key · masked input",
        auth: AuthKind::ApiKey,
        surface: Surface::ProviderKey,
        server_provider: Some("deepseek"),
        default_base_url: None,
        base_url: BaseUrlKind::Optional,
    },
    ProviderDescriptor {
        id: "fireworks",
        display_name: "Fireworks",
        aliases: &["fireworks-ai", "fireworks.ai"],
        detail: "API key · masked input",
        auth: AuthKind::ApiKey,
        surface: Surface::ProviderKey,
        server_provider: Some("fireworks"),
        default_base_url: None,
        base_url: BaseUrlKind::Optional,
    },
    ProviderDescriptor {
        id: "minimax",
        display_name: "MiniMax",
        aliases: &["minimaxi", "minimax-ai"],
        detail: "API key · masked input",
        auth: AuthKind::ApiKey,
        surface: Surface::ProviderKey,
        server_provider: Some("minimax"),
        default_base_url: None,
        base_url: BaseUrlKind::Optional,
    },
    ProviderDescriptor {
        id: "lmstudio",
        display_name: "LM Studio",
        aliases: &["lm-studio"],
        detail: "localhost:1234 · no API key",
        auth: AuthKind::LocalEndpoint,
        surface: Surface::Local,
        server_provider: None,
        default_base_url: Some("http://localhost:1234/v1"),
        base_url: BaseUrlKind::Optional,
    },
    ProviderDescriptor {
        id: "ollama",
        display_name: "Ollama",
        aliases: &[],
        detail: "localhost:11434 · no API key",
        auth: AuthKind::LocalEndpoint,
        surface: Surface::Local,
        server_provider: None,
        default_base_url: Some("http://localhost:11434/v1"),
        base_url: BaseUrlKind::Optional,
    },
    ProviderDescriptor {
        id: "openai-compatible",
        display_name: "OpenAI-compatible",
        aliases: &["openai_compatible", "compat", "custom"],
        detail: "custom API base · localhost may omit the key",
        auth: AuthKind::LocalEndpoint,
        surface: Surface::Local,
        server_provider: None,
        default_base_url: None,
        base_url: BaseUrlKind::Required,
    },
];

pub(crate) fn on_surface(surface: Surface) -> impl Iterator<Item = &'static ProviderDescriptor> {
    PROVIDERS
        .iter()
        .filter(move |provider| provider.surface == surface)
}

pub(crate) fn resolve(name: &str) -> Option<&'static ProviderDescriptor> {
    let name = name.trim();
    PROVIDERS.iter().find(|provider| {
        provider.id.eq_ignore_ascii_case(name)
            || provider
                .aliases
                .iter()
                .any(|alias| alias.eq_ignore_ascii_case(name))
    })
}

pub(crate) fn login_route(name: &str, supplied_base: Option<&str>) -> io::Result<LoginRoute> {
    let provider = resolve(name).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "unknown provider {}; no credential was requested",
                name.trim()
            ),
        )
    })?;
    let base_url = resolve_base_url(provider, supplied_base)?;
    let requires_key = match provider.auth {
        AuthKind::ApiKey => true,
        AuthKind::LocalEndpoint => !base_url.as_deref().is_some_and(is_local_url),
        AuthKind::ProviderOAuth | AuthKind::CopilotDevice => false,
    };
    Ok(LoginRoute {
        provider,
        base_url,
        requires_key,
    })
}

fn resolve_base_url(
    provider: &ProviderDescriptor,
    supplied_base: Option<&str>,
) -> io::Result<Option<String>> {
    let supplied_base = supplied_base
        .map(str::trim)
        .filter(|value| !value.is_empty());
    match (provider.base_url, supplied_base, provider.default_base_url) {
        (BaseUrlKind::None, Some(_), _) => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{} does not accept --base-url", provider.id),
        )),
        (BaseUrlKind::Required, None, None) => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "{} requires --base-url; no credential was requested",
                provider.id
            ),
        )),
        (_, Some(base), _) => normalize_base_url(base).map(Some),
        (_, None, Some(base)) => normalize_base_url(base).map(Some),
        _ => Ok(None),
    }
}

fn normalize_base_url(value: &str) -> io::Result<String> {
    let url = Url::parse(value).map_err(|_| invalid_base_url())?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || (url.scheme() == "http" && !is_local_host(url.host()))
    {
        return Err(invalid_base_url());
    }
    Ok(value.trim().trim_end_matches('/').to_string())
}

fn invalid_base_url() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        "provider base URL must be HTTPS, or HTTP on localhost/private network; credentials and query strings are not allowed",
    )
}

fn is_local_url(value: &str) -> bool {
    Url::parse(value)
        .ok()
        .is_some_and(|url| is_local_host(url.host()))
}

fn is_local_host(host: Option<Host<&str>>) -> bool {
    match host {
        Some(Host::Domain(host)) => {
            host.eq_ignore_ascii_case("localhost") || host.ends_with(".local")
        }
        Some(Host::Ipv4(ip)) => is_local_ip(IpAddr::V4(ip)),
        Some(Host::Ipv6(ip)) => is_local_ip(IpAddr::V6(ip)),
        None => false,
    }
}

fn is_local_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            ip.is_private()
                || ip.is_loopback()
                || ip.is_link_local()
                || (ip.octets()[0] == 100 && (64..=127).contains(&ip.octets()[1]))
        }
        IpAddr::V6(ip) => {
            ip.is_loopback()
                || (ip.segments()[0] & 0xfe00) == 0xfc00
                || (ip.segments()[0] & 0xffc0) == 0xfe80
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn catalog_covers_every_required_provider_with_unique_names() {
        let required = [
            "claude",
            "openai-api",
            "gemini",
            "copilot",
            "azure",
            "bedrock",
            "openrouter",
            "deepseek",
            "fireworks",
            "minimax",
            "lmstudio",
            "ollama",
            "openai-compatible",
        ];
        let mut names = HashSet::new();
        for provider in PROVIDERS {
            assert!(names.insert(provider.id), "duplicate id {}", provider.id);
            for alias in provider.aliases {
                assert!(names.insert(alias), "duplicate alias {alias}");
                assert_eq!(resolve(alias).map(|found| found.id), Some(provider.id));
            }
        }
        for id in required {
            assert_eq!(resolve(id).map(|provider| provider.id), Some(id));
        }
    }

    #[test]
    fn acquisition_kind_is_provider_data() {
        assert_eq!(resolve("claude").unwrap().auth, AuthKind::ProviderOAuth);
        assert!(resolve("openai").is_none());
        assert!(resolve("chatgpt").is_none());
        assert_eq!(resolve("copilot").unwrap().auth, AuthKind::CopilotDevice);
        assert_eq!(resolve("fireworks-ai").unwrap().id, "fireworks");
        assert_eq!(resolve("anthropic").unwrap().id, "anthropic-api");
        assert_eq!(resolve("openai-key").unwrap().id, "openai-api");
    }

    #[test]
    fn only_safe_local_http_endpoints_may_omit_a_key() {
        let lmstudio = login_route("lmstudio", None).expect("LM Studio route");
        assert_eq!(
            lmstudio.base_url.as_deref(),
            Some("http://localhost:1234/v1")
        );
        assert!(!lmstudio.requires_key);

        let custom = login_route("openai-compatible", Some("http://192.168.1.40:8000/v1/"))
            .expect("private endpoint");
        assert_eq!(
            custom.base_url.as_deref(),
            Some("http://192.168.1.40:8000/v1")
        );
        assert!(!custom.requires_key);

        assert!(login_route("openai-compatible", Some("http://example.com/v1")).is_err());
        assert!(login_route("openai-compatible", None).is_err());
        assert!(login_route("made-up-provider", None).is_err());
    }
}
