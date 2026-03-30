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
        Platform::ALL.iter().map(|&p| Self::new(p)).collect()
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
        let total_bytes = tokio::fs::metadata(&metadata.video_path)
            .await
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::model::UploadStatus;
    use std::io::Write;
    use std::sync::Arc;
    use std::sync::atomic::AtomicU64;

    fn temp_video() -> tempfile::NamedTempFile {
        let mut f = tempfile::Builder::new().suffix(".mp4").tempfile().unwrap();
        f.write_all(&[0u8; 4096]).unwrap();
        f
    }

    #[test]
    fn returns_correct_platform() {
        let m = MockUploader::new(Platform::YouTube);
        assert_eq!(m.platform(), Platform::YouTube);
    }

    #[test]
    fn all_platforms_returns_four() {
        let mocks = MockUploader::all_platforms();
        assert_eq!(mocks.len(), 4);
    }

    #[test]
    fn with_delay_sets_delay() {
        let m = MockUploader::new(Platform::VK).with_delay(100);
        assert_eq!(m.simulated_delay_ms, 100);
    }

    #[tokio::test]
    async fn authenticate_sets_flag() {
        let m = MockUploader::new(Platform::TikTok);
        assert!(!m.is_authenticated().await);
        m.authenticate().await.unwrap();
        assert!(m.is_authenticated().await);
    }

    #[tokio::test]
    async fn upload_returns_success_with_url() {
        let f = temp_video();
        let meta = VideoMetadata::new("My Video", f.path().to_path_buf());
        let progress_count = Arc::new(AtomicU64::new(0));
        let pc = Arc::clone(&progress_count);

        let m = MockUploader::new(Platform::YouTube).with_delay(50);
        let result = m
            .upload(
                &meta,
                Box::new(move |_| {
                    pc.fetch_add(1, Ordering::Relaxed);
                }),
            )
            .await
            .unwrap();

        assert!(result.is_success());
        assert!(progress_count.load(Ordering::Relaxed) > 0);
        if let UploadStatus::Success { url } = &result.status {
            assert!(url.contains("youtube.mock"));
            assert!(url.contains("my-video"));
        }
    }
}
