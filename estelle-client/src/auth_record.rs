//! The three-type client-side auth record — opencode parity (vendor-reference/opencode
//! packages/opencode/src/auth/index.ts), where our two credential shapes become three:
//! `api` (key + metadata), `oauth` (refresh, access, expires, accountId, enterpriseUrl), and
//! `wellknown` (key + token) for DISCOVERY-BASED auth. The third type is exactly what the MCP
//! lane is building server-side; this client-side record is how the two halves meet. The
//! serde tag and field names match opencode's on-disk JSON (`type`, `accountId`,
//! `enterpriseUrl`) so either side can read the other's file.
//!
//! Pure types and converters only — no consumer wiring. The MCP lane's server half lands
//! later; this is the meeting point.

use std::collections::BTreeMap;

use serde::Deserialize;
use serde::Serialize;

use crate::ApiKey;

/// One client-side auth record, serde-tagged on `type` exactly as opencode's `Info` union.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum AuthRecord {
    /// A bare API key, plus optional caller metadata.
    Api {
        key: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        metadata: Option<BTreeMap<String, String>>,
    },
    /// An OAuth credential: the refresh/access pair, the access token's expiry in
    /// MILLISECONDS since the epoch (opencode's `expires` is `Date.now() + expires_in * 1000`,
    /// not the JWT's seconds), and the account it belongs to.
    OAuth {
        refresh: String,
        access: String,
        expires: i64,
        #[serde(
            default,
            rename = "accountId",
            skip_serializing_if = "Option::is_none"
        )]
        account_id: Option<String>,
        /// Self-hosted/enterprise issuer seam. No estelle deployment uses one today — the
        /// field exists so the MCP lane's discovery half has somewhere to put it.
        #[serde(
            default,
            rename = "enterpriseUrl",
            skip_serializing_if = "Option::is_none"
        )]
        enterprise_url: Option<String>,
    },
    /// Discovery-based auth: a key naming the well-known document and the token it issued.
    WellKnown { key: String, token: String },
}

impl From<&ApiKey> for AuthRecord {
    fn from(key: &ApiKey) -> Self {
        Self::Api {
            key: key.expose().to_string(),
            metadata: None,
        }
    }
}

impl TryFrom<&codex_login::AuthDotJson> for AuthRecord {
    type Error = String;

    /// The OAuth half of a codex-login credential. `expires` comes from the access token's
    /// JWT `exp` (seconds → milliseconds); the account id prefers the stored field and falls
    /// back to the token claims via the opencode chain (id_token, then access token).
    fn try_from(auth: &codex_login::AuthDotJson) -> Result<Self, String> {
        let tokens = auth
            .tokens
            .as_ref()
            .ok_or_else(|| "the credential carries no OAuth tokens".to_string())?;
        let expires = codex_login::token_data::parse_jwt_expiration(&tokens.access_token)
            .ok()
            .flatten()
            .ok_or_else(|| {
                "the access token carries no parseable exp claim".to_string()
            })?
            .timestamp_millis();
        let account_id = tokens.account_id.clone().or_else(|| {
            codex_login::token_data::account_id_from_tokens(
                &tokens.id_token.raw_jwt,
                &tokens.access_token,
            )
        });
        Ok(Self::OAuth {
            refresh: tokens.refresh_token.clone(),
            access: tokens.access_token.clone(),
            expires,
            account_id,
            enterprise_url: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use base64::Engine;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use serde_json::json;

    use super::*;
    use crate::ApiKey;

    fn jwt(payload: serde_json::Value) -> String {
        let encode = |value: &serde_json::Value| {
            URL_SAFE_NO_PAD.encode(serde_json::to_vec(value).expect("jwt part"))
        };
        format!(
            "{}.{}.{}",
            encode(&json!({"alg": "none", "typ": "JWT"})),
            encode(&payload),
            URL_SAFE_NO_PAD.encode(b"sig")
        )
    }

    fn chatgpt_auth(account_id: Option<&str>, access_exp: i64) -> codex_login::AuthDotJson {
        codex_login::AuthDotJson {
            auth_mode: Some(codex_protocol::auth::AuthMode::Chatgpt),
            openai_api_key: None,
            tokens: Some(codex_login::token_data::TokenData {
                id_token: codex_login::token_data::parse_chatgpt_jwt_claims(&jwt(json!({
                    "https://api.openai.com/auth": {"chatgpt_account_id": "acct-from-jwt"}
                })))
                .expect("id token claims"),
                access_token: jwt(json!({"exp": access_exp})),
                refresh_token: "refresh-token".to_string(),
                account_id: account_id.map(str::to_string),
            }),
            last_refresh: None,
            agent_identity: None,
            personal_access_token: None,
            bedrock_api_key: None,
        }
    }

    #[test]
    fn api_record_round_trips_and_converts_from_the_estelle_key() {
        let key = ApiKey::new("estelle_live_test-only").expect("key");
        let record = AuthRecord::from(&key);
        let AuthRecord::Api { key, metadata } = &record else {
            panic!("an Estelle API key converts to the api record");
        };
        assert_eq!(key, "estelle_live_test-only");
        assert!(metadata.is_none());

        let encoded = serde_json::to_value(&record).expect("serialize");
        assert_eq!(encoded["type"], json!("api"));
        assert_eq!(encoded["key"], json!("estelle_live_test-only"));
        let decoded: AuthRecord = serde_json::from_value(encoded).expect("deserialize");
        assert_eq!(decoded, record);
    }

    #[test]
    fn authdotjson_maps_to_oauth_with_account_id_and_jwt_expiry() {
        // exp 1_900_000_000 (2030) → milliseconds, matching opencode's epoch-ms expires.
        let auth = chatgpt_auth(Some("acct-9"), 1_900_000_000);

        let record = AuthRecord::try_from(&auth).expect("oauth record");
        let AuthRecord::OAuth {
            refresh,
            access,
            expires,
            account_id,
            enterprise_url,
        } = &record
        else {
            panic!("a ChatGPT AuthDotJson converts to the oauth record");
        };
        assert_eq!(refresh, "refresh-token");
        assert_eq!(access, &jwt(json!({"exp": 1_900_000_000})));
        assert_eq!(*expires, 1_900_000_000_000);
        assert_eq!(account_id.as_deref(), Some("acct-9"));
        assert!(enterprise_url.is_none());

        let encoded = serde_json::to_value(&record).expect("serialize");
        assert_eq!(encoded["type"], json!("oauth"));
        // The two halves meet on opencode's field names, not ours.
        assert_eq!(encoded["accountId"], json!("acct-9"));
        let decoded: AuthRecord = serde_json::from_value(encoded).expect("deserialize");
        assert_eq!(decoded, record);
    }

    #[test]
    fn authdotjson_falls_back_to_the_jwt_claim_for_the_account_id() {
        let auth = chatgpt_auth(None, 1_900_000_000);

        let record = AuthRecord::try_from(&auth).expect("oauth record");

        let AuthRecord::OAuth { account_id, .. } = &record else {
            panic!("oauth record");
        };
        assert_eq!(account_id.as_deref(), Some("acct-from-jwt"));
    }

    #[test]
    fn authdotjson_without_tokens_cannot_become_an_oauth_record() {
        let mut auth = chatgpt_auth(Some("acct-9"), 1_900_000_000);
        auth.tokens = None;

        assert!(AuthRecord::try_from(&auth).is_err());
    }

    #[test]
    fn wellknown_record_round_trips() {
        let record = AuthRecord::WellKnown {
            key: "mcp-estelle".to_string(),
            token: "discovery-token".to_string(),
        };

        let encoded = serde_json::to_value(&record).expect("serialize");
        assert_eq!(
            encoded,
            json!({"type": "wellknown", "key": "mcp-estelle", "token": "discovery-token"})
        );
        let decoded: AuthRecord = serde_json::from_value(encoded).expect("deserialize");
        assert_eq!(decoded, record);
    }

    #[test]
    fn oauth_record_keeps_the_enterprise_url_seam() {
        let record = AuthRecord::OAuth {
            refresh: "r".to_string(),
            access: "a".to_string(),
            expires: 1_900_000_000_000,
            account_id: None,
            enterprise_url: Some("https://chatgpt.example-corp.com".to_string()),
        };

        let encoded = serde_json::to_value(&record).expect("serialize");
        assert_eq!(
            encoded["enterpriseUrl"],
            json!("https://chatgpt.example-corp.com")
        );
        let decoded: AuthRecord = serde_json::from_value(encoded).expect("deserialize");
        assert_eq!(decoded, record);
    }
}
