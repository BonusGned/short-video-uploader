use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use reqwest::header::{AUTHORIZATION, CONTENT_LENGTH, CONTENT_TYPE};
use tokio::io::AsyncReadExt;

use crate::adapter::oauth::{self, OAuthConfig, OAuthToken, ensure_valid_token, perform_full_auth};
use crate::domain::model::{Platform, UploadResult, VideoMetadata};
use crate::domain::port::{AsyncUploader, ProgressCallback, UploadProgress};
use crate::error::{CoreError, Result};

const YOUTUBE_AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const YOUTUBE_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const YOUTUBE_UPLOAD_URL: &str =
    "https://www.googleapis.com/upload/youtube/v3/videos?uploadType=resumable&part=snippet,status";
const REDIRECT_PORT: u16 = 8585;

pub struct YouTubeUploader {
    oauth_config: OAuthConfig,
    client: reqwest::Client,
    authenticated: AtomicBool,
}

impl YouTubeUploader {
    pub fn new(client_id: String, client_secret: String) -> Self {
        let oauth_config = OAuthConfig {
            platform: Platform::YouTube,
            client_id,
            client_secret,
            auth_url: YOUTUBE_AUTH_URL.into(),
            token_url: YOUTUBE_TOKEN_URL.into(),
            redirect_port: REDIRECT_PORT,
            scopes: vec![
                "https://www.googleapis.com/auth/youtube.upload".into(),
                "https://www.googleapis.com/auth/youtube".into(),
            ],
            use_pkce: false,
            extra_auth_params: HashMap::from([("access_type".into(), "offline".into())]),
        };

        let authenticated = oauth::load_token(Platform::YouTube)
            .ok()
            .flatten()
            .is_some();

        Self {
            oauth_config,
            client: reqwest::Client::new(),
            authenticated: AtomicBool::new(authenticated),
        }
    }

    async fn get_token(&self) -> Result<OAuthToken> {
        ensure_valid_token(&self.oauth_config).await
    }

    async fn initiate_resumable_upload(
        &self,
        token: &OAuthToken,
        metadata: &VideoMetadata,
        file_size: u64,
    ) -> Result<String> {
        let body = serde_json::json!({
            "snippet": {
                "title": metadata.title,
                "description": metadata.description,
                "tags": metadata.tags,
                "categoryId": "22"
            },
            "status": {
                "privacyStatus": "public",
                "selfDeclaredMadeForKids": false,
                "madeForKids": false
            }
        });

        let resp = self.client
            .post(YOUTUBE_UPLOAD_URL)
            .header(AUTHORIZATION, format!("Bearer {}", token.access_token))
            .header(CONTENT_TYPE, "application/json; charset=UTF-8")
            .header("X-Upload-Content-Length", file_size.to_string())
            .header("X-Upload-Content-Type", "video/*")
            .json(&body)
            .send()
            .await
            .map_err(|e| CoreError::Upload {
                platform: Platform::YouTube,
                reason: format!("Resumable upload init failed: {e}"),
            })?;

        let status = resp.status();
        let upload_url = resp
            .headers()
            .get("location")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        if !status.is_success() || upload_url.is_none() {
            return Err(CoreError::Upload {
                platform: Platform::YouTube,
                reason: format!("Failed to initiate upload (HTTP {status})"),
            });
        }

        Ok(upload_url.unwrap())
    }
}

#[async_trait]
impl AsyncUploader for YouTubeUploader {
    fn platform(&self) -> Platform {
        Platform::YouTube
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
        let token = self.get_token().await?;

        let mut file = tokio::fs::File::open(&metadata.video_path)
            .await
            .map_err(|e| CoreError::Upload {
                platform: Platform::YouTube,
                reason: format!("Cannot open video: {e}"),
            })?;

        let total_bytes = file
            .metadata()
            .await
            .map(|m| m.len())
            .unwrap_or(0);

        let upload_url = self.initiate_resumable_upload(&token, metadata, total_bytes).await?;

        let mut buffer = Vec::with_capacity(total_bytes as usize);
        file.read_to_end(&mut buffer).await.map_err(|e| CoreError::Upload {
            platform: Platform::YouTube,
            reason: format!("Cannot read video: {e}"),
        })?;

        on_progress(UploadProgress {
            bytes_sent: 0,
            total_bytes,
        });

        let resp = self.client
            .put(&upload_url)
            .header(AUTHORIZATION, format!("Bearer {}", token.access_token))
            .header(CONTENT_TYPE, "video/*")
            .header(CONTENT_LENGTH, total_bytes.to_string())
            .body(buffer)
            .send()
            .await
            .map_err(|e| CoreError::Upload {
                platform: Platform::YouTube,
                reason: format!("Upload failed: {e}"),
            })?;

        on_progress(UploadProgress {
            bytes_sent: total_bytes,
            total_bytes,
        });

        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();

        if !status.is_success() {
            return Ok(UploadResult::failed(
                Platform::YouTube,
                format!("HTTP {status}: {body}"),
            ));
        }

        let json: serde_json::Value = serde_json::from_str(&body).unwrap_or_default();
        let video_id = json["id"].as_str().unwrap_or("unknown");
        let url = format!("https://youtube.com/shorts/{video_id}");

        Ok(UploadResult::success(Platform::YouTube, url))
    }

    async fn is_authenticated(&self) -> bool {
        self.authenticated.load(Ordering::SeqCst)
    }
}
