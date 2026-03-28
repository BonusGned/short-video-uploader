use std::sync::Arc;

use tokio::task::JoinSet;

use crate::domain::model::{Platform, UploadResult, VideoMetadata};
use crate::domain::port::{AsyncUploader, ProgressCallback, UploadProgress};
use crate::validation::VideoValidator;
use crate::error::Result;

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
                Err(e) => results.push(UploadResult::failed(
                    Platform::YouTube, // fallback; JoinError doesn't carry platform info
                    format!("Task panicked: {e}"),
                )),
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

        let mut results = Vec::new();
        while let Some(res) = join_set.join_next().await {
            if let Ok(pair) = res {
                results.push(pair);
            }
        }

        results
    }
}
