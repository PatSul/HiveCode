//! OAuth 2.0 client with PKCE support.
//!
//! Implements the Authorization Code flow with Proof Key for Code Exchange (PKCE)
//! using only `reqwest` for HTTP and `sha2` for the code challenge.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rand::Rng;
use reqwest::Client;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tracing::debug;

// ── Configuration ──────────────────────────────────────────────────

/// Configuration required to initiate an OAuth 2.0 flow.
#[derive(Debug, Clone)]
pub struct OAuthConfig {
    pub client_id: String,
    pub client_secret: Option<String>,
    pub auth_url: String,
    pub token_url: String,
    pub redirect_uri: String,
    pub scopes: Vec<String>,
}

// ── Token ──────────────────────────────────────────────────────────

/// An OAuth 2.0 token returned by the authorization server.
#[derive(Debug, Clone)]
pub struct OAuthToken {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub token_type: String,
}

/// Raw JSON shape returned by the token endpoint.
#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<i64>,
    token_type: Option<String>,
}

// ── PKCE helpers ───────────────────────────────────────────────────

/// Generate a cryptographically random code verifier (43-128 unreserved characters).
fn generate_code_verifier() -> String {
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~";
    let mut rng = rand::rng();
    let len = rng.random_range(43..=128);
    (0..len)
        .map(|_| {
            let idx = rng.random_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}

/// Compute the S256 code challenge: BASE64URL(SHA256(code_verifier)).
fn compute_code_challenge(verifier: &str) -> String {
    let hash = Sha256::digest(verifier.as_bytes());
    base64url_encode(&hash)
}

/// BASE64-URL encode without padding (RFC 7636 appendix A).
fn base64url_encode(data: &[u8]) -> String {
    use std::fmt::Write;
    let mut buf = String::with_capacity(data.len() * 4 / 3 + 4);
    // Standard base64 table
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut i = 0;
    while i + 2 < data.len() {
        let n = ((data[i] as u32) << 16) | ((data[i + 1] as u32) << 8) | (data[i + 2] as u32);
        let _ = buf.write_char(TABLE[((n >> 18) & 0x3F) as usize] as char);
        let _ = buf.write_char(TABLE[((n >> 12) & 0x3F) as usize] as char);
        let _ = buf.write_char(TABLE[((n >> 6) & 0x3F) as usize] as char);
        let _ = buf.write_char(TABLE[(n & 0x3F) as usize] as char);
        i += 3;
    }
    let remaining = data.len() - i;
    if remaining == 2 {
        let n = ((data[i] as u32) << 16) | ((data[i + 1] as u32) << 8);
        let _ = buf.write_char(TABLE[((n >> 18) & 0x3F) as usize] as char);
        let _ = buf.write_char(TABLE[((n >> 12) & 0x3F) as usize] as char);
        let _ = buf.write_char(TABLE[((n >> 6) & 0x3F) as usize] as char);
        // no padding
    } else if remaining == 1 {
        let n = (data[i] as u32) << 16;
        let _ = buf.write_char(TABLE[((n >> 18) & 0x3F) as usize] as char);
        let _ = buf.write_char(TABLE[((n >> 12) & 0x3F) as usize] as char);
        // no padding
    }

    // Convert standard base64 to base64url: '+' -> '-', '/' -> '_'
    buf.replace('+', "-").replace('/', "_")
}

/// Generate a random state string for CSRF protection.
fn generate_state() -> String {
    let bytes: [u8; 16] = rand::random();
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

// ── Client ─────────────────────────────────────────────────────────

/// OAuth 2.0 client that manages the authorization code + PKCE flow.
pub struct OAuthClient {
    config: OAuthConfig,
    client: Client,
    code_verifier: String,
}

impl OAuthClient {
    /// Create a new OAuth client from the given configuration.
    pub fn new(config: OAuthConfig) -> Self {
        let code_verifier = generate_code_verifier();
        Self {
            config,
            client: Client::new(),
            code_verifier,
        }
    }

    /// Build the authorization URL the user should visit.
    ///
    /// Returns `(url, state)` where `state` must be verified on callback.
    pub fn authorization_url(&self) -> (String, String) {
        let state = generate_state();
        let challenge = compute_code_challenge(&self.code_verifier);
        let scope = self.config.scopes.join(" ");

        let url = format!(
            "{}?response_type=code\
             &client_id={}\
             &redirect_uri={}\
             &scope={}\
             &state={}\
             &code_challenge={}\
             &code_challenge_method=S256",
            self.config.auth_url,
            urlencod(&self.config.client_id),
            urlencod(&self.config.redirect_uri),
            urlencod(&scope),
            urlencod(&state),
            urlencod(&challenge),
        );

        debug!(url = %url, "built authorization URL");
        (url, state)
    }

    /// Exchange an authorization code for tokens.
    pub async fn exchange_code(&self, code: &str) -> Result<OAuthToken> {
        let mut params = vec![
            ("grant_type", "authorization_code".to_string()),
            ("code", code.to_string()),
            ("redirect_uri", self.config.redirect_uri.clone()),
            ("client_id", self.config.client_id.clone()),
            ("code_verifier", self.code_verifier.clone()),
        ];

        if let Some(ref secret) = self.config.client_secret {
            params.push(("client_secret", secret.clone()));
        }

        let resp = self
            .client
            .post(&self.config.token_url)
            .form(&params)
            .send()
            .await
            .context("token exchange request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("token exchange failed ({}): {}", status, body);
        }

        let raw: TokenResponse = resp
            .json()
            .await
            .context("failed to parse token response")?;
        Ok(to_oauth_token(raw))
    }

    /// Refresh an expired token using the refresh_token grant.
    pub async fn refresh_token(&self, token: &OAuthToken) -> Result<OAuthToken> {
        let refresh = token
            .refresh_token
            .as_deref()
            .context("no refresh token available")?;

        let mut params = vec![
            ("grant_type", "refresh_token".to_string()),
            ("refresh_token", refresh.to_string()),
            ("client_id", self.config.client_id.clone()),
        ];

        if let Some(ref secret) = self.config.client_secret {
            params.push(("client_secret", secret.clone()));
        }

        let resp = self
            .client
            .post(&self.config.token_url)
            .form(&params)
            .send()
            .await
            .context("token refresh request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("token refresh failed ({}): {}", status, body);
        }

        let raw: TokenResponse = resp
            .json()
            .await
            .context("failed to parse refresh response")?;
        Ok(to_oauth_token(raw))
    }

    /// Check whether a token has expired (with a 30-second safety margin).
    pub fn is_expired(token: &OAuthToken) -> bool {
        match token.expires_at {
            Some(at) => Utc::now() >= at - chrono::Duration::seconds(30),
            None => false, // no expiry info; assume still valid
        }
    }

    /// Return the PKCE code verifier (exposed for testing).
    #[cfg(test)]
    fn code_verifier(&self) -> &str {
        &self.code_verifier
    }
}

/// Convert the raw token response into our domain type.
fn to_oauth_token(raw: TokenResponse) -> OAuthToken {
    let expires_at = raw
        .expires_in
        .map(|secs| Utc::now() + chrono::Duration::seconds(secs));

    OAuthToken {
        access_token: raw.access_token,
        refresh_token: raw.refresh_token,
        expires_at,
        token_type: raw.token_type.unwrap_or_else(|| "Bearer".to_string()),
    }
}

/// Minimal percent-encoding for query parameters.
fn urlencod(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 2);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => {
                out.push('%');
                out.push(char::from(b"0123456789ABCDEF"[(b >> 4) as usize]));
                out.push(char::from(b"0123456789ABCDEF"[(b & 0x0F) as usize]));
            }
        }
    }
    out
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_config() -> OAuthConfig {
        OAuthConfig {
            client_id: "test-client-id".into(),
            client_secret: Some("test-secret".into()),
            auth_url: "https://accounts.google.com/o/oauth2/v2/auth".into(),
            token_url: "https://oauth2.googleapis.com/token".into(),
            redirect_uri: "http://localhost:8080/callback".into(),
            scopes: vec![
                "openid".into(),
                "email".into(),
                "https://www.googleapis.com/auth/drive.readonly".into(),
            ],
        }
    }

    #[test]
    fn test_config_fields() {
        let cfg = sample_config();
        assert_eq!(cfg.client_id, "test-client-id");
        assert!(cfg.client_secret.is_some());
        assert_eq!(cfg.scopes.len(), 3);
    }

    #[test]
    fn test_authorization_url_contains_pkce_params() {
        let client = OAuthClient::new(sample_config());
        let (url, state) = client.authorization_url();

        assert!(url.contains("response_type=code"));
        assert!(url.contains("client_id=test-client-id"));
        assert!(url.contains("code_challenge="));
        assert!(url.contains("code_challenge_method=S256"));
        assert!(url.contains(&format!("state={state}")));
        assert!(url.starts_with("https://accounts.google.com/o/oauth2/v2/auth?"));
    }

    #[test]
    fn test_code_verifier_length() {
        let client = OAuthClient::new(sample_config());
        let verifier = client.code_verifier();
        assert!(
            verifier.len() >= 43 && verifier.len() <= 128,
            "verifier length {} out of range",
            verifier.len()
        );
    }

    #[test]
    fn test_code_verifier_charset() {
        let client = OAuthClient::new(sample_config());
        let verifier = client.code_verifier();
        for ch in verifier.chars() {
            assert!(
                ch.is_ascii_alphanumeric() || ch == '-' || ch == '.' || ch == '_' || ch == '~',
                "invalid character in verifier: {ch}"
            );
        }
    }

    #[test]
    fn test_code_challenge_is_valid_base64url() {
        let challenge = compute_code_challenge("test-verifier-string");
        // Base64url has no '+', '/', or '=' characters.
        assert!(!challenge.contains('+'), "challenge contains +");
        assert!(!challenge.contains('/'), "challenge contains /");
        assert!(!challenge.contains('='), "challenge contains =");
        // SHA-256 produces 32 bytes -> 43 base64url chars (no padding).
        assert_eq!(challenge.len(), 43);
    }

    #[test]
    fn test_code_challenge_deterministic() {
        let a = compute_code_challenge("deterministic-verifier");
        let b = compute_code_challenge("deterministic-verifier");
        assert_eq!(a, b, "same verifier should produce same challenge");
    }

    #[test]
    fn test_is_expired_with_future_token() {
        let token = OAuthToken {
            access_token: "tok".into(),
            refresh_token: None,
            expires_at: Some(Utc::now() + chrono::Duration::hours(1)),
            token_type: "Bearer".into(),
        };
        assert!(!OAuthClient::is_expired(&token));
    }

    #[test]
    fn test_is_expired_with_past_token() {
        let token = OAuthToken {
            access_token: "tok".into(),
            refresh_token: None,
            expires_at: Some(Utc::now() - chrono::Duration::hours(1)),
            token_type: "Bearer".into(),
        };
        assert!(OAuthClient::is_expired(&token));
    }

    #[test]
    fn test_is_expired_none_expiry() {
        let token = OAuthToken {
            access_token: "tok".into(),
            refresh_token: None,
            expires_at: None,
            token_type: "Bearer".into(),
        };
        assert!(
            !OAuthClient::is_expired(&token),
            "no expiry should be treated as not expired"
        );
    }

    #[test]
    fn test_urlencod_preserves_unreserved() {
        assert_eq!(urlencod("abc-_.~XYZ019"), "abc-_.~XYZ019");
    }

    #[test]
    fn test_urlencod_encodes_special() {
        assert_eq!(urlencod("a b"), "a%20b");
        assert_eq!(urlencod("hello@world"), "hello%40world");
    }

    #[test]
    fn test_generate_state_is_hex() {
        let state = generate_state();
        assert_eq!(state.len(), 32); // 16 bytes -> 32 hex chars
        assert!(state.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_config_without_client_secret() {
        let cfg = OAuthConfig {
            client_id: "public-client".into(),
            client_secret: None,
            auth_url: "https://example.com/auth".into(),
            token_url: "https://example.com/token".into(),
            redirect_uri: "http://localhost:3000".into(),
            scopes: vec!["read".into()],
        };
        let client = OAuthClient::new(cfg);
        let (url, _state) = client.authorization_url();
        assert!(url.contains("client_id=public-client"));
    }
}
