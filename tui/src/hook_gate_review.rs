//! Admission over the existing `/gate` contract, not snippet `/verify` success.
//!
//! One request per changed path avoids promoting the server's diff-global count
//! to proof that every file in a multifile action was ruled on. Each request still
//! carries the whole candidate overlay, so sibling definitions retain their scope.
use crate::hook_edit_input::GateFile;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::time::Duration;

pub(crate) fn request(file: &GateFile, head: &str, sources: &BTreeMap<String, String>) -> Value {
    json!({"diff":file.diff,"head":head,"require_verified":true,"sources":sources,
        "working_tree":{"base":head,"changed":sources.keys().collect::<Vec<_>>(),"sources":sources}})
}

fn request_with_deletions(
    file: &GateFile,
    head: &str,
    sources: &BTreeMap<String, String>,
    deleted: &[String],
) -> Value {
    let mut body = request(file, head, sources);
    if !deleted.is_empty() {
        let changed = sources
            .keys()
            .cloned()
            .chain(deleted.iter().cloned())
            .collect::<std::collections::BTreeSet<_>>();
        body["working_tree"]["changed"] = json!(changed);
        body["working_tree"]["deleted"] = json!(deleted);
    }
    body
}

fn exact_array(value: Option<&Value>, expected: &[&str]) -> bool {
    value.and_then(Value::as_array).is_some_and(|rows| {
        rows.len() == expected.len()
            && rows
                .iter()
                .zip(expected)
                .all(|(row, expect)| row.as_str() == Some(expect))
    })
}
fn empty(value: Option<&Value>) -> bool {
    value.and_then(Value::as_array).is_some_and(Vec::is_empty)
}
fn head_matches(value: Option<&Value>, head: &str) -> bool {
    value.and_then(Value::as_str).is_some_and(|sha| {
        sha.len() >= 12
            && sha.len() <= head.len()
            && sha.bytes().all(|byte| byte.is_ascii_hexdigit())
            && head.starts_with(sha)
    })
}
fn count(report: &Value, key: &str) -> Result<u64, String> {
    report
        .pointer(&format!("/counts/{key}"))
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("gate omitted the {key} evidence count"))
}
fn detail(report: &Value) -> String {
    report
        .get("blockers")
        .and_then(Value::as_array)
        .and_then(|rows| rows.first())
        .and_then(|row| row.get("body"))
        .and_then(Value::as_str)
        .unwrap_or("the gate could not establish a verified ruling for this change")
        .chars()
        .take(3000)
        .collect()
}

#[cfg(test)]
pub(crate) fn accept(
    report: &Value,
    path: &str,
    repo: &str,
    head: &str,
    source_count: usize,
) -> Result<(), String> {
    accept_with_deletions(report, path, repo, head, source_count, &[])
}
fn accept_with_deletions(
    report: &Value,
    path: &str,
    repo: &str,
    head: &str,
    source_count: usize,
    deleted: &[String],
) -> Result<(), String> {
    if report.get("repo").and_then(Value::as_str) != Some(repo)
        || report.get("scope").and_then(Value::as_str) != Some(format!("repo:{repo}").as_str())
        || report.get("scope_degraded") == Some(&Value::Bool(true))
    {
        return Err("the gate did not confirm the requested repository scope".into());
    }
    if report.get("merge") != Some(&Value::Bool(true))
        || report.get("verified") != Some(&Value::Bool(true))
        || report.get("state").and_then(Value::as_str) != Some("verified-clean")
        || report.get("outcome").and_then(Value::as_str) != Some("verified")
        || !matches!(
            report.get("verdict").and_then(Value::as_str),
            Some("clean" | "clean-with-warnings")
        )
        || !empty(report.get("blockers"))
    {
        return Err(detail(report));
    }
    let currency = report
        .get("currency")
        .ok_or("the gate supplied no commit provenance")?;
    if currency.get("agrees") != Some(&Value::Bool(true))
        || currency.get("certifiable") != Some(&Value::Bool(true))
        || !head_matches(currency.get("indexed_head"), head)
        || !head_matches(currency.get("asked_head"), head)
    {
        return Err(
            "the gate evidence is not bound to this checkout HEAD; reindex the requested commit"
                .into(),
        );
    }
    let coverage = report
        .get("coverage")
        .ok_or("the gate omitted per-path coverage")?;
    if !exact_array(coverage.get("paths"), &[path]) {
        return Err("the gate did not account for the exact candidate path".into());
    }
    let categories = ["inert", "out_of_domain", "needs_verdict"];
    let mut owed = false;
    let mut memberships = 0;
    for key in categories {
        if exact_array(coverage.get(key), &[path]) {
            memberships += 1;
            owed |= key == "needs_verdict";
        } else if !empty(coverage.get(key)) {
            return Err("the gate's coverage ledger is malformed or partial".into());
        }
    }
    if memberships != 1 || count(report, "evidence")? != 1 {
        return Err("the gate did not produce one complete per-file ruling".into());
    }
    let verdicts = count(report, "api_verdicts")?;
    if (owed && verdicts == 0)
        || count(report, "api_checks")? > verdicts
        || count(report, "symbols_seen")? > verdicts
    {
        return Err("the gate observed API references it did not adjudicate".into());
    }
    if report
        .get("unverified_reasons")
        .is_some_and(|v| !empty(Some(v)))
        || report
            .get("ruleset_sources_dropped")
            .is_some_and(|v| !empty(Some(v)))
    {
        return Err("the gate disclosed incomplete checks or dropped source evidence".into());
    }
    let overlay = report
        .get("working_tree")
        .ok_or("the gate did not acknowledge the candidate overlay")?;
    if overlay.get("complete") != Some(&Value::Bool(true))
        || !empty(overlay.get("dropped"))
        || !empty(overlay.get("undelivered"))
        || overlay.get("files").and_then(Value::as_u64) != Some(source_count as u64)
        || overlay.get("declared").and_then(Value::as_u64)
            != Some((source_count + deleted.len()) as u64)
    {
        return Err("the gate did not consume the complete candidate working-tree overlay".into());
    }
    if !deleted.is_empty() {
        let expected = deleted.iter().map(String::as_str).collect::<Vec<_>>();
        if overlay.get("schema_version").and_then(Value::as_u64) != Some(2)
            || !exact_array(overlay.get("deleted"), &expected)
        {
            return Err("the server did not acknowledge exact deletion tombstones under working_tree schema_version 2; upgrade the server before retrying this combination".into());
        }
    }
    Ok(())
}

#[cfg(test)]
pub(crate) async fn check<F, Fut>(
    files: &[GateFile],
    repo: &str,
    head: &str,
    sources: &BTreeMap<String, String>,
    timeout: Duration,
    send: F,
) -> Result<usize, String>
where
    F: FnMut(Value) -> Fut,
    Fut: std::future::Future<Output = Result<Value, String>>,
{
    check_with_deletions(files, repo, head, sources, &[], timeout, send).await
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn check_with_deletions<F, Fut>(
    files: &[GateFile],
    repo: &str,
    head: &str,
    sources: &BTreeMap<String, String>,
    deleted: &[String],
    timeout: Duration,
    mut send: F,
) -> Result<usize, String>
where
    F: FnMut(Value) -> Fut,
    Fut: std::future::Future<Output = Result<Value, String>>,
{
    let future = async {
        let mut checked = 0;
        for file in files
            .iter()
            .filter(|file| file.has_added_code || file.candidate_diff.is_some())
        {
            // The actual added-line diff and the complete candidate artifact are
            // separate checks sharing one deadline. Never persist or describe the
            // synthetic full-postimage diff as the user's actual patch.
            if file.has_added_code {
                let report = send(request_with_deletions(file, head, sources, deleted))
                    .await
                    .map_err(|detail| {
                        format!("the gate request failed: {detail}; the action remains UNVERIFIED")
                    })?;
                accept_with_deletions(&report, &file.path, repo, head, sources.len(), deleted)
                    .map_err(|reason| format!("{} (actual diff): {reason}", file.path))?;
            }
            if let Some(diff) = &file.candidate_diff {
                let mut body = request_with_deletions(file, head, sources, deleted);
                body["diff"] = json!(diff);
                let report = send(body).await.map_err(|detail| {
                    format!(
                        "the candidate-artifact gate failed: {detail}; no approval was established"
                    )
                })?;
                accept_with_deletions(&report,&file.path,repo,head,sources.len(),deleted).map_err(|reason|
                    format!("{} (complete candidate artifact, not necessarily a newly introduced defect): {reason}",file.path))?;
            }
            checked += 1;
        }
        Ok(checked)
    };
    tokio::time::timeout(timeout, future).await.map_err(|_| {
        "the complete gate check timed out; no part of this action is approved".to_string()
    })?
}

#[cfg(test)]
mod tests {
    use super::*;
    const HEAD: &str = "0123456789abcdef0123456789abcdef01234567";
    fn clean(path: &str) -> Value {
        json!({"repo":"acme/demo","scope":"repo:acme/demo","merge":true,"verified":true,"state":"verified-clean","outcome":"verified","verdict":"clean","blockers":[],
        "currency":{"agrees":true,"certifiable":true,"indexed_head":&HEAD[..12],"asked_head":&HEAD[..12]},
        "coverage":{"paths":[path],"inert":[],"out_of_domain":[],"needs_verdict":[path]},
        "counts":{"evidence":1,"api_checks":1,"api_verdicts":1,"symbols_seen":1},
        "working_tree":{"complete":true,"files":2,"declared":2,"dropped":[],"undelivered":[]}})
    }
    fn file(path: &str) -> GateFile {
        GateFile {
            path: path.into(),
            diff: format!(
                "diff --git a/{path} b/{path}\n--- a/{path}\n+++ b/{path}\n@@ -0,0 +1 @@\n+real()\n"
            ),
            has_added_code: true,
            candidate_diff: None,
        }
    }
    #[test]
    fn normal_edit_is_allowed_only_with_scoped_complete_evidence() {
        assert!(accept(&clean("a.py"), "a.py", "acme/demo", HEAD, 2).is_ok());
    }
    #[test]
    fn new_definitions_can_be_out_of_domain_without_fake_reference_count() {
        let mut v = clean("a.py");
        v["coverage"]["needs_verdict"] = json!([]);
        v["coverage"]["out_of_domain"] = json!(["a.py"]);
        for key in ["api_checks", "api_verdicts", "symbols_seen"] {
            v["counts"][key] = json!(0);
        }
        assert!(accept(&v, "a.py", "acme/demo", HEAD, 2).is_ok());
    }
    #[test]
    fn merge_alone_legacy_false_scope_stale_partial_and_dropped_all_pause() {
        for (pointer, value) in [
            ("/verified", json!(false)),
            ("/repo", json!("other/repo")),
            ("/scope_degraded", json!(true)),
            ("/currency/agrees", json!(false)),
            ("/currency/indexed_head", json!("f".repeat(12))),
            ("/coverage/paths", json!(["other.py"])),
            ("/counts/symbols_seen", json!(2)),
            ("/working_tree/complete", json!(false)),
            ("/working_tree/dropped", json!(["a.py"])),
        ] {
            let mut v = clean("a.py");
            if let Some(slot) = v.pointer_mut(pointer) {
                *slot = value;
            } else {
                v["scope_degraded"] = value;
            }
            assert!(
                accept(&v, "a.py", "acme/demo", HEAD, 2).is_err(),
                "{pointer}"
            );
        }
        assert!(accept(&json!({"merge":true}), "a.py", "acme/demo", HEAD, 2).is_err());
    }
    #[tokio::test]
    async fn multifile_action_needs_every_response_and_preserves_postimages() {
        let files = [file("a.py"), file("b.py")];
        let sources = BTreeMap::from([
            ("a.py".into(), "  first\n".into()),
            ("b.py".into(), "second".into()),
        ]);
        let mut seen = 0;
        assert_eq!(
            check(
                &files,
                "acme/demo",
                HEAD,
                &sources,
                Duration::from_secs(1),
                |body| {
                    seen += 1;
                    assert_eq!(body["sources"]["a.py"], "  first\n");
                    assert_eq!(body["sources"]["b.py"], "second");
                    let mut reply = clean(if seen == 1 { "a.py" } else { "b.py" });
                    if seen == 2 {
                        reply["merge"] = json!(false);
                        reply["blockers"] =
                            json!([{"body":"confirmed nonexistent API; use real()"}]);
                    }
                    async move { Ok(reply) }
                }
            )
            .await
            .unwrap_err(),
            "b.py (actual diff): confirmed nonexistent API; use real()"
        );
        assert_eq!(seen, 2);
    }
    #[tokio::test]
    async fn timeout_is_unverified_and_no_suffix_is_approved() {
        let error = check(
            &[file("a.py")],
            "acme/demo",
            HEAD,
            &BTreeMap::new(),
            Duration::from_millis(1),
            |_| async { std::future::pending::<Result<Value, String>>().await },
        )
        .await
        .unwrap_err();
        assert!(error.contains("timed out"));
    }
    #[tokio::test]
    async fn deletion_has_no_introduction_obligation_and_never_calls_gate() {
        let mut f = file("gone.py");
        f.has_added_code = false;
        assert_eq!(
            check(
                &[f],
                "acme/demo",
                HEAD,
                &BTreeMap::new(),
                Duration::from_secs(1),
                |_| async { panic!("deletion is not a new API call") }
            )
            .await
            .unwrap(),
            0
        );
    }
    #[test]
    fn existing_server_handler_fixtures_accept_real_api_and_new_definition_but_refuse_invented_api()
    {
        let fixtures: Value =
            serde_json::from_str(include_str!("../tests/fixtures/native-gate-contract.json"))
                .unwrap();
        for (name, allowed) in [
            ("valid", true),
            ("new_definition", true),
            ("typescript_definition", true),
            ("blocked", false),
        ] {
            let case = &fixtures["cases"][name];
            let req = &case["request"];
            let path = req["working_tree"]["changed"][0].as_str().unwrap();
            assert_eq!(
                accept(
                    &case["response"],
                    path,
                    req["repo"].as_str().unwrap(),
                    req["head"].as_str().unwrap(),
                    1
                )
                .is_ok(),
                allowed,
                "{name}"
            );
        }
    }
    #[test]
    fn actual_handler_tombstone_contract_blocks_all_deleted_import_forms_and_allows_renames() {
        let fixtures: Value = serde_json::from_str(include_str!(
            "../tests/fixtures/tombstone-gate-contract.json"
        ))
        .unwrap();
        for case in fixtures["fixtures"].as_array().unwrap() {
            let name = case["name"].as_str().unwrap();
            let req = &case["request"];
            let deleted = req["working_tree"]["deleted"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().to_string())
                .collect::<Vec<_>>();
            let path = case["response"]["coverage"]["paths"][0].as_str().unwrap();
            let expected = matches!(
                name,
                "rename_new_module"
                    | "rename_new_symbol"
                    | "parent_export_is_valid"
                    | "real_empty_module"
            );
            assert_eq!(
                accept_with_deletions(
                    &case["response"],
                    path,
                    req["repo"].as_str().unwrap(),
                    req["head"].as_str().unwrap(),
                    req["sources"].as_object().unwrap().len(),
                    &deleted
                )
                .is_ok(),
                expected,
                "{name}"
            );
            if expected && !deleted.is_empty() {
                for field in ["schema_version", "deleted"] {
                    let mut missing = case["response"].clone();
                    missing["working_tree"]
                        .as_object_mut()
                        .unwrap()
                        .remove(field);
                    assert!(
                        accept_with_deletions(
                            &missing,
                            path,
                            req["repo"].as_str().unwrap(),
                            req["head"].as_str().unwrap(),
                            req["sources"].as_object().unwrap().len(),
                            &deleted
                        )
                        .is_err()
                    );
                }
            }
        }
    }
    #[test]
    fn actual_handler_full_candidate_closes_quoted_code_activation_that_actual_added_lines_miss() {
        let fixtures: Value = serde_json::from_str(include_str!(
            "../tests/fixtures/activation-gate-contract.json"
        ))
        .unwrap();
        for (name, expected) in [("actual_diff", true), ("candidate_artifact", false)] {
            let case = &fixtures[name];
            let req = &case["request"];
            assert_eq!(
                accept(
                    &case["response"],
                    "src/activated.py",
                    req["repo"].as_str().unwrap(),
                    req["head"].as_str().unwrap(),
                    1
                )
                .is_ok(),
                expected,
                "{name}"
            );
        }
    }
    #[tokio::test]
    async fn actual_and_candidate_rulings_share_one_total_deadline() {
        let mut candidate = file("a.py");
        candidate.candidate_diff = Some(candidate.diff.clone());
        let sources = BTreeMap::from([
            ("a.py".into(), "real()\n".into()),
            ("b.py".into(), "sibling\n".into()),
        ]);
        let result = check(
            &[candidate],
            "acme/demo",
            HEAD,
            &sources,
            Duration::from_millis(75),
            |_| async {
                tokio::time::sleep(Duration::from_millis(50)).await;
                Ok(clean("a.py"))
            },
        )
        .await;
        assert!(
            result.unwrap_err().contains("timed out"),
            "a per-request budget reset would incorrectly admit both checks"
        );
    }
}
