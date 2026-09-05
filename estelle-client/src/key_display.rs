//! The one owner of "may this token appear on a key-inventory row?".
//!
//! 🔴 **A PREFIX IS AN IDENTIFIER; THE FULL STRING IS A CREDENTIAL — AND ONLY ONE FUNCTION IN
//! THIS BINARY IS ALLOWED TO TELL THEM APART.** [`mask_secret`] masks a whole value the moment it
//! sees a vendor marker, which is correct for prose and wrong for the single surface whose
//! SUBJECT is the key inventory. `/keys` renders the display prefix the server has already cut
//! down (`estelle_live_167fd7c…`), and the blunt rule turned every row into
//! `[credential hidden]`: a list of two hidden things, over a heading that promised prefixes.
//!
//! ⚠️ **THIS FILE IS NOT A HOLE IN THE FENCE AND MUST NEVER BECOME ONE.** The pass is granted on
//! LENGTH before content: a value long enough to *be* a key cannot take it, because the body
//! between the marker and the elision mark is capped well under the shortest string
//! [`is_secret_shaped`] will call a credential, and that predicate is asked as well. Both checks
//! must fail for a value to be shown, so widening either one alone does not open the gate — and
//! `a_credential_shape_and_a_rendered_key_row_cannot_both_exist` in the TUI proves it over the
//! shipped renderer rather than over a hand-built string.
//!
//! ⚠️ **WHAT THIS DOES NOT CLAIM.** It judges TOKENS. A credential containing whitespace would be
//! judged in pieces — no vendor issues one, and no shape in the fence's catalogue permits it, but
//! that is an assumption written down rather than a property proven.

use crate::auth::is_secret_shaped;
use crate::auth::mask_secret;

/// The mark the server appends when it cuts a key down for display (`keyring.key_prefix`).
const ELISION: char = '…';

/// The largest body — the characters between the vendor marker and [`ELISION`] — this side will
/// show.
///
/// The server shows 20 characters of an `estelle_live_` key, so 7 body characters survive. This
/// bound is deliberately looser than 7, so a display-length change on the server does not
/// silently start masking every row, and deliberately far tighter than the 12 the credential
/// regex needs before it will call something an Estelle key, so no real key can reach it.
const MAX_ELIDED_BODY: usize = 10;

/// The vendor markers a key row may legitimately carry, elided. Kept in step with the markers
/// [`mask_secret`] refuses outright — this is the same list, read for the opposite purpose.
const MARKERS: [&str; 4] = ["estelle_live_", "sk-", "ghp_", "github_pat_"];

/// True only for a value the SERVER has already elided to a display prefix.
///
/// Every clause is a refusal: no elision mark, no marker at position zero, an empty body, a body
/// past [`MAX_ELIDED_BODY`], a body character outside the key alphabet, or a value the credential
/// fence recognises — any one of those and the answer is false.
pub fn is_elided_key_prefix(value: &str) -> bool {
    let Some(body) = value.strip_suffix(ELISION) else {
        return false;
    };
    let Some(marker) = MARKERS.iter().find(|marker| body.starts_with(**marker)) else {
        return false;
    };
    let body = &body[marker.len()..];
    !body.is_empty()
        && body.len() <= MAX_ELIDED_BODY
        && body
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
        && !is_secret_shaped(value)
}

/// Mask one rendered key-inventory row, token by token, preserving the caller's exact spacing.
///
/// Token granularity is strictly tighter than the whole-line rule it replaces on this surface: a
/// credential anywhere in the row is still replaced in full, and only the elided prefix — the
/// thing the row exists to show — survives beside it.
pub fn mask_key_row(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    // BOUND (Power of Ten rule 2): every pass consumes at least one byte of `rest`, so this runs
    // at most `line.len()` times. The `is_empty` break below is what makes that true when a token
    // ends the line.
    while !rest.is_empty() {
        let gap = rest
            .find(|character: char| !character.is_whitespace())
            .unwrap_or(rest.len());
        let (whitespace, tail) = rest.split_at(gap);
        out.push_str(whitespace);
        if tail.is_empty() {
            break;
        }
        let end = tail.find(char::is_whitespace).unwrap_or(tail.len());
        let (token, remainder) = tail.split_at(end);
        out.push_str(&mask_token(token));
        rest = remainder;
    }
    out
}

fn mask_token(token: &str) -> String {
    if is_elided_key_prefix(token) {
        token.to_string()
    } else {
        mask_secret(token)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::find_secret_shape;

    /// The server's real shape: `estelle_live_` + 7 body characters + the elision mark.
    const ELIDED: &str = "estelle_live_167fd7c…";
    /// A key long enough for the fence to name. Test-only, and it is not a credential: the body
    /// is a single repeated letter, which is why it is safe to write down.
    const FULL: &str = "estelle_live_aaaaaaaaaaaaaaaaaaaaaaaa";

    #[test]
    fn the_elided_prefix_the_server_sends_is_shown_and_a_full_key_is_not() {
        assert!(is_elided_key_prefix(ELIDED));
        assert!(!is_elided_key_prefix(FULL));
        // The instrument can fail: the value the pass refuses is one the fence itself names.
        assert!(find_secret_shape(FULL).is_some(), "the probe is not a key");
        assert!(find_secret_shape(ELIDED).is_none(), "the probe is a key");
    }

    #[test]
    fn every_clause_of_the_pass_is_a_refusal_on_its_own() {
        assert!(!is_elided_key_prefix("estelle_live_167fd7c"), "no elision");
        assert!(!is_elided_key_prefix("estelle_live_…"), "empty body");
        assert!(!is_elided_key_prefix("x estelle_live_1a…"), "not at zero");
        assert!(!is_elided_key_prefix("nope_1a…"), "no marker");
        assert!(
            !is_elided_key_prefix("estelle_live_1a2b3c4d5e6f…"),
            "too long"
        );
        assert!(
            !is_elided_key_prefix("estelle_live_1a.2b…"),
            "outside alphabet"
        );
        assert!(!is_elided_key_prefix(""), "empty");
    }

    #[test]
    fn a_row_keeps_its_words_and_its_prefix_and_loses_a_key() {
        let row = format!("  1  laptop  ·  {ELIDED}  ·  created 2026-07-11  ·  ACTIVE");
        let masked = mask_key_row(&row);
        assert_eq!(masked, row, "an ordinary row must survive unchanged");

        let leaked = format!("  1  laptop  ·  {FULL}  ·  created 2026-07-11  ·  ACTIVE");
        let masked = mask_key_row(&leaked);
        assert!(masked.contains("[credential hidden]"), "{masked}");
        assert!(
            masked.contains("laptop"),
            "the row lost its label: {masked}"
        );
        assert!(masked.contains("created 2026-07-11"), "{masked}");
        assert!(find_secret_shape(&masked).is_none(), "{masked}");
    }

    #[test]
    fn spacing_survives_masking_byte_for_byte_when_nothing_is_masked() {
        for line in ["", " ", "a", " a ", "a  b", "a\tb", "  1  x  ·  y  ", "…"] {
            assert_eq!(mask_key_row(line), line, "spacing changed for {line:?}");
        }
    }
}
