use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use reqwest::header::AUTHORIZATION;

use crate::adapter::oauth::{self, OAuthConfig, OAuthToken, ensure_valid_token, perform_full_auth};
use crate::domain::model::{Platform, UploadResult, VideoMetadata};
use crate::domain::port::{AsyncUploader, ProgressCallback, UploadProgress};
use crate::error::{CoreError, Result};

const FB_AUTH_URL: &str = "https://www.facebook.com/v21.0/dialog/oauth";
const FB_TOKEN_URL: &str = "https://graph.facebook.com/v21.0/oauth/access_token";
const GRAPH_API: &str = "https://graph.facebook.com/v21.0";
const REDIRECT_PORT: u16 = 8587;

const STATUS_POLL_INTERVAL: Duration = Duration::from_secs(5);
const STATUS_POLL_MAX_ATTEMPTS: usize = 60;

pub struct InstagramUploader {
    oauth_config: OAuthConfig,
    ig_user_id: String,
    client: reqwest::Client,
    authenticated: AtomicBool,
}

impl InstagramUploader {
    pub fn new(client_id: String, client_secret: String, ig_user_id: String) -> Self {
        let oauth_config = OAuthConfig {
            platform: Platform::Instagram,
            client_id,
            client_secret,
            auth_url: FB_AUTH_URL.into(),
            token_url: FB_TOKEN_URL.into(),
            redirect_port: REDIRECT_PORT,
            scopes: vec![
                "instagram_basic".into(),
                "instagram_content_publish".into(),
                "pages_read_engagement".into(),
            ],
            use_pkce: false,
            extra_auth_params: HashMap::new(),
        };

        let authenticated = oauth::load_token(Platform::Instagram)
            .ok()
            .flatten()
            .is_some();

        Self {
            oauth_config,
            ig_user_id,
            client: reqwest::Client::new(),
            authenticated: AtomicBool::new(authenticated),
        }
    }

    fn bearer(token: &OAuthToken) -> String {
        format!("Bearer {}", token.access_token)
    }

    async fn create_resumable_container(
        &self,
        token: &OAuthToken,
        metadata: &VideoMetadata,
    ) -> Result<ResumableSession> {
        let url = format!("{GRAPH_API}/{}/media", self.ig_user_id);
        let params = [
            ("media_type", "REELS"),
            ("upload_type", "resumable"),
            ("caption", metadata.description.as_str()),
        ];

        let resp = self
            .client
            .post(&url)
            .header(AUTHORIZATION, Self::bearer(token))
            .form(&params)
            .send()
            .await
            .map_err(|e| upload_err(format!("Container creation failed: {e}")))?;

        let (status, body) = read_response(resp).await;
        if !status.is_success() {
            return Err(upload_err(format!(
                "Container creation returned {status}: {body}"
            )));
        }

        let json = parse_json(&body)?;
        let id = json["id"]
            .as_str()
            .ok_or_else(|| upload_err(format!("No container id in response: {body}")))?
            .to_string();
        let uri = json["uri"]
            .as_str()
            .ok_or_else(|| upload_err(format!("No upload uri in response: {body}")))?
            .to_string();

        Ok(ResumableSession { id, uri })
    }

    async fn upload_video_bytes(
        &self,
        token: &OAuthToken,
        session: &ResumableSession,
        metadata: &VideoMetadata,
        on_progress: &ProgressCallback,
    ) -> Result<()> {
        let bytes = tokio::fs::read(&metadata.video_path)
            .await
            .map_err(|e| upload_err(format!("Cannot read video: {e}")))?;

        let total_bytes = bytes.len() as u64;
        if total_bytes == 0 {
            return Err(upload_err("Video file is empty"));
        }

        on_progress(UploadProgress {
            bytes_sent: 0,
            total_bytes,
        });

        let resp = self
            .client
            .post(&session.uri)
            .header(AUTHORIZATION, format!("OAuth {}", token.access_token))
            .header("offset", "0")
            .header("file_size", total_bytes.to_string())
            .body(bytes)
            .send()
            .await
            .map_err(|e| upload_err(format!("Video upload failed: {e}")))?;

        let (status, body) = read_response(resp).await;
        if !status.is_success() {
            return Err(upload_err(format!(
                "Video upload returned {status}: {body}"
            )));
        }

        on_progress(UploadProgress {
            bytes_sent: total_bytes,
            total_bytes,
        });
        Ok(())
    }

    async fn wait_for_container(&self, token: &OAuthToken, container_id: &str) -> Result<()> {
        let url = format!("{GRAPH_API}/{container_id}?fields=status_code");
        let bearer = Self::bearer(token);

        for _ in 0..STATUS_POLL_MAX_ATTEMPTS {
            let resp = self
                .client
                .get(&url)
                .header(AUTHORIZATION, &bearer)
                .send()
                .await
                .map_err(|e| upload_err(format!("Status check failed: {e}")))?;

            let (status, body) = read_response(resp).await;
            if !status.is_success() {
                return Err(upload_err(format!(
                    "Status check returned {status}: {body}"
                )));
            }

            let json = parse_json(&body)?;
            match json["status_code"].as_str() {
                Some("FINISHED") => return Ok(()),
                Some("ERROR") | Some("EXPIRED") => {
                    return Err(upload_err(format!(
                        "Container {container_id} processing failed: {body}"
                    )));
                }
                _ => {}
            }

            tokio::time::sleep(STATUS_POLL_INTERVAL).await;
        }

        Err(upload_err(format!(
            "Container {container_id} processing timed out after {}s",
            STATUS_POLL_INTERVAL.as_secs() * STATUS_POLL_MAX_ATTEMPTS as u64
        )))
    }

    async fn publish_container(&self, token: &OAuthToken, container_id: &str) -> Result<String> {
        let url = format!("{GRAPH_API}/{}/media_publish", self.ig_user_id);
        let params = [("creation_id", container_id)];

        let resp = self
            .client
            .post(&url)
            .header(AUTHORIZATION, Self::bearer(token))
            .form(&params)
            .send()
            .await
            .map_err(|e| upload_err(format!("Publish failed: {e}")))?;

        let (status, body) = read_response(resp).await;
        if !status.is_success() {
            return Err(upload_err(format!("Publish returned {status}: {body}")));
        }

        let json = parse_json(&body)?;
        let media_id = json["id"]
            .as_str()
            .ok_or_else(|| upload_err(format!("Missing media id in publish response: {body}")))?;
        Ok(format!("https://www.instagram.com/reel/{media_id}/"))
    }
}

struct ResumableSession {
    id: String,
    uri: String,
}

fn upload_err(reason: impl Into<String>) -> CoreError {
    CoreError::Upload {
        platform: Platform::Instagram,
        reason: reason.into(),
    }
}

fn parse_json(body: &str) -> Result<serde_json::Value> {
    serde_json::from_str(body)
        .map_err(|e| upload_err(format!("Invalid JSON response: {e}: {body}")))
}

async fn read_response(resp: reqwest::Response) -> (reqwest::StatusCode, String) {
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    (status, body)
}

#[async_trait]
impl AsyncUploader for InstagramUploader {
    fn platform(&self) -> Platform {
        Platform::Instagram
    }

    async fn authenticate(&self) -> Result<()> {
        perform_full_auth(&self.oauth_config).await?;
        self.authenticated.store(true, Ordering::SeqCst);
        Ok(())
    }

    async fn upload(
        &self,
        metadata: &VideoMetadata,
        on_progress: ProgressCallback,
    ) -> Result<UploadResult> {
        let token = ensure_valid_token(&self.oauth_config).await?;

        let session = self.create_resumable_container(&token, metadata).await?;
        self.upload_video_bytes(&token, &session, metadata, &on_progress)
            .await?;
        self.wait_for_container(&token, &session.id).await?;
        let url = self.publish_container(&token, &session.id).await?;

        Ok(UploadResult::success(Platform::Instagram, url))
    }

    async fn is_authenticated(&self) -> bool {
        self.authenticated.load(Ordering::SeqCst)
    }
}
