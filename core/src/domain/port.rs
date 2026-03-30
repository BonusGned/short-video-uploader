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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentage_at_50_percent() {
        let p = UploadProgress { bytes_sent: 50, total_bytes: 100 };
        assert!((p.percentage() - 50.0).abs() < f64::EPSILON);
    }

    #[test]
    fn percentage_at_100_percent() {
        let p = UploadProgress { bytes_sent: 100, total_bytes: 100 };
        assert!((p.percentage() - 100.0).abs() < f64::EPSILON);
    }

    #[test]
    fn percentage_at_zero_bytes_sent() {
        let p = UploadProgress { bytes_sent: 0, total_bytes: 100 };
        assert!((p.percentage()).abs() < f64::EPSILON);
    }

    #[test]
    fn percentage_with_zero_total_returns_zero() {
        let p = UploadProgress { bytes_sent: 0, total_bytes: 0 };
        assert!((p.percentage()).abs() < f64::EPSILON);
    }

    #[test]
    fn percentage_non_zero_sent_zero_total() {
        let p = UploadProgress { bytes_sent: 50, total_bytes: 0 };
        assert!((p.percentage()).abs() < f64::EPSILON);
    }
}
