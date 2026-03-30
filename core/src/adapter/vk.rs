use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use reqwest::multipart;
use tokio::io::AsyncReadExt;

use crate::adapter::oauth::{self, OAuthConfig, OAuthToken, ensure_valid_token, perform_full_auth};
use crate::domain::model::{Platform, UploadResult, VideoMetadata};
use crate::domain::port::{AsyncUploader, ProgressCallback, UploadProgress};
use crate::error::{CoreError, Result};

const VK_AUTH_URL: &str = "https://oauth.vk.com/authorize";
const VK_TOKEN_URL: &str = "https://oauth.vk.com/access_token";
const VK_API: &str = "https://api.vk.com/method";
const VK_API_VERSION: &str = "5.199";
const REDIRECT_PORT: u16 = 8588;

pub struct VKUploader {
    oauth_config: OAuthConfig,
    client: reqwest::Client,
    authenticated: AtomicBool,
}

impl VKUploader {
    pub fn new(client_id: String, client_secret: String) -> Self {
        let oauth_config = OAuthConfig {
            platform: Platform::VK,
            client_id,
            client_secret,
            auth_url: VK_AUTH_URL.into(),
            token_url: VK_TOKEN_URL.into(),
            redirect_port: REDIRECT_PORT,
            scopes: vec!["video".into(), "wall".into()],
            use_pkce: false,
            extra_auth_params: HashMap::from([("display".into(), "page".into())]),
        };

        let authenticated = oauth::load_token(Platform::VK)
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

    async fn get_upload_server(
        &self,
        token: &OAuthToken,
        metadata: &VideoMetadata,
    ) -> Result<String> {
        let params = [
            ("access_token", token.access_token.as_str()),
            ("v", VK_API_VERSION),
            ("name", &metadata.title),
            ("description", &metadata.description),
            ("is_private", "0"),
            ("wallpost", "1"),
        ];

        let url = format!("{VK_API}/video.save");
        let resp = self.client
            .post(&url)
            .form(&params)
            .send()
            .await
            .map_err(|e| CoreError::Upload {
                platform: Platform::VK,
                reason: format!("video.save failed: {e}"),
            })?;

        let body = resp.text().await.unwrap_or_default();
        let json: serde_json::Value = serde_json::from_str(&body).unwrap_or_default();

        if let Some(error) = json["error"]["error_msg"].as_str() {
            return Err(CoreError::Upload {
                platform: Platform::VK,
                reason: error.to_string(),
            });
        }

        json["response"]["upload_url"]
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| CoreError::Upload {
                platform: Platform::VK,
                reason: "No upload_url in video.save response".into(),
            })
    }
}

#[async_trait]
impl AsyncUploader for VKUploader {
    fn platform(&self) -> Platform {
        Platform::VK
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
        let upload_url = self.get_upload_server(&token, metadata).await?;

        let mut file = tokio::fs::File::open(&metadata.video_path)
            .await
            .map_err(|e| CoreError::Upload {
                platform: Platform::VK,
                reason: format!("Cannot open video: {e}"),
            })?;

        let total_bytes = file.metadata().await.map(|m| m.len()).unwrap_or(0);
        let mut buffer = Vec::with_capacity(total_bytes as usize);
        file.read_to_end(&mut buffer).await.map_err(|e| CoreError::Upload {
            platform: Platform::VK,
            reason: format!("Cannot read video: {e}"),
        })?;

        on_progress(UploadProgress {
            bytes_sent: 0,
            total_bytes,
        });

        let filename = metadata
            .video_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("video.mp4")
            .to_string();

        let part = multipart::Part::bytes(buffer).file_name(filename);
        let form = multipart::Form::new().part("video_file", part);

        let resp = self.client
            .post(&upload_url)
            .multipart(form)
            .send()
            .await
            .map_err(|e| CoreError::Upload {
                platform: Platform::VK,
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
                Platform::VK,
                format!("HTTP {status}: {body}"),
            ));
        }

        let json: serde_json::Value = serde_json::from_str(&body).unwrap_or_default();
        let owner_id = json["owner_id"].as_i64().unwrap_or(0);
        let video_id = json["video_id"].as_i64().unwrap_or(0);
        let url = format!("https://vk.com/clip{owner_id}_{video_id}");

        Ok(UploadResult::success(Platform::VK, url))
    }

    async fn is_authenticated(&self) -> bool {
        self.authenticated.load(Ordering::SeqCst)
    }
}
