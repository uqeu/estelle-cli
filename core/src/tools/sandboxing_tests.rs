use super::*;
use crate::sandboxing::SandboxPermissions;
use crate::tools::hook_names::HookToolName;
use codex_network_proxy::ManagedNetworkSandboxContext;
use codex_protocol::permissions::FileSystemAccessMode;
use codex_protocol::permissions::FileSystemPath;
use codex_protocol::permissions::FileSystemSandboxEntry;
use codex_protocol::protocol::GranularApprovalConfig;
use codex_sandboxing::SandboxCommand;
use codex_sandboxing::SandboxManager;
use codex_sandboxing::SandboxType;
use codex_utils_absolute_path::AbsolutePathBuf;
use codex_utils_path_uri::PathUri;
use pretty_assertions::assert_eq;
use serde_json::json;
use std::collections::HashMap;

#[test]
fn bash_permission_request_payload_omits_missing_description() {
    assert_eq!(
        PermissionRequestPayload::bash("echo hi".to_string(), /*description*/ None),
        PermissionRequestPayload {
            tool_name: HookToolName::bash(),
            tool_input: json!({ "command": "echo hi" }),
        }
    );
}

#[test]
fn bash_permission_request_payload_includes_description_when_present() {
    assert_eq!(
        PermissionRequestPayload::bash(
            "echo hi".to_string(),
            Some("network-access example.com".to_string()),
        ),
        PermissionRequestPayload {
            tool_name: HookToolName::bash(),
            tool_input: json!({
                "command": "echo hi",
                "description": "network-access example.com",
            }),
        }
    );
}

#[test]
fn external_sandbox_skips_exec_approval_on_request() {
    assert_eq!(
        default_exec_approval_requirement(
            AskForApproval::OnRequest,
            &FileSystemSandboxPolicy::external_sandbox(),
        ),
        ExecApprovalRequirement::Skip {
            bypass_sandbox: false,
            proposed_execpolicy_amendment: None,
        }
    );
}

#[test]
fn restricted_sandbox_requires_exec_approval_on_request() {
    assert_eq!(
        default_exec_approval_requirement(
            AskForApproval::OnRequest,
            &FileSystemSandboxPolicy::default()
        ),
        ExecApprovalRequirement::NeedsApproval {
            reason: None,
            proposed_execpolicy_amendment: None,
        }
    );
}

#[test]
fn default_exec_approval_requirement_rejects_sandbox_prompt_when_granular_disables_it() {
    let policy = AskForApproval::Granular(GranularApprovalConfig {
        sandbox_approval: false,
        rules: true,
        skill_approval: true,
        request_permissions: true,
        mcp_elicitations: true,
    });

    let requirement =
        default_exec_approval_requirement(policy, &FileSystemSandboxPolicy::default());

    assert_eq!(
        requirement,
        ExecApprovalRequirement::Forbidden {
            reason: "approval policy disallowed sandbox approval prompt".to_string(),
        }
    );
}

#[test]
fn default_exec_approval_requirement_keeps_prompt_when_granular_allows_sandbox_approval() {
    let policy = AskForApproval::Granular(GranularApprovalConfig {
        sandbox_approval: true,
        rules: false,
        skill_approval: true,
        request_permissions: true,
        mcp_elicitations: false,
    });

    let requirement =
        default_exec_approval_requirement(policy, &FileSystemSandboxPolicy::default());

    assert_eq!(
        requirement,
        ExecApprovalRequirement::NeedsApproval {
            reason: None,
            proposed_execpolicy_amendment: None,
        }
    );
}

#[test]
fn additional_permissions_allow_bypass_sandbox_first_attempt_when_execpolicy_skips() {
    assert_eq!(
        sandbox_override_for_first_attempt(
            SandboxPermissions::WithAdditionalPermissions,
            &ExecApprovalRequirement::Skip {
                bypass_sandbox: true,
                proposed_execpolicy_amendment: None,
            },
            &FileSystemSandboxPolicy::default(),
        ),
        SandboxOverride::BypassSandboxFirstAttempt
    );
}

#[test]
fn guardian_bypasses_sandbox_for_explicit_escalation_on_first_attempt() {
    assert_eq!(
        sandbox_override_for_first_attempt(
            SandboxPermissions::RequireEscalated,
            &ExecApprovalRequirement::Skip {
                bypass_sandbox: false,
                proposed_execpolicy_amendment: None,
            },
            &FileSystemSandboxPolicy::default(),
        ),
        SandboxOverride::BypassSandboxFirstAttempt
    );
}

#[test]
fn deny_read_blocks_explicit_escalation_and_policy_bypass() {
    let file_system_policy = FileSystemSandboxPolicy::restricted(vec![FileSystemSandboxEntry {
        path: FileSystemPath::GlobPattern {
            pattern: "**/*.env".to_string(),
        },
        access: FileSystemAccessMode::Deny,
        missing_path_behavior: None,
    }]);

    assert_eq!(
        sandbox_override_for_first_attempt(
            SandboxPermissions::RequireEscalated,
            &ExecApprovalRequirement::Skip {
                bypass_sandbox: false,
                proposed_execpolicy_amendment: None,
            },
            &file_system_policy,
        ),
        SandboxOverride::NoOverride,
        "explicit escalation would drop deny-read filesystem policy, so keep the first attempt sandboxed",
    );
    assert!(!unsandboxed_execution_allowed(&file_system_policy));
    assert_eq!(
        sandbox_permissions_preserving_denied_reads(
            SandboxPermissions::RequireEscalated,
            &file_system_policy,
        ),
        SandboxPermissions::UseDefault,
    );
    assert_eq!(
        sandbox_permissions_preserving_denied_reads(
            SandboxPermissions::WithAdditionalPermissions,
            &file_system_policy,
        ),
        SandboxPermissions::WithAdditionalPermissions,
    );
    assert_eq!(
        sandbox_permissions_preserving_denied_reads(
            SandboxPermissions::RequireEscalated,
            &FileSystemSandboxPolicy::default(),
        ),
        SandboxPermissions::RequireEscalated,
    );
    assert_eq!(
        sandbox_override_for_first_attempt(
            SandboxPermissions::WithAdditionalPermissions,
            &ExecApprovalRequirement::Skip {
                bypass_sandbox: true,
                proposed_execpolicy_amendment: None,
            },
            &file_system_policy,
        ),
        SandboxOverride::NoOverride,
        "exec-policy allow rules would drop deny-read filesystem policy, so keep the first attempt sandboxed",
    );
}

#[test]
fn exec_server_env_keeps_command_native_and_carries_sandbox_context() {
    let cwd: AbsolutePathBuf = std::env::current_dir()
        .expect("current dir")
        .try_into()
        .expect("absolute cwd");
    let cwd_uri = PathUri::from_abs_path(&cwd);
    let exec_server_permissions = codex_protocol::models::PermissionProfile::workspace_write();
    let permissions = exec_server_permissions
        .clone()
        .materialize_project_roots_with_workspace_roots(std::slice::from_ref(&cwd));
    let manager = SandboxManager::new();
    let mut attempt = SandboxAttempt {
        sandbox: SandboxType::None,
        sandbox_requested: true,
        permissions: &permissions,
        exec_server_permissions: &exec_server_permissions,
        enforce_managed_network: true,
        manager: &manager,
        sandbox_cwd: &cwd_uri,
        workspace_roots: std::slice::from_ref(&cwd_uri),
        codex_linux_sandbox_exe: None,
        use_legacy_landlock: false,
        windows_sandbox_level: codex_protocol::config_types::WindowsSandboxLevel::Disabled,
        windows_sandbox_private_desktop: false,
        network_denial_cancellation_token: None,
        network_proxy: None,
    };
    let managed_network = ManagedNetworkSandboxContext {
        loopback_ports: vec![43123],
        allow_local_binding: false,
    };
    let command = || SandboxCommand {
        program: "/bin/bash".into(),
        args: vec!["-lc".to_string(), "pwd".to_string()],
        cwd: cwd_uri.clone(),
        env: HashMap::new(),
        managed_network: Some(managed_network.clone()),
        additional_permissions: None,
    };
    let options = || crate::sandboxing::ExecOptions {
        expiration: crate::exec::ExecExpiration::DefaultTimeout,
        capture_policy: crate::exec::ExecCapturePolicy::ShellTool,
    };
    let request = attempt
        .env_for_exec_server(command(), options())
        .expect("prepare remote exec request");

    assert_eq!(
        request.command,
        vec![
            "/bin/bash".to_string(),
            "-lc".to_string(),
            "pwd".to_string()
        ]
    );
    assert_eq!(request.arg0, None);
    assert_eq!(request.sandbox, SandboxType::None);
    assert_eq!(
        request.exec_server_sandbox,
        Some(codex_exec_server::FileSystemSandboxContext {
            permissions: exec_server_permissions.clone().into(),
            cwd: Some(cwd_uri.clone()),
            workspace_roots: vec![cwd_uri.clone()],
            windows_sandbox_level: codex_protocol::config_types::WindowsSandboxLevel::Disabled,
            windows_sandbox_private_desktop: false,
            windows_sandbox_proxy_settings_mode: None,
            use_legacy_landlock: false,
        })
    );
    assert!(request.exec_server_enforce_managed_network);
    assert_eq!(
        request.exec_server_managed_network,
        Some(managed_network.clone())
    );

    attempt.sandbox_requested = false;
    let request = attempt
        .env_for_exec_server(command(), options())
        .expect("prepare unsandboxed remote exec request");

    assert_eq!(request.exec_server_sandbox, None);
    assert!(!request.exec_server_enforce_managed_network);
    assert_eq!(request.exec_server_managed_network, Some(managed_network));
}

// ---------------------------------------------------------------------------
// Ported from the rival-CLI teardown, take-list #3 and #10.
//
// jcode's `Decision` carries a NON-OPTIONAL `decided_via: String`
// (`crates/jcode-base/src/safety.rs:76-84`, MIT, Copyright (c) 2025 Jeremy
// Huang), and kimi-cli records `approved_via_session_cache: bool` on every
// approval record (`src/kimi_cli/approval_runtime/models.py:37`, Apache-2.0).
//
// Both exist for the same reason: an audit that cannot tell "a human was
// asked" from "a prior always answered" is not an audit. Before this change
// `with_cached_approval` emitted `codex.approval.requested` ONLY on the branch
// that actually prompted, so a cache hit was invisible: the counter
// under-reported approvals and no field distinguished the two.
// ---------------------------------------------------------------------------

/// A cache hit and a prompt are different events and must not be reported as
/// the same one.
#[tokio::test]
async fn cache_hit_and_prompt_are_distinguishable_in_the_audit() {
    let store = Mutex::new(ApprovalStore::default());
    let keys = vec!["cmd::rm -rf build".to_string()];

    // First call: nobody has answered yet, so the human is asked.
    let first = resolve_cached_approval(&store, keys.clone(), || async {
        ReviewDecision::ApprovedForSession
    })
    .await;
    assert_eq!(ApprovalDecidedVia::Prompt, first.decided_via);
    assert_eq!(ReviewDecision::ApprovedForSession, first.decision);

    // Second call: answered by the stored "always". Same decision, DIFFERENT
    // provenance -- and the fetch closure must not run at all.
    let second = resolve_cached_approval(&store, keys, || async {
        panic!("a cached approval must not re-prompt the human");
    })
    .await;
    assert_eq!(ApprovalDecidedVia::SessionCache, second.decided_via);
    assert_eq!(ReviewDecision::ApprovedForSession, second.decision);

    assert_ne!(
        first.decided_via, second.decided_via,
        "an approval answered by a prior 'always' must not be recorded \
         identically to one a human was actually asked"
    );
}

/// The exemption shape: only `ApprovedForSession` is cached. A one-off
/// approval, a denial and a timeout must each re-prompt, and must each be
/// recorded as `Prompt` -- never silently promoted into a stored always.
#[tokio::test]
async fn non_session_decisions_are_never_served_from_the_cache() {
    for decision in [
        ReviewDecision::Approved,
        ReviewDecision::denied("no"),
        ReviewDecision::TimedOut,
        ReviewDecision::Abort,
    ] {
        let store = Mutex::new(ApprovalStore::default());
        let keys = vec!["cmd::curl".to_string()];

        let first = resolve_cached_approval(&store, keys.clone(), {
            let decision = decision.clone();
            || async move { decision }
        })
        .await;
        assert_eq!(ApprovalDecidedVia::Prompt, first.decided_via);

        let second = resolve_cached_approval(&store, keys, {
            let decision = decision.clone();
            || async move { decision }
        })
        .await;
        assert_eq!(
            ApprovalDecidedVia::Prompt,
            second.decided_via,
            "{decision:?} must not have been cached"
        );
    }
}

/// Empty keys are the defensive path: nothing to look up, so the decision is
/// always a fresh prompt and is never attributed to a cache.
#[tokio::test]
async fn empty_keys_are_reported_as_a_prompt_not_a_cache_hit() {
    let store = Mutex::new(ApprovalStore::default());
    let outcome = resolve_cached_approval(&store, Vec::<String>::new(), || async {
        ReviewDecision::Approved
    })
    .await;
    assert_eq!(ApprovalDecidedVia::Prompt, outcome.decided_via);
}

/// `decided_via` is the string that reaches telemetry; pin the wire values so a
/// rename cannot silently split a dashboard in two.
#[test]
fn decided_via_wire_values_are_stable() {
    assert_eq!("prompt", ApprovalDecidedVia::Prompt.as_str());
    assert_eq!("session_cache", ApprovalDecidedVia::SessionCache.as_str());
}
