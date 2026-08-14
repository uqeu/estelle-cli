use super::*;
use codex_http_client::RouteAwareRequestBuilder;
use codex_login::CodexAuth;
use codex_utils_absolute_path::AbsolutePathBuf;
use http::Method;
use http::StatusCode;
use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeMap;
use std::io;
use std::path::Path;
use tracing::warn;
use url::Url;

mod checkout;
mod local_paths;

pub use checkout::checkout_remote_plugin_share;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RemotePluginShareAccessPolicy {
    pub discoverability: Option<RemotePluginShareDiscoverability>,
    pub share_targets: Option<Vec<RemotePluginShareTarget>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RemotePluginShareDiscoverability {
    Listed,
    Unlisted,
    Private,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RemotePluginShareUpdateDiscoverability {
    Listed,
    Unlisted,
    Private,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RemotePluginSharePrincipalType {
    User,
    Group,
    Workspace,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemotePluginShareTarget {
    pub principal_type: RemotePluginSharePrincipalType,
    pub principal_id: String,
    pub role: RemotePluginShareTargetRole,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct RemotePluginSharePrincipal {
    pub principal_type: RemotePluginSharePrincipalType,
    pub principal_id: String,
    pub role: RemotePluginSharePrincipalRole,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RemotePluginShareTargetRole {
    Reader,
    Editor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RemotePluginSharePrincipalRole {
    Reader,
    Editor,
    Owner,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemotePluginShareUpdateTargetsResult {
    pub principals: Vec<RemotePluginSharePrincipal>,
    pub discoverability: RemotePluginShareDiscoverability,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct RemotePluginShareUpdateTargetsRequest {
    discoverability: RemotePluginShareUpdateDiscoverability,
    targets: Vec<RemotePluginShareTarget>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct RemotePluginShareUpdateTargetsResponse {
    principals: Vec<RemotePluginSharePrincipal>,
    discoverability: RemotePluginShareDiscoverability,
}

pub async fn list_remote_plugin_shares(
    config: &RemotePluginServiceConfig,
    auth: Option<&CodexAuth>,
    codex_home: &Path,
) -> Result<Vec<RemotePluginShareSummary>, RemotePluginCatalogError> {
    let auth = ensure_chatgpt_auth(auth)?;
    let created_plugins = fetch_created_workspace_plugins(config, auth).await?;
    if created_plugins.is_empty() {
        return Ok(Vec::new());
    }

    let installed_by_id =
        fetch_installed_plugins_for_scope(config, auth, RemotePluginScope::Workspace)
            .await?
            .into_iter()
            .map(|plugin| (plugin.plugin.id.clone(), plugin))
            .collect::<BTreeMap<_, _>>();
    let local_plugin_paths =
        local_paths::load_plugin_share_local_paths(codex_home).map_err(|err| {
            RemotePluginCatalogError::UnexpectedResponse(format!(
                "failed to load plugin share local path mapping: {err}"
            ))
        })?;

    created_plugins
        .into_iter()
        .map(|plugin| {
            let summary = build_remote_plugin_summary(&plugin, installed_by_id.get(&plugin.id))?;
            if summary
                .share_context
                .as_ref()
                .and_then(|context| context.share_principals.as_ref())
                .is_none()
            {
                return Err(RemotePluginCatalogError::UnexpectedResponse(format!(
                    "created workspace plugin `{}` did not include share_principals",
                    plugin.id
                )));
            }
            let local_plugin_path = local_plugin_paths.get(&plugin.id).cloned();
            Ok(RemotePluginShareSummary {
                summary,
                local_plugin_path,
            })
        })
        .collect()
}

pub fn load_plugin_share_remote_ids_by_local_path(
    codex_home: &Path,
) -> io::Result<BTreeMap<AbsolutePathBuf, String>> {
    let local_paths = local_paths::load_plugin_share_local_paths(codex_home)?;
    local_paths
        .into_iter()
        .map(|(remote_plugin_id, local_plugin_path)| {
            if !is_valid_remote_plugin_id(&remote_plugin_id) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "invalid remote plugin id in share local path mapping: {remote_plugin_id}"
                    ),
                ));
            }
            Ok((local_plugin_path, remote_plugin_id))
        })
        .collect()
}

pub async fn delete_remote_plugin_share(
    config: &RemotePluginServiceConfig,
    auth: Option<&CodexAuth>,
    codex_home: &Path,
    remote_plugin_id: &str,
) -> Result<(), RemotePluginCatalogError> {
    let auth = ensure_chatgpt_auth(auth)?;
    let base_url = config.chatgpt_base_url.trim_end_matches('/');
    let url = format!("{base_url}/public/plugins/workspace/{remote_plugin_id}");
    let request = authenticated_request(config.http_request(Method::DELETE, &url), auth);
    send_and_expect_status(request, &url, &[StatusCode::NO_CONTENT]).await?;
    if let Err(err) = local_paths::remove_plugin_share_local_path(codex_home, remote_plugin_id) {
        warn!(
            remote_plugin_id = %remote_plugin_id,
            "failed to remove plugin share local path mapping: {err}"
        );
    }
    Ok(())
}

pub async fn update_remote_plugin_share_targets(
    config: &RemotePluginServiceConfig,
    auth: Option<&CodexAuth>,
    remote_plugin_id: &str,
    targets: Vec<RemotePluginShareTarget>,
    discoverability: RemotePluginShareUpdateDiscoverability,
) -> Result<RemotePluginShareUpdateTargetsResult, RemotePluginCatalogError> {
    let auth = ensure_chatgpt_auth(auth)?;
    let target_discoverability = match discoverability {
        RemotePluginShareUpdateDiscoverability::Listed => RemotePluginShareDiscoverability::Listed,
        RemotePluginShareUpdateDiscoverability::Unlisted => {
            RemotePluginShareDiscoverability::Unlisted
        }
        RemotePluginShareUpdateDiscoverability::Private => {
            RemotePluginShareDiscoverability::Private
        }
    };
    let targets =
        ensure_unlisted_workspace_target(auth, Some(target_discoverability), Some(targets))?
            .unwrap_or_default();
    let base_url = config.chatgpt_base_url.trim_end_matches('/');
    let url = format!("{base_url}/ps/plugins/{remote_plugin_id}/shares");
    let request = authenticated_request(config.http_request(Method::PUT, &url), auth).json(
        &RemotePluginShareUpdateTargetsRequest {
            discoverability,
            targets,
        },
    );
    let response: RemotePluginShareUpdateTargetsResponse = send_and_decode(request, &url).await?;
    Ok(RemotePluginShareUpdateTargetsResult {
        principals: response.principals,
        discoverability: response.discoverability,
    })
}

fn ensure_unlisted_workspace_target(
    auth: &CodexAuth,
    discoverability: Option<RemotePluginShareDiscoverability>,
    targets: Option<Vec<RemotePluginShareTarget>>,
) -> Result<Option<Vec<RemotePluginShareTarget>>, RemotePluginCatalogError> {
    if discoverability != Some(RemotePluginShareDiscoverability::Unlisted) {
        return Ok(targets);
    }
    let account_id = auth.get_account_id().ok_or_else(|| {
        RemotePluginCatalogError::UnexpectedResponse(
            "workspace plugin share requires an account id".to_string(),
        )
    })?;
    let mut targets = targets.unwrap_or_default();
    if !targets.iter().any(|target| {
        target.principal_type == RemotePluginSharePrincipalType::Workspace
            && target.principal_id == account_id
    }) {
        targets.push(RemotePluginShareTarget {
            principal_type: RemotePluginSharePrincipalType::Workspace,
            principal_id: account_id,
            role: RemotePluginShareTargetRole::Reader,
        });
    }
    Ok(Some(targets))
}

async fn fetch_created_workspace_plugins(
    config: &RemotePluginServiceConfig,
    auth: &CodexAuth,
) -> Result<Vec<RemotePluginDirectoryItem>, RemotePluginCatalogError> {
    let mut plugins = Vec::new();
    let mut page_token = None;
    loop {
        let response =
            get_created_workspace_plugins_page(config, auth, page_token.as_deref()).await?;
        plugins.extend(response.plugins);
        let Some(next_page_token) = response.pagination.next_page_token else {
            break;
        };
        page_token = Some(next_page_token);
    }
    Ok(plugins)
}

async fn get_created_workspace_plugins_page(
    config: &RemotePluginServiceConfig,
    auth: &CodexAuth,
    page_token: Option<&str>,
) -> Result<RemotePluginListResponse, RemotePluginCatalogError> {
    let base_url = config.chatgpt_base_url.trim_end_matches('/');
    let mut url = Url::parse(&format!("{base_url}/ps/plugins/workspace/created"))
        .map_err(RemotePluginCatalogError::InvalidBaseUrl)?;
    url.query_pairs_mut()
        .append_pair("limit", &REMOTE_PLUGIN_LIST_PAGE_LIMIT.to_string());
    if let Some(page_token) = page_token {
        url.query_pairs_mut().append_pair("pageToken", page_token);
    }
    let url = url.to_string();
    let request = authenticated_request(config.http_request(Method::GET, &url), auth);
    send_and_decode(request, &url).await
}

async fn send_and_expect_status(
    request: RouteAwareRequestBuilder,
    url_for_error: &str,
    expected_statuses: &[StatusCode],
) -> Result<(), RemotePluginCatalogError> {
    let response = request
        .send()
        .await
        .map_err(|source| RemotePluginCatalogError::Request {
            url: url_for_error.to_string(),
            source,
        })?;
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if !expected_statuses.contains(&status) {
        return Err(RemotePluginCatalogError::UnexpectedStatus {
            url: url_for_error.to_string(),
            status,
            body,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests;
