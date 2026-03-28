use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use async_trait::async_trait;

use crate::domain::model::{Platform, UploadResult, VideoMetadata};
use crate::domain::port::{AsyncUploader, ProgressCallback, UploadProgress};
use crate::error::Result;

pub struct MockUploader {
    platform: Platform,
    authenticated: AtomicBool,
    simulated_delay_ms: u64,
}

impl MockUploader {
    pub fn new(platform: Platform) -> Self {
        Self {
            platform,
            authenticated: AtomicBool::new(false),
            simulated_delay_ms: 500,
        }
    }

    pub fn with_delay(mut self, delay_ms: u64) -> Self {
        self.simulated_delay_ms = delay_ms;
        self
    }

    pub fn all_platforms() -> Vec<Self> {
        Platform::ALL
            .iter()
            .map(|&p| Self::new(p))
            .collect()
    }
}

#[async_trait]
impl AsyncUploader for MockUploader {
    fn platform(&self) -> Platform {
        self.platform
    }

    async fn authenticate(&self) -> Result<()> {
        tokio::time::sleep(Duration::from_millis(100)).await;
        self.authenticated.store(true, Ordering::SeqCst);
        Ok(())
    }

    async fn upload(
        &self,
        metadata: &VideoMetadata,
        on_progress: ProgressCallback,
    ) -> Result<UploadResult> {
        let total_bytes = std::fs::metadata(&metadata.video_path)
            .map(|m| m.len())
            .unwrap_or(1_000_000);

        let steps = 10u64;
        let chunk = total_bytes / steps;

        for i in 1..=steps {
            tokio::time::sleep(Duration::from_millis(self.simulated_delay_ms / steps)).await;
            on_progress(UploadProgress {
                bytes_sent: chunk * i,
                total_bytes,
            });
        }

        let url = format!(
            "https://{}.mock/shorts/{}",
            self.platform.to_string().to_lowercase(),
            &metadata.title.replace(' ', "-").to_lowercase()
        );

        Ok(UploadResult::success(self.platform, url))
    }

    async fn is_authenticated(&self) -> bool {
        self.authenticated.load(Ordering::SeqCst)
    }
}
