use std::collections::HashMap;
use std::io::{Read as _, Write as _};
use std::net::TcpListener;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::{DateTime, Utc};
use rand::Rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use url::Url;

use crate::adapter::keyring_store::KeyringStore;
use crate::domain::model::Platform;
use crate::error::{CoreError, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthToken {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub token_type: String,
    pub scope: Option<String>,
}

impl OAuthToken {
    pub fn is_expired(&self) -> bool {
        self.expires_at
            .map(|exp| Utc::now() >= exp)
            .unwrap_or(false)
    }
}

#[derive(Debug, Clone)]
pub struct OAuthConfig {
    pub platform: Platform,
    pub client_id: String,
    pub client_secret: String,
    pub auth_url: String,
    pub token_url: String,
    pub redirect_port: u16,
    pub scopes: Vec<String>,
    pub use_pkce: bool,
    pub extra_auth_params: HashMap<String, String>,
}

impl OAuthConfig {
    pub fn redirect_uri(&self) -> String {
        format!("http://localhost:{}", self.redirect_port)
    }
}

pub struct AuthCodeResult {
    pub code: String,
    pub pkce_verifier: Option<String>,
}

pub struct PkceChallenge {
    pub verifier: String,
    pub challenge: String,
}

pub fn generate_pkce() -> PkceChallenge {
    let verifier_bytes: Vec<u8> = (0..32).map(|_| rand::rng().random::<u8>()).collect();
    let verifier = URL_SAFE_NO_PAD.encode(&verifier_bytes);

    let mut hasher = Sha256::new();
    hasher.update(verifier.as_bytes());
    let challenge = URL_SAFE_NO_PAD.encode(hasher.finalize());

    PkceChallenge {
        verifier,
        challenge,
    }
}

pub fn generate_state() -> String {
    let bytes: Vec<u8> = (0..16).map(|_| rand::rng().random::<u8>()).collect();
    URL_SAFE_NO_PAD.encode(&bytes)
}

pub fn build_auth_url(config: &OAuthConfig, state: &str, pkce: Option<&PkceChallenge>) -> String {
    let mut url = Url::parse(&config.auth_url).expect("Invalid auth URL");

    {
        let mut params = url.query_pairs_mut();
        params.append_pair("client_id", &config.client_id);
        params.append_pair("redirect_uri", &config.redirect_uri());
        params.append_pair("response_type", "code");
        params.append_pair("state", state);

        if !config.scopes.is_empty() {
            params.append_pair("scope", &config.scopes.join(" "));
        }

        if let Some(pkce) = pkce {
            params.append_pair("code_challenge", &pkce.challenge);
            params.append_pair("code_challenge_method", "S256");
        }

        for (k, v) in &config.extra_auth_params {
            params.append_pair(k, v);
        }
    }

    url.to_string()
}

pub fn wait_for_callback(port: u16, expected_state: &str, platform: Platform) -> Result<String> {
    let listener = TcpListener::bind(format!("127.0.0.1:{port}")).map_err(|e| CoreError::Auth {
        platform,
        reason: format!("Failed to bind callback server: {e}"),
    })?;

    let (mut stream, _) = listener.accept().map_err(|e| CoreError::Auth {
        platform,
        reason: format!("Failed to accept callback: {e}"),
    })?;

    let mut buf = [0u8; 4096];
    let n = stream.read(&mut buf).map_err(|e| CoreError::Auth {
        platform,
        reason: format!("Failed to read callback: {e}"),
    })?;

    let request = String::from_utf8_lossy(&buf[..n]);
    let path = request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("/");

    let response_html = r#"<html><body><h2>Authorization complete!</h2><p>You can close this window.</p><script>window.close()</script></body></html>"#;
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\n\r\n{}",
        response_html.len(),
        response_html
    );
    let _ = stream.write_all(response.as_bytes());

    let callback_url =
        Url::parse(&format!("http://localhost:{port}{path}")).map_err(|_| CoreError::Auth {
            platform,
            reason: "Invalid callback URL".into(),
        })?;

    let params: HashMap<String, String> = callback_url.query_pairs().into_owned().collect();

    if let Some(error) = params.get("error") {
        return Err(CoreError::Auth {
            platform,
            reason: format!("OAuth error: {error}"),
        });
    }

    let state = params.get("state").map(|s| s.as_str()).unwrap_or("");
    if state != expected_state {
        return Err(CoreError::Auth {
            platform,
            reason: "State mismatch — possible CSRF attack".into(),
        });
    }

    params.get("code").cloned().ok_or_else(|| CoreError::Auth {
        platform,
        reason: "No authorization code in callback".into(),
    })
}

pub async fn exchange_code(
    config: &OAuthConfig,
    code: &str,
    pkce_verifier: Option<&str>,
) -> Result<OAuthToken> {
    let client = reqwest::Client::new();
    let mut params = HashMap::new();
    params.insert("grant_type", "authorization_code".to_string());
    params.insert("code", code.to_string());
    params.insert("redirect_uri", config.redirect_uri());
    params.insert("client_id", config.client_id.clone());
    params.insert("client_secret", config.client_secret.clone());

    if let Some(verifier) = pkce_verifier {
        params.insert("code_verifier", verifier.to_string());
    }

    let platform = config.platform;
    let resp = client
        .post(&config.token_url)
        .form(&params)
        .send()
        .await
        .map_err(|e| CoreError::Auth {
            platform,
            reason: format!("Token exchange failed: {e}"),
        })?;

    let status = resp.status();
    let body = resp.text().await.map_err(|e| CoreError::Auth {
        platform,
        reason: format!("Failed to read token response: {e}"),
    })?;

    if !status.is_success() {
        return Err(CoreError::Auth {
            platform,
            reason: format!("Token exchange returned {status}: {body}"),
        });
    }

    parse_token_response(&body, platform)
}

pub async fn refresh_token(config: &OAuthConfig, refresh_tok: &str) -> Result<OAuthToken> {
    let client = reqwest::Client::new();
    let params = [
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_tok),
        ("client_id", &config.client_id),
        ("client_secret", &config.client_secret),
    ];

    let platform = config.platform;
    let resp = client
        .post(&config.token_url)
        .form(&params)
        .send()
        .await
        .map_err(|e| CoreError::Auth {
            platform,
            reason: format!("Token refresh failed: {e}"),
        })?;

    let status = resp.status();
    let body = resp.text().await.map_err(|e| CoreError::Auth {
        platform,
        reason: format!("Failed to read refresh response: {e}"),
    })?;

    if !status.is_success() {
        return Err(CoreError::Auth {
            platform,
            reason: format!("Token refresh returned {status}: {body}"),
        });
    }

    let mut token = parse_token_response(&body, platform)?;
    if token.refresh_token.is_none() {
        token.refresh_token = Some(refresh_tok.to_string());
    }
    Ok(token)
}

fn parse_token_response(body: &str, platform: Platform) -> Result<OAuthToken> {
    let json: serde_json::Value = serde_json::from_str(body).map_err(|e| CoreError::Auth {
        platform,
        reason: format!("Invalid token JSON: {e}"),
    })?;

    let access_token = json["access_token"]
        .as_str()
        .ok_or_else(|| CoreError::Auth {
            platform,
            reason: "Missing access_token".into(),
        })?
        .to_string();

    let refresh_token = json["refresh_token"].as_str().map(|s| s.to_string());
    let token_type = json["token_type"].as_str().unwrap_or("Bearer").to_string();
    let scope = json["scope"].as_str().map(|s| s.to_string());

    let expires_at = json["expires_in"]
        .as_i64()
        .map(|secs| Utc::now() + chrono::Duration::seconds(secs));

    Ok(OAuthToken {
        access_token,
        refresh_token,
        expires_at,
        token_type,
        scope,
    })
}

pub fn save_token(platform: Platform, token: &OAuthToken) -> Result<()> {
    let json = serde_json::to_string(token).map_err(|e| CoreError::Auth {
        platform,
        reason: format!("Failed to serialize token: {e}"),
    })?;
    KeyringStore::store_token(platform, &json)
}

pub fn load_token(platform: Platform) -> Result<Option<OAuthToken>> {
    match KeyringStore::get_token(platform)? {
        Some(json) => {
            let token: OAuthToken = serde_json::from_str(&json).map_err(|e| CoreError::Auth {
                platform,
                reason: format!("Failed to deserialize token: {e}"),
            })?;
            Ok(Some(token))
        }
        None => Ok(None),
    }
}

pub async fn ensure_valid_token(config: &OAuthConfig) -> Result<OAuthToken> {
    let platform = config.platform;
    let token = load_token(platform)?.ok_or_else(|| CoreError::Auth {
        platform,
        reason: "Not authenticated. Run auth first.".into(),
    })?;

    if !token.is_expired() {
        return Ok(token);
    }

    if let Some(ref refresh_tok) = token.refresh_token {
        let new_token = refresh_token(config, refresh_tok).await?;
        save_token(platform, &new_token)?;
        Ok(new_token)
    } else {
        Err(CoreError::Auth {
            platform,
            reason: "Token expired and no refresh token available".into(),
        })
    }
}

pub fn run_auth_flow(config: &OAuthConfig) -> Result<AuthCodeResult> {
    let state = generate_state();
    let pkce = if config.use_pkce {
        Some(generate_pkce())
    } else {
        None
    };

    let auth_url = build_auth_url(config, &state, pkce.as_ref());
    log::info!("Opening browser for authorization...");
    let _ = open::that(&auth_url);

    let code = wait_for_callback(config.redirect_port, &state, config.platform)?;
    log::info!("Received authorization code");

    Ok(AuthCodeResult {
        code,
        pkce_verifier: pkce.map(|p| p.verifier),
    })
}

pub async fn perform_full_auth(config: &OAuthConfig) -> Result<OAuthToken> {
    let config_for_blocking = config.clone();
    let platform = config.platform;

    let result = tokio::task::spawn_blocking(move || run_auth_flow(&config_for_blocking))
        .await
        .map_err(|e| CoreError::Auth {
            platform,
            reason: format!("Auth flow task failed: {e}"),
        })??;

    let token = exchange_code(config, &result.code, result.pkce_verifier.as_deref()).await?;
    save_token(config.platform, &token)?;
    Ok(token)
}
