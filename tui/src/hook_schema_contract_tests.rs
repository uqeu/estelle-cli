//! 🔴 THE HOST'S WHOLE PAYLOAD CONTRACT, READ OFF ITS OWN GENERATED SCHEMAS AT TEST TIME.
//!
//! `transcript_path` was one field of one event. The class it belongs to is: **we validated
//! [`super::HookPayload`] against Claude Code's contract and shipped it into a host whose
//! contract is a strict superset.** Fixing four more field names would have fixed the instance
//! and missed the class again, so nothing here names a field: the payloads are BUILT from
//! `hooks/schema/generated/*.command.input.schema.json` at runtime, which is the same file the
//! host generates its own serializers from.
//!
//! # What the host actually requires — measured, not remembered
//!
//! Eleven input schemas, `required` per event (⊕ = also an OPTIONAL property of that event):
//!
//! ```text
//!   event               required
//!   PreToolUse          cwd hook_event_name model permission_mode session_id tool_input
//!                       tool_name tool_use_id transcript_path turn_id          ⊕ agent_id agent_type
//!   PostToolUse         …the above, plus tool_response                          ⊕ agent_id agent_type
//!   PermissionRequest   …PreToolUse minus tool_use_id                           ⊕ agent_id agent_type
//!   UserPromptSubmit    cwd hook_event_name model permission_mode prompt
//!                       session_id transcript_path turn_id                      ⊕ agent_id agent_type
//!   PreCompact          cwd hook_event_name model session_id transcript_path
//!   PostCompact           trigger turn_id                                       ⊕ agent_id agent_type
//!   SessionStart        cwd hook_event_name model permission_mode session_id source transcript_path
//!   SessionEnd          cwd hook_event_name reason session_id transcript_path
//!   Stop                cwd hook_event_name last_assistant_message model permission_mode
//!                       session_id stop_hook_active transcript_path turn_id
//!   SubagentStart       agent_id agent_type cwd hook_event_name model permission_mode
//!                       session_id transcript_path turn_id
//!   SubagentStop        …Stop, plus agent_id agent_type agent_transcript_path
//! ```
//!
//! **Twenty distinct field names across the eleven events. [`super::HookPayload`] names eight.**
//! The other twelve — `agent_id`, `agent_transcript_path`, `agent_type`, `last_assistant_message`,
//! `model`, `permission_mode`, `reason`, `source`, `stop_hook_active`, `tool_use_id`, `trigger`,
//! `turn_id` — are facts no mode reads today.
//!
//! # ⚠️ THE PREMISE I WAS HANDED WAS WRONG, AND SAYING SO IS THE POINT
//!
//! Those twelve were reported as the cause of `hook exited with code 1` in a real Codex session.
//! **They are not.** [`super::HookPayload`] carries no `#[serde(deny_unknown_fields)]`, so serde
//! IGNORES a field it has no place for; an unmodelled field is dropped, never rejected. Measured
//! against the shipped `estelle` binary before a line of this was written — all eleven schemas,
//! both the required-with-nulls and the everything-populated shape, every mode `HOOK_TABLE`
//! declares for that event: **22 payloads, 0 non-zero exits.**
//!
//! So the tolerance was real and it was **accidental**: nothing anywhere asserted it, and one
//! attribute on one struct would have taken all eight verbs down again. That is what these tests
//! convert into a contract.
//!
//! # WHICH HALF THIS COVERS
//!
//! The REQUEST half — what a host is allowed to SEND us — and only as far as the mode's LOCAL
//! branch. Every payload is deliberately shaped to keep every mode off the network (a `.txt`
//! `file_path` so `ground` abstains and `sync` declines an unindexable type; a blank `prompt` so
//! `context` never searches), because the subject under test is DESERIALIZATION AND DISPATCH,
//! not the server call beyond it. It asserts nothing about the response envelope, nothing about
//! any mode's networked branch, and nothing about hosts whose schemas we have not vendored.

use super::*;

use std::collections::BTreeSet;

/// Where the host's generated input schemas live, relative to this crate.
const SCHEMA_DIR: &str = "../hooks/schema/generated";

/// The suffix that marks an INPUT schema. The same directory holds `*.command.output.schema.json`
/// for the response half, which is a different contract with a different owner.
const INPUT_SUFFIX: &str = ".command.input.schema.json";

/// Every field name any vendored host schema can send, written out.
///
/// 🔴 **PINNED BY WRITING IT OUT, BECAUSE A DERIVED LIST CANNOT CATCH A REGRESSION.** Computing
/// this from the schemas and comparing it to itself would pass for any schema set, including an
/// empty one. Written like this, re-vendoring a host that has ADDED a field turns
/// [`the_union_of_host_fields_is_the_set_this_payload_was_written_against`] red and somebody
/// decides whether the new fact is one we should read — while every OTHER test here proves the
/// verbs keep working in the meantime.
const HOST_FIELDS: [&str; 20] = [
    "agent_id",
    "agent_transcript_path",
    "agent_type",
    "cwd",
    "hook_event_name",
    "last_assistant_message",
    "model",
    "permission_mode",
    "prompt",
    "reason",
    "session_id",
    "source",
    "stop_hook_active",
    "tool_input",
    "tool_name",
    "tool_response",
    "tool_use_id",
    "transcript_path",
    "trigger",
    "turn_id",
];

/// Every generated INPUT schema the host ships, as `(file stem, parsed schema)`.
///
/// Read from disk at runtime rather than `include_str!`ed one by one: a hard-coded list is a
/// second owner of "which events exist" and could never notice an event the host added, which is
/// precisely the failure this file exists to prevent.
fn host_input_schemas() -> Vec<(String, Value)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(SCHEMA_DIR);
    let mut schemas: Vec<(String, Value)> = fs::read_dir(&dir)
        .unwrap_or_else(|error| panic!("the host's schema directory {}: {error}", dir.display()))
        .map(|entry| entry.expect("schema directory entry").path())
        .filter_map(|path| {
            let name = path.file_name()?.to_str()?.to_string();
            let stem = name.strip_suffix(INPUT_SUFFIX)?.to_string();
            let text = fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
            let value: Value = serde_json::from_str(&text)
                .unwrap_or_else(|error| panic!("parse {}: {error}", path.display()));
            Some((stem, value))
        })
        .collect();
    schemas.sort_by(|left, right| left.0.cmp(&right.0));
    schemas
}

/// The `hook_event_name` const a schema declares — the host's own name for the event.
fn event_of(stem: &str, schema: &Value) -> String {
    schema["properties"]["hook_event_name"]["const"]
        .as_str()
        .unwrap_or_else(|| panic!("{stem}: no hook_event_name const in the host's schema"))
        .to_string()
}

/// One value satisfying one property spec.
///
/// ⚠️ **THE FALLBACK IS A PANIC, NOT A STRING.** A `_ =>` arm returning `"probe"` would let a
/// property shape this synthesiser does not understand pass silently as a string, and the whole
/// suite would then be green over a payload that does not satisfy the schema it claims to. An
/// unknown shape is a defect in this file and must read like one.
fn sample(spec: &Value, defs: &Value, nullables_as_null: bool, field: &str) -> Value {
    // `true` is JSON Schema for "any value at all" — `tool_input` and `tool_response` are typed
    // that way because the host does not constrain a tool's own payload.
    if spec == &Value::Bool(true) {
        return json!({"probe": "any JSON satisfies this property"});
    }
    if let Some(constant) = spec.get("const") {
        return constant.clone();
    }
    if let Some(first) = spec.get("enum").and_then(Value::as_array).and_then(|e| {
        assert!(!e.is_empty(), "{field}: an empty enum cannot be satisfied");
        e.first()
    }) {
        return first.clone();
    }
    if let Some(reference) = spec.get("$ref").and_then(Value::as_str) {
        let name = reference
            .rsplit('/')
            .next()
            .unwrap_or_else(|| panic!("{field}: unusable $ref {reference}"));
        let target = defs
            .get(name)
            .unwrap_or_else(|| panic!("{field}: $ref {reference} names no definition"));
        return sample(target, defs, nullables_as_null, field);
    }
    match spec.get("type") {
        Some(Value::String(kind)) => match kind.as_str() {
            "string" => json!("estelle-probe"),
            "boolean" => json!(false),
            "integer" | "number" => json!(1),
            "object" => json!({}),
            "array" => json!([]),
            other => panic!("{field}: unhandled schema type {other:?}"),
        },
        // A union type, which in these schemas is always `["string", "null"]`. BOTH spellings are
        // exercised — `null` is what Codex sends for a session with no materialised rollout, and
        // a real string is what it sends otherwise.
        Some(Value::Array(kinds)) => {
            assert!(
                kinds.iter().any(|kind| kind == "string"),
                "{field}: union type without a string arm: {kinds:?}"
            );
            if nullables_as_null && kinds.iter().any(|kind| kind == "null") {
                Value::Null
            } else {
                json!("estelle-probe")
            }
        }
        other => panic!("{field}: unhandled property spec {other:?} / {spec}"),
    }
}

/// The value this probe sends for one field.
///
/// Three fields are chosen rather than synthesised, and every choice still SATISFIES the schema
/// — each of the three is typed `string` and each of these is a string. They are chosen to keep
/// every mode on its local branch, which is what makes this suite runnable with no credential,
/// no server and no network. **That is also the limit: nothing past those branches is covered.**
fn sample_for_field(field: &str, spec: &Value, defs: &Value, nulls: bool, root: &Path) -> Value {
    match field {
        // A real directory. `cwd` is a string to the schema and a PATH to `welcome`, `sync` and
        // `shift`; a synthetic name would exercise the not-a-directory branch instead.
        "cwd" => json!(root.to_string_lossy()),
        // Blank on purpose: `context_precheck` trims and returns `Silent` for an empty prompt, so
        // `context` never reaches `/memory/chat`. A non-blank prompt here would make this suite
        // spend a customer's credential on every run.
        "prompt" => json!("   "),
        // A `.txt` write: `ground_scope` abstains on anything that is not Python and
        // `hook_sync_refusal` calls it an unindexable type, so neither mode calls out.
        "tool_input" => json!({"file_path": "notes.txt", "content": "probe"}),
        _ => sample(spec, defs, nulls, field),
    }
}

/// How much of the host's contract one probe payload carries.
#[derive(Clone, Copy, Debug)]
enum Shape {
    /// Exactly the properties the host marks `required`, with every nullable one sent as `null`.
    /// This is what Codex writes for a thread with no materialised local rollout.
    RequiredWithNulls,
    /// Required AND optional, every nullable carrying a real string. The widest payload the
    /// vendored contract permits.
    EverythingPopulated,
    /// The widest permitted payload PLUS a property no host ships today.
    ///
    /// 🔴 This is the arm that makes "a host adding a field must never break a verb" a fact
    /// rather than an intention. It is the exact mutation `#[serde(deny_unknown_fields)]` would
    /// perform on us, driven from the outside.
    EverythingPlusAnUnknownField,
}

impl Shape {
    const ALL: [Shape; 3] = [
        Shape::RequiredWithNulls,
        Shape::EverythingPopulated,
        Shape::EverythingPlusAnUnknownField,
    ];

    fn nullables_as_null(self) -> bool {
        matches!(self, Shape::RequiredWithNulls)
    }

    fn include_optional(self) -> bool {
        !matches!(self, Shape::RequiredWithNulls)
    }
}

/// A JSON payload that satisfies `schema` in the given shape.
fn payload_for(stem: &str, schema: &Value, shape: Shape, root: &Path) -> String {
    let defs = schema
        .get("definitions")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let properties = schema["properties"]
        .as_object()
        .unwrap_or_else(|| panic!("{stem}: schema has no properties"));
    let required: BTreeSet<&str> = schema["required"]
        .as_array()
        .unwrap_or_else(|| panic!("{stem}: schema has no required list"))
        .iter()
        .map(|field| {
            field
                .as_str()
                .unwrap_or_else(|| panic!("{stem}: non-string entry in required"))
        })
        .collect();

    let mut payload = serde_json::Map::new();
    for (field, spec) in properties {
        if !required.contains(field.as_str()) && !shape.include_optional() {
            continue;
        }
        payload.insert(
            field.clone(),
            sample_for_field(field, spec, &defs, shape.nullables_as_null(), root),
        );
    }
    if matches!(shape, Shape::EverythingPlusAnUnknownField) {
        payload.insert(
            "a_field_no_host_ships_today".to_string(),
            json!({"nested": [1, "two", null]}),
        );
    }
    assert!(
        required.iter().all(|field| payload.contains_key(*field)),
        "{stem}: the probe dropped a required field"
    );
    Value::Object(payload).to_string()
}

/// 🔴 **THE LOAD-BEARING ONE.** Every schema the host ships, in three shapes, through the REAL
/// deserializer and the REAL dispatcher.
///
/// A hand-built `HookPayload` literal proves nothing — it cannot fail the way a wire payload
/// fails — so every row here starts as JSON text and ends inside [`super::run_hook_with`], the
/// same function `estelle hook <mode>` calls with the host's bytes on stdin.
///
/// Events `HOOK_TABLE` declares a mode for are driven through THAT mode with `--event` pinned,
/// which is exactly how `install-hooks` invokes us. Events we install nothing for — Codex fires
/// `PermissionRequest`, `PostCompact`, `SubagentStart` and `SubagentStop` — are still payloads we
/// must never choke on, so they go through the dispatcher with the event unconstrained.
#[tokio::test]
async fn every_generated_host_schema_reaches_every_mode_it_declares() {
    let root = tempfile::tempdir().expect("hook root");
    let repo = Repo::default();
    let schemas = host_input_schemas();

    // NON-VACUITY. A glob that matched nothing would otherwise make every assertion below
    // trivially true. Eleven is what the host ships today; MORE is fine, fewer is a defect in
    // this reader or a schema that stopped shipping.
    assert!(
        schemas.len() >= 11,
        "only {} host input schemas found — this suite would be measuring nothing",
        schemas.len()
    );

    let mut events_seen: BTreeSet<String> = BTreeSet::new();
    let mut dispatched: Vec<String> = Vec::new();

    for (stem, schema) in &schemas {
        let event = event_of(stem, schema);
        events_seen.insert(event.clone());
        for shape in Shape::ALL {
            let payload = payload_for(stem, schema, shape, root.path());

            // 1 — THE REAL DESERIALIZER, named separately so a parse failure reads as one.
            let parsed = serde_json::from_str::<HookPayload>(&payload).unwrap_or_else(|error| {
                panic!("{stem} {shape:?}: the host's own payload was REFUSED: {error}\n{payload}")
            });
            assert_eq!(
                parsed.hook_event_name, event,
                "{stem} {shape:?}: the event did not survive deserialization"
            );

            // 2 — THE REAL DISPATCHER, for every mode this event installs.
            let rows: Vec<&HookRow> = HOOK_TABLE.iter().filter(|row| row.event == event).collect();
            if rows.is_empty() {
                let result = run_hook_with("guard", None, &payload, &repo, root.path()).await;
                assert!(
                    result.is_ok(),
                    "{stem} {shape:?}: an event we install nothing for still broke a verb: {result:?}"
                );
                dispatched.push(format!("{event}/guard(unbound)"));
                continue;
            }
            for row in rows {
                let result =
                    run_hook_with(row.mode, Some(row.event), &payload, &repo, root.path()).await;
                assert!(
                    result.is_ok(),
                    "{stem} {shape:?}: mode {} failed on a payload the host is allowed to send: {result:?}",
                    row.mode
                );
                dispatched.push(format!("{event}/{}", row.mode));
            }
        }
    }

    // NON-VACUITY, SECOND HALF: the loop above must have actually run every mode the installer
    // can write, not merely have iterated. A table row whose event no schema declares would be a
    // hook we install and never test.
    for row in HOOK_TABLE {
        assert!(
            dispatched.contains(&format!("{}/{}", row.event, row.mode)),
            "no host schema declares {} — mode {} was never driven",
            row.event,
            row.mode
        );
    }
    assert!(
        dispatched.len() >= HOOK_TABLE.len() * Shape::ALL.len(),
        "only {} dispatches for {} table rows x {} shapes",
        dispatched.len(),
        HOOK_TABLE.len(),
        Shape::ALL.len()
    );
    for event in [
        "PermissionRequest",
        "PostCompact",
        "SubagentStart",
        "SubagentStop",
    ] {
        assert!(
            events_seen.contains(event),
            "the host ships {event} and this suite never saw it"
        );
    }
}

/// The SIBLING SHAPE, one field at a time, over the WHOLE union rather than the eight we read.
///
/// `#[serde(default)]` covers the ABSENT key and nothing else; a present `null` is a different
/// wire shape and serde treats it as a type error. Both spellings must mean "the host has no
/// value for this", for every field the host can send — including the twelve no mode reads,
/// because "we ignore it" and "we reject the payload over it" are one attribute apart.
///
/// `hook_event_name` is the one exemption and it is a DECLARED one: its absence is a real
/// failure, because dispatch has nothing to route. That branch is asserted in
/// [`a_malformed_payload_is_still_refused_and_names_its_branch`].
#[test]
fn every_field_any_host_schema_can_send_tolerates_absent_null_and_its_own_type() {
    // ABSENT — the whole union missing at once.
    assert!(
        serde_json::from_value::<HookPayload>(json!({"hook_event_name": "PreToolUse"})).is_ok(),
        "the minimal payload every host sends was refused"
    );

    let mut all_null = json!({"hook_event_name": "PreToolUse"});
    for field in HOST_FIELDS {
        if field == "hook_event_name" {
            continue;
        }
        all_null[field] = Value::Null;
        for spelling in [json!(null), json!("estelle-probe")] {
            let mut payload = json!({"hook_event_name": "PreToolUse"});
            payload[field] = spelling.clone();
            let parsed = serde_json::from_value::<HookPayload>(payload);
            assert!(
                parsed.is_ok(),
                "{field} = {spelling}: a spelling the host is allowed to send was REFUSED: {parsed:?}"
            );
        }
    }
    // NULL IN EVERY POSITION AT ONCE — the shape a host with nothing to say produces.
    assert!(
        serde_json::from_value::<HookPayload>(all_null.clone()).is_ok(),
        "a payload that is null in all nineteen positions was refused: {all_null}"
    );
}

/// The ledger of what the host can send, pinned by hand.
///
/// Re-vendoring a host that added a field turns this red on purpose. It is a REVIEW trigger, not
/// a runtime one: the verb keeps working either way — that is what
/// [`every_generated_host_schema_reaches_every_mode_it_declares`] proves — and this only forces
/// somebody to decide whether the new fact is one a mode should read.
#[test]
fn the_union_of_host_fields_is_the_set_this_payload_was_written_against() {
    let mut union: BTreeSet<String> = BTreeSet::new();
    for (stem, schema) in host_input_schemas() {
        let properties = schema["properties"]
            .as_object()
            .unwrap_or_else(|| panic!("{stem}: schema has no properties"));
        for field in properties.keys() {
            union.insert(field.clone());
        }
    }
    let expected: BTreeSet<String> = HOST_FIELDS.iter().map(ToString::to_string).collect();
    assert_eq!(
        union, expected,
        "the vendored host schemas no longer send exactly the fields this payload was written \
         against. Nothing is broken — unknown fields are ignored, and \
         every_generated_host_schema_reaches_every_mode_it_declares proves the verbs still run — \
         but decide whether the new field is one a mode should READ, then update HOST_FIELDS."
    );
}

/// 🔴 **THE NEGATIVE CONTROL. TOLERANCE IS NOT "ACCEPT ANYTHING".**
///
/// The cheapest way to make every test above pass is to stop validating, which would trade a
/// loud failure for a silent one — the defect this codebase spends the most effort avoiding. So
/// each row is a payload that MUST still be refused, and the rows reach THREE different refusal
/// branches, so one over-broad "accept everything" change cannot leave this green.
#[tokio::test]
async fn a_malformed_payload_is_still_refused_and_names_its_branch() {
    let root = tempfile::tempdir().expect("hook root");
    let repo = Repo::default();

    let rows: [(&str, Option<&str>, &str, &str); 5] = [
        // Not JSON at all.
        ("welcome", None, "{not json", "branch=input-json"),
        // JSON, but not an object.
        (
            "guard",
            Some("PreToolUse"),
            "[1, 2, 3]",
            "branch=input-json",
        ),
        // A MODELLED field present with the wrong type. `session_id` is a string to every host,
        // and a number there is a defect worth failing loudly over.
        (
            "guard",
            Some("PreToolUse"),
            r#"{"hook_event_name":"PreToolUse","session_id":7}"#,
            "branch=input-json",
        ),
        // Parses, but does not say which event fired — dispatch has nothing to route.
        (
            "guard",
            Some("PreToolUse"),
            r#"{"cwd":"/tmp"}"#,
            "branch=event-missing",
        ),
        // Says an event other than the one this handler was installed for.
        (
            "checkpoint",
            Some("Stop"),
            r#"{"hook_event_name":"SubagentStop"}"#,
            "branch=event-mismatch",
        ),
    ];

    let mut branches: BTreeSet<&str> = BTreeSet::new();
    for (mode, event, payload, branch) in rows {
        let error = run_hook_with(mode, event, payload, &repo, root.path())
            .await
            .unwrap_err();
        assert!(
            error.contains(branch),
            "mode {mode} refused {payload} on the wrong branch: {error}"
        );
        branches.insert(branch);
    }
    // Three distinct refusal branches, so this cannot be satisfied by one of them still working.
    assert_eq!(branches.len(), 3, "{branches:?}");
}
