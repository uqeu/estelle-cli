//! 🔴 **THE CLI CANNOT REVOKE OR RENAME AN API KEY, AND THIS FILE IS THE PROOF.**
//!
//! The founder's decision, 2026-09-05: *"if someone gets a hold of your api key then kicks you
//! out with their api key its bad, we only let them do that api key stuff in the dashboard."* A
//! leaked `estelle_live_` key is theft of access; a leaked key that can also REVOKE is LOCKOUT,
//! and the victim loses the credential they would use to respond while the attacker keeps
//! working. The server already enforces it — `POST /me/keys/revoke` and `POST /me/keys/rename`
//! resolve a browser session and answer **401 `sign in required` to a valid API key, byte-
//! identical to the answer they give no credential at all**, while `GET /me/keys` accepts either.
//! This side must not grow a door the server would refuse.
//!
//! ⚠️ **ASSERTED ON WHAT IS SENT, NEVER ON A GREP OF SOURCE TEXT.** A scan for the string
//! `me/keys/revoke` matches the paragraph you are reading, so a source grep over this repo would
//! report a violation caused by its own documentation — and would equally pass over a request
//! built by string concatenation. Three instruments instead, each reading a request rather than a
//! file:
//!
//! 1. [`the_only_key_route_the_client_can_address_is_a_read`] reads `API_ENDPOINTS`, the table
//!    every `Client::get`/`post`/`put` resolves its path through.
//! 2. [`no_command_in_the_session_inventory_builds_a_request_to_a_key_write`] builds the actual
//!    [`RemoteRequest`] for every one of the CLI's session commands and reads the path it
//!    resolved to.
//! 3. [`driving_the_keys_door_with_every_spelling_of_the_two_verbs_puts_only_a_read_on_the_wire`]
//!    drives `execute_remote_command` — the function `main` calls — and reads
//!    wiremock's REQUEST LOG, which is the bytes that left the process.
//!
//! ⚠️ **WHAT A HOSTILE READER SHOULD BE TOLD THIS DOES NOT PROVE.** Instrument 1 covers every
//! request whose path comes from an `Endpoint`. Two request builders in `estelle-client/src/lib.rs`
//! do NOT take an `Endpoint` — `Client::job` (`jobs/{id}`) and `Client::stream_child_job`-family
//! (`jobs/{id}/events`) — and both are GETs over a locator validated to 24 hex characters, so
//! neither can address `me/keys/anything`. That is read and cited, not asserted here. Instrument 3
//! covers the `/keys` door only; instrument 2 is what extends the claim to the other commands.

use std::time::Duration;

use serde_json::json;
use tokio_util::sync::CancellationToken;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers::method;
use wiremock::matchers::path;

use crate::PendingCommand;
use crate::commands::remote_request;
use crate::commands::render_remote_reply;
use crate::commands::session_command_names;
use crate::execute_remote_command;
use crate::transcript::command_line_mask;
use estelle_client::API_ENDPOINTS;
use estelle_client::ApiKey;
use estelle_client::Client;
use estelle_client::HttpMethod;
use estelle_client::Repo;

/// The account key-management routes this CLI must never address. Spelled the way the server
/// spells them (`web/app/dashboard/keys/page.tsx` calls both).
const FORBIDDEN: [&str; 2] = ["me/keys/revoke", "me/keys/rename"];

/// The read the CLI does depend on, and the only key route it may address.
const PERMITTED: &str = "me/keys";

/// Every spelling of the two verbs a reader might type while hunting for them, plus the plain
/// list. None of these may put anything but a read on the wire.
const ARGUMENTS: [&str; 7] = [
    "",
    "revoke 1",
    "revoke 1 confirm",
    "revoke estelle_live_227eda8",
    "rename 1 ci-runner",
    "delete 1",
    "1 revoke",
];

/// The one judgement, shared by the live-wire test and by its own positive control: which of these
/// `(method, path)` pairs is a write to an account key-management route?
///
/// Deliberately WIDER than [`FORBIDDEN`]: anything that touches `/me/keys` and is not a `GET` of
/// exactly `/me/keys` is reported, so a route nobody has thought of yet — a `/me/keys/rotate`, a
/// `PUT` on `/me/keys` — is caught without being named.
fn key_writes(traffic: &[(String, String)]) -> Vec<String> {
    traffic
        .iter()
        .filter(|(verb, path)| {
            path.starts_with("/me/keys") && !(verb == "GET" && path == "/me/keys")
        })
        .map(|(verb, path)| format!("{verb} {path}"))
        .collect()
}

/// 🔴 **INSTRUMENT 1 — THE TABLE EVERY REQUEST BUILDER RESOLVES A PATH THROUGH.**
///
/// `Client::get`, `Client::post`, `Client::post_scoped` and `Client::put` all take an `Endpoint`
/// and build their URL from `Endpoint::path()`. A variant that does not exist cannot be passed,
/// so a table with no write under `me/keys/` is a structural fact about what this binary can
/// send — not a claim about what it happens to call today.
#[test]
fn the_only_key_route_the_client_can_address_is_a_read() {
    // CONTROL: an emptied or truncated table must not be able to pass this test by vacuity.
    assert!(
        API_ENDPOINTS.len() >= 50,
        "the endpoint table has {} rows — too few to be the real one, so every \
         'no key write exists' assertion below would be vacuous",
        API_ENDPOINTS.len()
    );

    let key_routes = API_ENDPOINTS
        .iter()
        .filter(|spec| spec.path == PERMITTED || spec.path.starts_with("me/keys/"))
        .collect::<Vec<_>>();

    // CONTROL, the other direction: exactly ONE key route, and it is the read the `/keys` listing
    // needs. A table that lost `me/keys` entirely would fail here rather than passing quietly.
    assert_eq!(
        key_routes.len(),
        1,
        "expected exactly one key route; got {:?}",
        key_routes.iter().map(|spec| spec.path).collect::<Vec<_>>()
    );
    assert_eq!(key_routes[0].path, PERMITTED);
    assert_eq!(
        key_routes[0].methods,
        &[HttpMethod::Get],
        "the key inventory route gained a write method"
    );
    assert!(!key_routes[0].requires_repo, "keys are account-scoped");

    // And the two names the founder's decision is actually about, said out loud.
    for forbidden in FORBIDDEN {
        assert!(
            !API_ENDPOINTS.iter().any(|spec| spec.path == forbidden),
            "{forbidden} is addressable from this client — key management is dashboard-only"
        );
    }
}

/// 🔴 **INSTRUMENT 2 — THE REQUEST EACH COMMAND ACTUALLY BUILDS.**
///
/// The whole session inventory, not just `/keys`: every command name the CLI accepts, over the
/// arguments a reader hunting for the verbs would type, with and without a diff (`gate`, `scan`
/// and `review` refuse to route without one). Reads the `Endpoint` each built request resolved
/// to, which is the thing that becomes a URL.
#[test]
fn no_command_in_the_session_inventory_builds_a_request_to_a_key_write() {
    let mut built = 0_usize;
    let mut saw_the_key_read = false;
    for name in session_command_names() {
        for argument in ARGUMENTS {
            for diff in [None, Some("--- a/x\n+++ b/x\n@@ -1 +1 @@\n-a\n+b\n")] {
                let Some(request) = remote_request(name, argument, diff, None).ok().flatten()
                else {
                    continue;
                };
                built += 1;
                let path = request.endpoint.path();
                assert!(
                    !path.starts_with("me/keys/"),
                    "/{name} {argument:?} builds a request to {path} — key management is \
                     dashboard-only"
                );
                if path == PERMITTED {
                    saw_the_key_read = true;
                }
            }
        }
    }
    // CONTROL: a `remote_request` that returned `None` for everything would satisfy the assertion
    // above without proving anything. Both of these fail on a sweep that built nothing real.
    assert!(
        built >= 100,
        "the sweep built only {built} requests over {} commands — it is not exercising the \
         inventory",
        session_command_names().len()
    );
    assert!(
        saw_the_key_read,
        "no command in the inventory routed to {PERMITTED}; /keys is not reaching the wire, so \
         this sweep proves nothing about it"
    );
}

/// 🔴 **INSTRUMENT 3 — THE BYTES THAT LEFT THE PROCESS.**
///
/// `.expect(0)` on a catch-all `POST` and a catch-all `PUT` is verified when the server drops,
/// so a write to ANY path fails this — including one nobody anticipated. The request log is then
/// read directly, because a mock can be mis-specified and a log cannot.
#[tokio::test]
async fn driving_the_keys_door_with_every_spelling_of_the_two_verbs_puts_only_a_read_on_the_wire() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/me/keys"))
        .respond_with(ResponseTemplate::new(200).set_body_json(listing()))
        .mount(&server)
        .await;
    for verb in ["POST", "PUT"] {
        Mock::given(method(verb))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"ok": true})))
            .expect(0)
            .mount(&server)
            .await;
    }

    for argument in ARGUMENTS {
        let screen = run_keys(&server, argument).await;
        // CONTROL: a CLI that renders nothing, errors out, or quietly stops calling the server
        // would send no writes and pass the assertion below for the wrong reason.
        assert!(
            screen.contains("testing2") && screen.contains("estelle_live_227eda8…"),
            "/keys {argument:?} did not render the listing\n{screen}"
        );
        assert!(
            screen.contains("fatelabs.ca/dashboard/keys"),
            "/keys {argument:?} did not say where key management lives\n{screen}"
        );
    }

    let traffic = traffic(&server).await;
    assert!(
        key_writes(&traffic).is_empty(),
        "a key write reached the wire: {:?}",
        key_writes(&traffic)
    );
    // CONTROL: the log must show the reads that DID happen, one per invocation. An empty log
    // would make the assertion above vacuous.
    assert_eq!(
        traffic,
        ARGUMENTS
            .iter()
            .map(|_| ("GET".to_string(), "/me/keys".to_string()))
            .collect::<Vec<_>>(),
        "the wire carried something other than one read per invocation"
    );
}

/// ⚠️ **THE POSITIVE CONTROL FOR INSTRUMENT 3, AND IT IS NOT OPTIONAL.** The test above asserts
/// an ABSENCE, and an absence is what a broken instrument reports too. This fires a real
/// `POST /me/keys/revoke` at a mock server with `reqwest` — nothing in the CLI can produce it,
/// which is the point — and asserts the log records it and [`key_writes`] names it.
#[tokio::test]
async fn the_request_log_and_the_predicate_would_both_see_a_key_write_if_one_happened() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"ok": true})))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/me/keys"))
        .respond_with(ResponseTemplate::new(200).set_body_json(listing()))
        .mount(&server)
        .await;

    let http = reqwest::Client::new();
    for forbidden in FORBIDDEN {
        http.post(format!("{}/{forbidden}", server.uri()))
            .json(&json!({"id": "hash-2"}))
            .send()
            .await
            .expect("the control request must reach the mock server");
    }
    // The read comes from the SHIPPED door, not from `reqwest`, so this one log proves the
    // recorder sees this client's traffic as well as the synthetic write.
    // The count line carries no credential, so this control is independent of the display masker
    // — a masker regression must fail the display tests, not this one.
    let screen = run_keys(&server, "revoke 1 confirm").await;
    assert!(screen.contains("2 keys"), "{screen}");

    let traffic = traffic(&server).await;
    assert_eq!(
        key_writes(&traffic),
        vec![
            "POST /me/keys/revoke".to_string(),
            "POST /me/keys/rename".to_string()
        ],
        "the predicate did not name the writes it exists to catch; got {traffic:?}"
    );
    // …and it does not fire on the CLI's read, so the assertion in the test above is
    // discriminating rather than merely quiet.
    assert!(
        traffic.contains(&("GET".to_string(), "/me/keys".to_string())),
        "the shipped client's own read is missing from the log: {traffic:?}"
    );
}

/// Two rows in the server's own field names.
fn listing() -> serde_json::Value {
    json!({"keys": [
        {"id": "hash-1", "prefix": "estelle_live_167fd7c…", "label": "default",
         "created_at": "2026-07-11T18:02:11+00:00", "expired": false, "revoked": false},
        {"id": "hash-2", "prefix": "estelle_live_227eda8…", "label": "testing2",
         "created_at": "2026-07-17T09:41:00+00:00", "expired": false, "revoked": false}
    ]})
}

/// Every request the mock server saw, as `(METHOD, /path)`, in order.
async fn traffic(server: &MockServer) -> Vec<(String, String)> {
    server
        .received_requests()
        .await
        .expect("request recording is on")
        .iter()
        .map(|request| {
            (
                request.method.to_string().to_ascii_uppercase(),
                request.url.path().to_string(),
            )
        })
        .collect()
}

/// One `/keys …` invocation through the shipped entry point, rendered exactly as it reaches the
/// screen: `execute_remote_command` → `render_remote_reply` → `command_line_mask`.
async fn run_keys(server: &MockServer, argument: &str) -> String {
    let client = Client::new(
        &format!("{}/", server.uri()),
        // Test-only. A single repeated character after the marker, which is why it is safe to
        // write down: it is not, and could not be, anyone's credential.
        ApiKey::new("estelle_live_zzzzzzzzzzzzzzzzzzzz").expect("key"),
        Duration::from_secs(120),
    )
    .expect("client");
    let reply = execute_remote_command(
        client,
        Repo::new("fatelabs/estelle").expect("repo"),
        tempfile::tempdir().expect("root").path().to_path_buf(),
        PendingCommand {
            name: "keys",
            argument: argument.to_string(),
            last_question: None,
            skill_thread: None,
        },
        &CancellationToken::new(),
        None,
        None,
    )
    .await
    .expect("keys reply")
    .reply;
    render_remote_reply("keys", &reply)
        .iter()
        .map(|line| command_line_mask("keys", line))
        .collect::<Vec<_>>()
        .join("\n")
}
