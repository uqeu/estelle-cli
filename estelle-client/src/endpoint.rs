use reqwest::Method;

use crate::Error;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HttpMethod {
    Get,
    Post,
    Put,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EndpointSpec {
    pub endpoint: Endpoint,
    pub path: &'static str,
    pub methods: &'static [HttpMethod],
    pub requires_repo: bool,
}

macro_rules! endpoints {
    ($(($variant:ident, $path:literal, [$($method:ident),+], $scoped:literal)),+ $(,)?) => {
        #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
        #[repr(usize)]
        pub enum Endpoint { $($variant),+ }

        pub const API_ENDPOINTS: &[EndpointSpec] = &[
            $(EndpointSpec {
                endpoint: Endpoint::$variant,
                path: $path,
                methods: &[$(HttpMethod::$method),+],
                requires_repo: $scoped,
            }),+
        ];
    };
}

endpoints! {
    (Account, "account", [Get], false),
    (Me, "me", [Get], false),
    (MeKeys, "me/keys", [Get], false),
    (MeTeam, "me/team", [Get], false),
    (MemoryCards, "memory/cards", [Get], false),
    (Providers, "providers", [Get], false),
    (ProviderSelect, "provider/select", [Post], false),
    (Overview, "overview", [Get], false),
    (Repos, "repos", [Get], false),
    (Session, "session", [Get], false),
    (Sessions, "sessions", [Get], false),
    (Skills, "skills", [Get], false),
    (Mcp, "mcp", [Post], false),
    (Instincts, "instincts", [Get], false),
    (Search, "search", [Post], true),
    (DeepSearch, "deep-search", [Post], true),
    (Verify, "verify", [Post], true),
    (Gate, "gate", [Post], true),
    (Scan, "scan", [Post], true),
    (Improve, "improve", [Post], true),
    (Work, "work", [Post], true),
    (Orchestra, "orchestra", [Post], true),
    (Route, "route", [Post], true),
    (SweepEstimate, "sweep/estimate", [Post], true),
    (GithubSweep, "github/sweep", [Post], false),
    (IngestStart, "ingest/start", [Post], true),
    (IngestProgress, "ingest/progress", [Get], true),
    (Reindex, "reindex", [Post], true),
    (Sync, "sync", [Post], true),
    (Checkpoint, "checkpoint", [Post], true),
    (Forget, "forget", [Post], true),
    (Retract, "retract", [Post], true),
    (Unlearn, "unlearn", [Post], false),
    (DeletionReceipts, "deletion-receipts", [Get], false),
    (Wiki, "wiki", [Get], true),
    (Graph, "graph", [Get], true),
    (GraphNodes, "graph/nodes", [Get], true),
    (SkillRun, "skill/run", [Post], true),
    (Autonomy, "autonomy", [Post], false),
    (AutonomyScope, "autonomy/scope", [Get, Post], true),
    (SettingsSuite, "settings/suite", [Get, Post], false),
    (ChatCompletions, "v1/chat/completions", [Post], true),
    (GithubAppSetup, "github/app/setup", [Post], false),
    (GithubAppCallback, "github/app/callback", [Get], false),
    (GithubIdentity, "github/identity", [Get], false),
    (GithubIdentityAuthorizeUrl, "github/identity/authorize-url", [Get], false),
    (GithubIdentityInstallations, "github/identity/installations", [Get], false),
    (GithubIdentityLink, "github/identity/link", [Post], false),
    (GithubRepos, "github/repos", [Get], false),
    (Issues, "issues", [Get], false),
    (MonitorOverview, "monitor/overview", [Get], false),
    (MonitorIssues, "monitor/issues", [Get], false),
    (MonitorIssue, "monitor/issue", [Post], false),
    (MonitorAlerts, "monitor/alerts", [Get], false),
    (MonitorLogs, "monitor/logs", [Get], false),
    (MonitorUptime, "monitor/uptime", [Get, Post], false),
    (VendorDrift, "vendor-drift", [Post], false),
    (VendorDriftWatchlist, "vendor-drift/watchlist", [Get, Put], false),
    (VendorDriftRepair, "vendor-drift/repair", [Post], false),
}

impl Endpoint {
    fn spec(self) -> &'static EndpointSpec {
        &API_ENDPOINTS[self as usize]
    }

    pub fn path(self) -> &'static str {
        self.spec().path
    }

    pub fn methods(self) -> &'static [HttpMethod] {
        self.spec().methods
    }

    pub fn requires_repo(self) -> bool {
        self.spec().requires_repo
    }

    pub(crate) fn validate_method(self, method: &Method) -> Result<(), Error> {
        let expected = match *method {
            Method::GET => HttpMethod::Get,
            Method::POST => HttpMethod::Post,
            Method::PUT => HttpMethod::Put,
            _ => {
                return Err(Error::UnsupportedMethod {
                    endpoint: self,
                    method: "unsupported method",
                });
            }
        };
        if self.methods().contains(&expected) {
            Ok(())
        } else {
            Err(Error::UnsupportedMethod {
                endpoint: self,
                method: match expected {
                    HttpMethod::Get => "GET",
                    HttpMethod::Post => "POST",
                    HttpMethod::Put => "PUT",
                },
            })
        }
    }
}
