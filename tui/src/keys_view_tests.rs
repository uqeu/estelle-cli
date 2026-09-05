use super::*;
use crate::commands::render_remote_reply;
use crate::transcript::command_line_mask;
use serde_json::json;

/// The two rows from the dashboard the founder photographed, in the server's own field names.
fn dashboard_reply() -> CommandReply {
    serde_json::from_value(json!({"keys": [
        {"id": "hash-1", "prefix": "estelle_live_167fd7c…", "label": "default",
         "created_at": "2026-07-11T18:02:11+00:00", "expires_at": null,
         "expired": false, "revoked": true},
        {"id": "hash-2", "prefix": "estelle_live_227eda8…", "label": "testing2",
         "created_at": "2026-07-17T09:41:00+00:00", "expires_at": null,
         "expired": false, "revoked": false}
    ]}))
    .expect("keys reply")
}

/// Everything the surface actually prints: the renderer, then the display masker, joined.
fn on_screen(reply: &CommandReply) -> String {
    render_remote_reply("keys", reply)
        .iter()
        .map(|line| command_line_mask("keys", line))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The whole screen, byte for byte. Substring assertions cannot see a column that has silently
/// stopped lining up, or a footer that has drifted away from what the surface can actually do.
#[test]
fn the_two_dashboard_rows_render_exactly_like_this() {
    assert_eq!(
        on_screen(&dashboard_reply()),
        concat!(
            "2 keys  ·  raw keys are never returned — prefixes only\n",
            "   1  default   estelle_live_167fd7c…  ·  created 2026-07-11  ·  REVOKED\n",
            "   2  testing2  estelle_live_227eda8…  ·  created 2026-07-17  ·  ACTIVE\n",
            "\n",
            "Read-only. Renaming and revoking need a browser sign-in, so a stolen key cannot \
             revoke yours: https://fatelabs.ca/dashboard/keys",
        )
    );
}

/// 🔴 **THE REGRESSION THIS LANE EXISTS FOR.** Two rows reached the founder's screen as
/// `[credential hidden]` twice — safe and useless. Parity with the dashboard is four facts per
/// row, and this asserts all four SURVIVE THE MASKER, not merely that the renderer emitted them.
#[test]
fn every_row_shows_its_label_prefix_date_and_status_after_masking() {
    let screen = on_screen(&dashboard_reply());
    for fact in [
        "default",
        "estelle_live_167fd7c…",
        "created 2026-07-11",
        "REVOKED",
        "testing2",
        "estelle_live_227eda8…",
        "created 2026-07-17",
        "ACTIVE",
    ] {
        assert!(screen.contains(fact), "{fact:?} is missing\n{screen}");
    }
    assert!(
        !screen.contains("[credential hidden]"),
        "a row the server had already elided was hidden again\n{screen}"
    );
}

/// 🔴 **THE PAIR THAT CANNOT BOTH BE TRUE: A CREDENTIAL SHAPE REACHED THE SCREEN, AND THIS IS
/// THE `/keys` SURFACE.** Stated with no cause named, so it catches variants nobody has thought
/// of yet — a field added to `KeySummary`, a label a customer typed a key into, a server that
/// starts sending something it should not.
///
/// The corpus is DERIVED from the type: every string-valued field of a fully populated row is
/// enumerated from its own JSON, so a new field is covered the day it is added rather than the
/// day someone remembers to extend a list here.
#[test]
fn a_credential_shape_and_a_rendered_key_row_cannot_both_exist() {
    // Test-only strings. Each is a single repeated character after the marker, which is why it
    // is safe to write down and still long enough for the fence to name.
    const SHAPES: [&str; 6] = [
        "estelle_live_aaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "sk-bbbbbbbbbbbbbbbbbbbbbbbb",
        "sk_live_cccccccccccccccc",
        "ghp_dddddddddddddddddddddddddd",
        "github_pat_eeeeeeeeeeeeeeeeeeeeeeee",
        // ⚠️ THE PROBE THAT SITS ON THE GUARD'S BOUNDARY: a value wearing the elision mark, so
        // it LOOKS like something the server already cut down, and still long enough to be a
        // key. Both of `is_elided_key_prefix`'s clauses refuse it — the length cap and the
        // credential fence — and a mutation run showed each one alone is sufficient here, which
        // is the only way to know the second is insurance rather than decoration.
        "estelle_live_ffffffffffffffff…",
    ];
    let template = json!({
        "id": "hash-1", "prefix": "estelle_live_167fd7c…", "label": "default",
        "created_at": "2026-07-11T18:02:11+00:00", "expires_at": "2026-10-11T00:00:00+00:00",
        "expired": false, "revoked": false
    });
    let fields = template
        .as_object()
        .expect("object")
        .iter()
        .filter(|(_, value)| value.is_string())
        .map(|(field, _)| field.clone())
        .collect::<Vec<_>>();
    assert!(fields.len() >= 5, "the corpus lost its fields: {fields:?}");

    let mut probes = 0_usize;
    let mut reported = 0_usize;
    for field in &fields {
        for shape in SHAPES {
            // The instrument can fail: the value being injected is one the fence names.
            assert!(
                estelle_client::find_secret_shape(shape).is_some(),
                "{shape:?} is not recognised as a credential, so this probe proves nothing"
            );
            let mut row = template.clone();
            row[field] = json!(shape);
            let reply: CommandReply =
                serde_json::from_value(json!({"keys": [row]})).expect("keys reply");
            let screen = on_screen(&reply);
            assert!(
                estelle_client::find_secret_shape(&screen).is_none(),
                "a credential reached the screen via {field:?}\n{screen}"
            );
            // `id` is the one string field this view deliberately never prints (it is the key
            // HASH and the row number replaces it), so it has nothing to report as hidden.
            // Every field that IS printed must say so rather than silently dropping the value.
            if field != "id" {
                assert!(
                    screen.contains("[credential hidden]"),
                    "{field:?} vanished instead of being reported as hidden\n{screen}"
                );
                reported += 1;
            }
            probes += 1;
        }
    }
    assert_eq!(probes, fields.len() * SHAPES.len(), "probes were skipped");
    assert_eq!(reported, probes - SHAPES.len(), "the id exemption widened");
}

/// The server's `id` is the key hash. The row number addresses a key; the hash is never printed,
/// so it cannot be copied out of a screenshot or a scrollback.
#[test]
fn the_key_id_never_reaches_the_screen() {
    let screen = on_screen(&dashboard_reply());
    assert!(!screen.contains("hash-1"), "{screen}");
    assert!(!screen.contains("hash-2"), "{screen}");
}

/// `None` is not `false`. An absent flag reads as its own word rather than as ACTIVE.
#[test]
fn an_absent_status_flag_is_never_rendered_as_active() {
    let reply: CommandReply = serde_json::from_value(json!({"keys": [
        {"id": "h", "prefix": "estelle_live_167fd7c…", "label": "unknown-state"}
    ]}))
    .expect("sparse reply");
    let screen = on_screen(&reply);
    assert!(screen.contains("STATE NOT RETURNED"), "{screen}");
    assert!(!screen.contains("ACTIVE"), "{screen}");
    assert!(screen.contains("created not returned"), "{screen}");
}

/// A one-key account says "1 key", not "1 keys", and an expiry column appears only when the
/// server sent one.
#[test]
fn a_single_key_is_counted_in_the_singular_and_shows_its_expiry() {
    let reply: CommandReply = serde_json::from_value(json!({"keys": [
        {"id": "h", "prefix": "estelle_live_167fd7c…", "label": "only",
         "created_at": "2026-07-11T18:02:11+00:00", "expires_at": "2026-10-11T00:00:00+00:00",
         "expired": false, "revoked": false}
    ]}))
    .expect("single reply");
    let screen = on_screen(&reply);
    assert!(screen.starts_with("1 key  ·  "), "{screen}");
    assert!(screen.contains("expires 2026-10-11"), "{screen}");
    assert!(!screen.contains("never expires"), "{screen}");
}

#[test]
fn an_empty_account_says_so_and_still_names_the_dashboard() {
    let reply: CommandReply = serde_json::from_value(json!({"keys": []})).expect("empty");
    let screen = on_screen(&reply);
    assert!(screen.contains("No keys"), "{screen}");
    assert!(screen.contains("fatelabs.ca/dashboard/keys"), "{screen}");
}

#[test]
fn only_an_iso_date_is_shortened_and_anything_else_is_passed_through_whole() {
    assert_eq!(date("2026-07-11T18:02:11+00:00"), "2026-07-11");
    assert_eq!(date("2026-07-11"), "2026-07-11");
    assert_eq!(date("last tuesday"), "last tuesday");
    assert_eq!(date("20260711"), "20260711");
    assert_eq!(date(""), "");
}
