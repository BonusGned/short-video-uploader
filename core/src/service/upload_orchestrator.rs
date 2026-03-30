use std::sync::Arc;

use tokio::task::JoinSet;

use crate::domain::model::{Platform, UploadResult, VideoMetadata};
use crate::domain::port::{AsyncUploader, ProgressCallback, UploadProgress};
use crate::error::Result;
use crate::validation::VideoValidator;

pub struct UploadOrchestrator {
    uploaders: Vec<Arc<dyn AsyncUploader>>,
}

impl UploadOrchestrator {
    pub fn new(uploaders: Vec<Arc<dyn AsyncUploader>>) -> Self {
        Self { uploaders }
    }

    pub fn platforms(&self) -> Vec<Platform> {
        self.uploaders.iter().map(|u| u.platform()).collect()
    }

    pub async fn upload_all(
        &self,
        metadata: &VideoMetadata,
        on_progress: impl Fn(Platform, UploadProgress) + Send + Sync + 'static,
    ) -> Result<Vec<UploadResult>> {
        let platforms = self.platforms();
        VideoValidator::validate_or_fail(metadata, &platforms)?;

        let on_progress = Arc::new(on_progress);
        let mut join_set = JoinSet::new();

        for uploader in &self.uploaders {
            let uploader = Arc::clone(uploader);
            let metadata = metadata.clone();
            let on_progress = Arc::clone(&on_progress);

            join_set.spawn(async move {
                let platform = uploader.platform();
                let cb: ProgressCallback = Box::new(move |progress| {
                    on_progress(platform, progress);
                });

                match uploader.upload(&metadata, cb).await {
                    Ok(result) => result,
                    Err(e) => UploadResult::failed(platform, e.to_string()),
                }
            });
        }

        let mut results = Vec::with_capacity(self.uploaders.len());
        while let Some(res) = join_set.join_next().await {
            match res {
                Ok(upload_result) => results.push(upload_result),
                Err(e) => log::error!("Upload task panicked: {e}"),
            }
        }

        Ok(results)
    }

    pub async fn authenticate_all(&self) -> Vec<(Platform, Result<()>)> {
        let mut join_set = JoinSet::new();

        for uploader in &self.uploaders {
            let uploader = Arc::clone(uploader);
            join_set.spawn(async move {
                let platform = uploader.platform();
                let result = uploader.authenticate().await;
                (platform, result)
            });
        }

        let mut results = Vec::with_capacity(self.uploaders.len());
        while let Some(res) = join_set.join_next().await {
            match res {
                Ok(pair) => results.push(pair),
                Err(e) => log::error!("Auth task panicked: {e}"),
            }
        }

        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::mock_uploader::MockUploader;
    use std::io::Write;
    use std::sync::atomic::{AtomicU32, Ordering};

    fn mock_uploaders() -> Vec<Arc<dyn AsyncUploader>> {
        MockUploader::all_platforms()
            .into_iter()
            .map(|m| m.with_delay(50))
            .map(|m| Arc::new(m) as Arc<dyn AsyncUploader>)
            .collect()
    }

    fn temp_video() -> tempfile::NamedTempFile {
        let mut f = tempfile::Builder::new().suffix(".mp4").tempfile().unwrap();
        f.write_all(&[0u8; 2048]).unwrap();
        f
    }

    #[test]
    fn platforms_returns_all_mock_platforms() {
        let orch = UploadOrchestrator::new(mock_uploaders());
        let platforms = orch.platforms();
        assert_eq!(platforms.len(), 4);
    }

    #[test]
    fn empty_orchestrator_has_no_platforms() {
        let orch = UploadOrchestrator::new(vec![]);
        assert!(orch.platforms().is_empty());
    }

    #[tokio::test]
    async fn upload_all_succeeds_with_valid_file() {
        let f = temp_video();
        let meta = VideoMetadata::new("Integration", f.path().to_path_buf());
        let orch = UploadOrchestrator::new(mock_uploaders());

        let progress_calls = Arc::new(AtomicU32::new(0));
        let pc = Arc::clone(&progress_calls);

        let results = orch
            .upload_all(&meta, move |_, _| {
                pc.fetch_add(1, Ordering::Relaxed);
            })
            .await
            .unwrap();

        assert_eq!(results.len(), 4);
        for r in &results {
            assert!(r.is_success(), "Failed: {:?}", r);
        }
        assert!(progress_calls.load(Ordering::Relaxed) > 0);
    }

    #[tokio::test]
    async fn upload_all_fails_validation_for_missing_file() {
        let meta = VideoMetadata::new("Test", std::path::PathBuf::from("/nonexistent.mp4"));
        let orch = UploadOrchestrator::new(mock_uploaders());
        let result = orch.upload_all(&meta, |_, _| {}).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn authenticate_all_succeeds() {
        let orch = UploadOrchestrator::new(mock_uploaders());
        let results = orch.authenticate_all().await;
        assert_eq!(results.len(), 4);
        for (platform, result) in &results {
            assert!(result.is_ok(), "Auth failed for {platform}");
        }
    }

    #[tokio::test]
    async fn single_uploader_orchestrator() {
        let f = temp_video();
        let meta = VideoMetadata::new("Single", f.path().to_path_buf());
        let uploaders: Vec<Arc<dyn AsyncUploader>> = vec![Arc::new(
            MockUploader::new(Platform::YouTube).with_delay(50),
        )];
        let orch = UploadOrchestrator::new(uploaders);
        let results = orch.upload_all(&meta, |_, _| {}).await.unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].is_success());
    }
}
