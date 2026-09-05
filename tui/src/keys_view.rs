//! How `/keys` reads. Parity with `web/app/dashboard/keys/page.tsx` on the four facts a human
//! needs to tell two keys apart: label, the masked prefix, the created date, and the status.
//!
//! 🔴 **THIS SURFACE IS READ-ONLY, AND THAT IS A SECURITY DECISION, NOT AN UNFINISHED FEATURE.**
//! A leaked `estelle_live_` key is theft of access. If that same key could also REVOKE the
//! owner's keys it would be LOCKOUT: the victim loses the credential they would use to respond
//! to the incident while the attacker keeps working. So the server resolves a browser session
//! (Supabase JWT) for `POST /me/keys/rename` and `POST /me/keys/revoke` and answers **401 `sign
//! in required` to a valid API key — byte-identical to the answer it gives no credential at
//! all** — while `GET /me/keys` accepts either. Measured on production 2026-09-05. The CLI holds
//! an API key and nothing else, so those two verbs could only ever render a 401 here; shipping
//! them would be shipping a broken feature. [`DASHBOARD_LINE`] says so in words instead.
//!
//! ⚠️ **NOTHING HERE DECIDES WHAT MAY BE SHOWN.** This file lays out text; the credential
//! judgement is `estelle_client::mask_key_row`, applied to every line of a `/keys` reply on the
//! way to the screen. Adding a field to this layout cannot widen what is displayed.

use estelle_client::CommandReply;
use estelle_client::KeySummary;

const SEPARATOR: &str = "  ·  ";

/// The one honest sentence about the two verbs that are not here.
///
/// The URL is the real dashboard route: `web/app/dashboard/keys/page.tsx` exists in the parent
/// repo and is where `saveRename` and `revoke` are wired. `commands.rs` already sends readers to
/// `https://fatelabs.ca/dashboard/provider` for the sibling surface, so the host is not invented.
pub(crate) const DASHBOARD_LINE: &str = concat!(
    "Read-only. Renaming and revoking need a browser sign-in, ",
    "so a stolen key cannot revoke yours: https://fatelabs.ca/dashboard/keys"
);

pub(crate) fn render(reply: &CommandReply) -> Vec<String> {
    if reply.me_keys.is_empty() {
        return vec![
            "No keys on this account. New keys are created on the dashboard and shown once."
                .to_string(),
            String::new(),
            DASHBOARD_LINE.to_string(),
        ];
    }
    let mut lines = vec![format!(
        "{} {}{SEPARATOR}raw keys are never returned — prefixes only",
        reply.me_keys.len(),
        if reply.me_keys.len() == 1 {
            "key"
        } else {
            "keys"
        }
    )];
    let label_width = reply
        .me_keys
        .iter()
        .map(|key| label_of(key).chars().count())
        .max()
        .unwrap_or(0);
    for (row, key) in reply.me_keys.iter().enumerate() {
        lines.push(format!(
            "  {:>2}  {:<label_width$}  {}",
            row + 1,
            label_of(key),
            facts(key)
        ));
    }
    lines.push(String::new());
    lines.push(DASHBOARD_LINE.to_string());
    lines
}

fn label_of(key: &KeySummary) -> String {
    match key.label.as_deref() {
        Some(label) if !label.trim().is_empty() => label.trim().to_string(),
        Some(_) => "(no label)".to_string(),
        None => "(label not returned)".to_string(),
    }
}

/// `prefix · created … [· expires …] · STATUS`
fn facts(key: &KeySummary) -> String {
    let mut parts = vec![
        key.prefix
            .as_deref()
            .filter(|prefix| !prefix.trim().is_empty())
            .unwrap_or("(prefix not returned)")
            .to_string(),
        match key.created_at.as_deref() {
            Some(created) if !created.trim().is_empty() => format!("created {}", date(created)),
            _ => "created not returned".to_string(),
        },
    ];
    if let Some(expires) = key
        .expires_at
        .as_deref()
        .filter(|expires| !expires.trim().is_empty())
    {
        parts.push(format!("expires {}", date(expires)));
    }
    parts.push(status(key).to_string());
    parts.join(SEPARATOR)
}

/// 🔴 THREE STATES, ONE COLUMN, AND `None` IS NOT `false`. The server sends booleans; an ABSENT
/// flag is not a claim that the key is fine, so it reads as its own word rather than as ACTIVE.
fn status(key: &KeySummary) -> &'static str {
    match (key.revoked, key.expired) {
        (Some(true), _) => "REVOKED",
        (_, Some(true)) => "EXPIRED",
        (Some(false), Some(false)) => "ACTIVE",
        _ => "STATE NOT RETURNED",
    }
}

/// The server sends ISO-8601; the dashboard shows the day. Anything that is not an ISO date is
/// passed through whole rather than truncated into something that looks like one.
fn date(value: &str) -> &str {
    let head = value.split('T').next().unwrap_or(value);
    let dated = head.len() == 10
        && head.bytes().enumerate().all(|(index, byte)| match index {
            4 | 7 => byte == b'-',
            _ => byte.is_ascii_digit(),
        });
    if dated { head } else { value }
}

#[cfg(test)]
#[path = "keys_view_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "keys_readonly_tests.rs"]
mod readonly_tests;
