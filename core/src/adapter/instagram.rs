use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use async_trait::async_trait;

use crate::adapter::oauth::{self, OAuthConfig, OAuthToken, ensure_valid_token, perform_full_auth};
use crate::domain::model::{Platform, UploadResult, VideoMetadata};
use crate::domain::port::{AsyncUploader, ProgressCallback, UploadProgress};
use crate::error::{CoreError, Result};

const FB_AUTH_URL: &str = "https://www.facebook.com/v21.0/dialog/oauth";
const FB_TOKEN_URL: &str = "https://graph.facebook.com/v21.0/oauth/access_token";
const GRAPH_API: &str = "https://graph.facebook.com/v21.0";
const REDIRECT_PORT: u16 = 8587;

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

    async fn get_token(&self) -> Result<OAuthToken> {
        ensure_valid_token(&self.oauth_config).await
    }

    async fn create_container(
        &self,
        token: &OAuthToken,
        metadata: &VideoMetadata,
    ) -> Result<String> {
        let video_url = format!("file://{}", metadata.video_path.display());

        let mut params = HashMap::new();
        params.insert("media_type", "REELS".to_string());
        params.insert("video_url", video_url);
        params.insert("caption", metadata.description.clone());
        params.insert("access_token", token.access_token.clone());

        let url = format!("{GRAPH_API}/{}/media", self.ig_user_id);
        let resp = self.client
            .post(&url)
            .form(&params)
            .send()
            .await
            .map_err(|e| CoreError::Upload {
                platform: Platform::Instagram,
                reason: format!("Container creation failed: {e}"),
            })?;

        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();

        if !status.is_success() {
            return Err(CoreError::Upload {
                platform: Platform::Instagram,
                reason: format!("Container creation returned {status}: {body}"),
            });
        }

        let json: serde_json::Value = serde_json::from_str(&body).unwrap_or_default();
        json["id"]
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| CoreError::Upload {
                platform: Platform::Instagram,
                reason: "No container ID in response".into(),
            })
    }

    async fn wait_for_container(&self, token: &OAuthToken, container_id: &str) -> Result<()> {
        let url = format!(
            "{GRAPH_API}/{container_id}?fields=status_code&access_token={}",
            token.access_token
        );

        for _ in 0..30 {
            tokio::time::sleep(Duration::from_secs(2)).await;

            let resp = self.client.get(&url).send().await.map_err(|e| CoreError::Upload {
                platform: Platform::Instagram,
                reason: format!("Status check failed: {e}"),
            })?;

            let body = resp.text().await.unwrap_or_default();
            let json: serde_json::Value = serde_json::from_str(&body).unwrap_or_default();

            match json["status_code"].as_str() {
                Some("FINISHED") => return Ok(()),
                Some("ERROR") => {
                    return Err(CoreError::Upload {
                        platform: Platform::Instagram,
                        reason: "Container processing failed".into(),
                    });
                }
                _ => continue,
            }
        }

        Err(CoreError::Upload {
            platform: Platform::Instagram,
            reason: "Container processing timed out".into(),
        })
    }

    async fn publish_container(
        &self,
        token: &OAuthToken,
        container_id: &str,
    ) -> Result<String> {
        let url = format!("{GRAPH_API}/{}/media_publish", self.ig_user_id);

        let params = [
            ("creation_id", container_id),
            ("access_token", &token.access_token),
        ];

        let resp = self.client
            .post(&url)
            .form(&params)
            .send()
            .await
            .map_err(|e| CoreError::Upload {
                platform: Platform::Instagram,
                reason: format!("Publish failed: {e}"),
            })?;

        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();

        if !status.is_success() {
            return Err(CoreError::Upload {
                platform: Platform::Instagram,
                reason: format!("Publish returned {status}: {body}"),
            });
        }

        let json: serde_json::Value = serde_json::from_str(&body).unwrap_or_default();
        let media_id = json["id"].as_str().unwrap_or("unknown");
        Ok(format!("https://www.instagram.com/reel/{media_id}/"))
    }
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
        let token = self.get_token().await?;
        let total_bytes = tokio::fs::metadata(&metadata.video_path)
            .await
            .map(|m| m.len())
            .unwrap_or(0);

        on_progress(UploadProgress {
            bytes_sent: 0,
            total_bytes,
        });

        let container_id = self.create_container(&token, metadata).await?;

        on_progress(UploadProgress {
            bytes_sent: total_bytes / 3,
            total_bytes,
        });

        self.wait_for_container(&token, &container_id).await?;

        on_progress(UploadProgress {
            bytes_sent: total_bytes * 2 / 3,
            total_bytes,
        });

        let url = self.publish_container(&token, &container_id).await?;

        on_progress(UploadProgress {
            bytes_sent: total_bytes,
            total_bytes,
        });

        Ok(UploadResult::success(Platform::Instagram, url))
    }

    async fn is_authenticated(&self) -> bool {
        self.authenticated.load(Ordering::SeqCst)
    }
}
