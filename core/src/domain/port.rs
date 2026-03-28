use async_trait::async_trait;

use crate::domain::model::{Platform, UploadResult, VideoMetadata};
use crate::error::Result;

#[async_trait]
pub trait AsyncUploader: Send + Sync {
    fn platform(&self) -> Platform;

    async fn authenticate(&self) -> Result<()>;

    async fn upload(&self, metadata: &VideoMetadata, on_progress: ProgressCallback) -> Result<UploadResult>;

    async fn is_authenticated(&self) -> bool;
}

pub type ProgressCallback = Box<dyn Fn(UploadProgress) + Send + Sync>;

#[derive(Debug, Clone, Copy)]
pub struct UploadProgress {
    pub bytes_sent: u64,
    pub total_bytes: u64,
}

impl UploadProgress {
    pub fn percentage(&self) -> f64 {
        if self.total_bytes == 0 {
            return 0.0;
        }
        (self.bytes_sent as f64 / self.total_bytes as f64) * 100.0
    }
}
