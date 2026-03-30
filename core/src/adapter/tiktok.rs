use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use reqwest::header::{AUTHORIZATION, CONTENT_LENGTH, CONTENT_RANGE, CONTENT_TYPE};
use tokio::io::AsyncReadExt;

use crate::adapter::oauth::{self, OAuthConfig, OAuthToken, ensure_valid_token, perform_full_auth};
use crate::domain::model::{Platform, UploadResult, VideoMetadata};
use crate::domain::port::{AsyncUploader, ProgressCallback, UploadProgress};
use crate::error::{CoreError, Result};

const TIKTOK_AUTH_URL: &str = "https://www.tiktok.com/v2/auth/authorize/";
const TIKTOK_TOKEN_URL: &str = "https://open.tiktokapis.com/v2/oauth/token/";
const TIKTOK_UPLOAD_INIT_URL: &str = "https://open.tiktokapis.com/v2/post/publish/inbox/video/init/";
const REDIRECT_PORT: u16 = 8586;

pub struct TikTokUploader {
    oauth_config: OAuthConfig,
    client: reqwest::Client,
    authenticated: AtomicBool,
}

impl TikTokUploader {
    pub fn new(client_id: String, client_secret: String) -> Self {
        let extra_client_key = client_id.clone();
        let oauth_config = OAuthConfig {
            platform: Platform::TikTok,
            client_id,
            client_secret,
            auth_url: TIKTOK_AUTH_URL.into(),
            token_url: TIKTOK_TOKEN_URL.into(),
            redirect_port: REDIRECT_PORT,
            scopes: vec!["video.upload".into(), "video.publish".into()],
            use_pkce: true,
            extra_auth_params: HashMap::from([("client_key".into(), extra_client_key)]),
        };

        let authenticated = oauth::load_token(Platform::TikTok)
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

    async fn init_upload(&self, token: &OAuthToken, file_size: u64) -> Result<String> {
        let body = serde_json::json!({
            "source_info": {
                "source": "FILE_UPLOAD",
                "video_size": file_size,
                "chunk_size": file_size,
                "total_chunk_count": 1
            }
        });

        let resp = self.client
            .post(TIKTOK_UPLOAD_INIT_URL)
            .header(AUTHORIZATION, format!("Bearer {}", token.access_token))
            .header(CONTENT_TYPE, "application/json; charset=UTF-8")
            .json(&body)
            .send()
            .await
            .map_err(|e| CoreError::Upload {
                platform: Platform::TikTok,
                reason: format!("Upload init failed: {e}"),
            })?;

        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();

        if !status.is_success() {
            return Err(CoreError::Upload {
                platform: Platform::TikTok,
                reason: format!("Init returned {status}: {body}"),
            });
        }

        let json: serde_json::Value = serde_json::from_str(&body).unwrap_or_default();
        json["data"]["upload_url"]
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| CoreError::Upload {
                platform: Platform::TikTok,
                reason: "No upload_url in response".into(),
            })
    }
}

#[async_trait]
impl AsyncUploader for TikTokUploader {
    fn platform(&self) -> Platform {
        Platform::TikTok
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
                platform: Platform::TikTok,
                reason: format!("Cannot open video: {e}"),
            })?;

        let total_bytes = file.metadata().await.map(|m| m.len()).unwrap_or(0);

        if total_bytes == 0 {
            return Err(CoreError::Upload {
                platform: Platform::TikTok,
                reason: "Video file is empty".into(),
            });
        }

        let upload_url = self.init_upload(&token, total_bytes).await?;

        let mut buffer = Vec::with_capacity(total_bytes as usize);
        file.read_to_end(&mut buffer).await.map_err(|e| CoreError::Upload {
            platform: Platform::TikTok,
            reason: format!("Cannot read video: {e}"),
        })?;

        on_progress(UploadProgress {
            bytes_sent: 0,
            total_bytes,
        });

        let content_range = format!("bytes 0-{}/{total_bytes}", total_bytes - 1);

        let resp = self.client
            .put(&upload_url)
            .header(CONTENT_TYPE, "video/mp4")
            .header(CONTENT_LENGTH, total_bytes.to_string())
            .header(CONTENT_RANGE, &content_range)
            .body(buffer)
            .send()
            .await
            .map_err(|e| CoreError::Upload {
                platform: Platform::TikTok,
                reason: format!("Upload failed: {e}"),
            })?;

        on_progress(UploadProgress {
            bytes_sent: total_bytes,
            total_bytes,
        });

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Ok(UploadResult::failed(
                Platform::TikTok,
                format!("HTTP {status}: {body}"),
            ));
        }

        Ok(UploadResult::success(
            Platform::TikTok,
            "https://www.tiktok.com (video pending review)",
        ))
    }

    async fn is_authenticated(&self) -> bool {
        self.authenticated.load(Ordering::SeqCst)
    }
}
