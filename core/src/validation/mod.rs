use std::fs;
use std::path::Path;

use crate::domain::model::{Platform, PlatformConstraints, VideoMetadata};
use crate::error::{CoreError, Result};

pub struct VideoValidator;

impl VideoValidator {
    pub fn validate(metadata: &VideoMetadata, platforms: &[Platform]) -> Vec<ValidationReport> {
        platforms
            .iter()
            .map(|&platform| {
                let constraints = PlatformConstraints::for_platform(platform);
                let errors = Self::check_constraints(metadata, platform, &constraints);
                ValidationReport { platform, errors }
            })
            .collect()
    }

    pub fn validate_or_fail(metadata: &VideoMetadata, platforms: &[Platform]) -> Result<()> {
        let reports = Self::validate(metadata, platforms);
        for report in &reports {
            if !report.errors.is_empty() {
                return Err(CoreError::Validation {
                    platform: report.platform,
                    reason: report.errors.join("; "),
                });
            }
        }
        Ok(())
    }

    fn check_constraints(
        metadata: &VideoMetadata,
        platform: Platform,
        constraints: &PlatformConstraints,
    ) -> Vec<String> {
        let mut errors = Vec::new();

        if !metadata.video_path.exists() {
            errors.push(format!("File not found: {}", metadata.video_path.display()));
            return errors;
        }

        if let Err(e) = Self::check_format(&metadata.video_path, platform, constraints) {
            errors.push(e);
        }

        if let Err(e) = Self::check_file_size(&metadata.video_path, platform, constraints) {
            errors.push(e);
        }

        if let Some(ref thumb) = metadata.thumbnail_path
            && !thumb.exists()
        {
            errors.push(format!("Thumbnail not found: {}", thumb.display()));
        }

        errors
    }

    fn check_format(
        path: &Path,
        platform: Platform,
        constraints: &PlatformConstraints,
    ) -> std::result::Result<(), String> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase())
            .unwrap_or_default();

        if ext.is_empty() {
            return Err(format!("[{platform}] File has no extension"));
        }

        if !constraints.supported_formats.contains(&ext.as_str()) {
            return Err(format!(
                "[{platform}] Unsupported format '.{ext}'. Supported: {}",
                constraints.supported_formats.join(", ")
            ));
        }

        Ok(())
    }

    fn check_file_size(
        path: &Path,
        platform: Platform,
        constraints: &PlatformConstraints,
    ) -> std::result::Result<(), String> {
        let file_size = fs::metadata(path).map(|m| m.len()).unwrap_or(0);

        let max_bytes = constraints.max_file_size_mb * 1024 * 1024;
        if file_size > max_bytes {
            let size_mb = file_size / (1024 * 1024);
            return Err(format!(
                "[{platform}] File too large: {size_mb}MB (max: {}MB)",
                constraints.max_file_size_mb
            ));
        }

        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct ValidationReport {
    pub platform: Platform,
    pub errors: Vec<String>,
}

impl ValidationReport {
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::path::PathBuf;
    use tempfile::NamedTempFile;

    fn temp_video(ext: &str) -> NamedTempFile {
        let mut f = tempfile::Builder::new()
            .suffix(&format!(".{ext}"))
            .tempfile()
            .unwrap();
        f.write_all(&[0u8; 1024]).unwrap();
        f
    }

    #[test]
    fn valid_mp4_passes_all_platforms() {
        let f = temp_video("mp4");
        let meta = VideoMetadata::new("Test", f.path().to_path_buf());
        let reports = VideoValidator::validate(&meta, &Platform::ALL);
        for r in &reports {
            assert!(r.is_valid(), "{}: {:?}", r.platform, r.errors);
        }
    }

    #[test]
    fn missing_file_reports_error() {
        let meta = VideoMetadata::new("Test", PathBuf::from("/nonexistent/video.mp4"));
        let reports = VideoValidator::validate(&meta, &[Platform::YouTube]);
        assert!(!reports[0].is_valid());
        assert!(reports[0].errors[0].contains("File not found"));
    }

    #[test]
    fn unsupported_format_reports_error() {
        let f = temp_video("flv");
        let meta = VideoMetadata::new("Test", f.path().to_path_buf());
        let reports = VideoValidator::validate(&meta, &[Platform::YouTube]);
        assert!(!reports[0].is_valid());
        assert!(reports[0].errors[0].contains("Unsupported format"));
    }

    #[test]
    fn no_extension_reports_error() {
        let f = tempfile::Builder::new().suffix("").tempfile().unwrap();
        let meta = VideoMetadata::new("Test", f.path().to_path_buf());
        let reports = VideoValidator::validate(&meta, &[Platform::YouTube]);
        assert!(!reports[0].is_valid());
        assert!(reports[0].errors[0].contains("no extension"));
    }

    #[test]
    fn missing_thumbnail_reports_error() {
        let f = temp_video("mp4");
        let meta = VideoMetadata::new("Test", f.path().to_path_buf())
            .with_thumbnail(PathBuf::from("/nonexistent/thumb.jpg"));
        let reports = VideoValidator::validate(&meta, &[Platform::YouTube]);
        assert!(!reports[0].is_valid());
        assert!(reports[0].errors[0].contains("Thumbnail not found"));
    }

    #[test]
    fn existing_thumbnail_passes() {
        let f = temp_video("mp4");
        let thumb = temp_video("jpg");
        let meta = VideoMetadata::new("Test", f.path().to_path_buf())
            .with_thumbnail(thumb.path().to_path_buf());
        let reports = VideoValidator::validate(&meta, &[Platform::YouTube]);
        assert!(reports[0].is_valid());
    }

    #[test]
    fn validate_or_fail_returns_err_on_invalid() {
        let meta = VideoMetadata::new("Test", PathBuf::from("/nonexistent/video.mp4"));
        let result = VideoValidator::validate_or_fail(&meta, &[Platform::YouTube]);
        assert!(result.is_err());
    }

    #[test]
    fn validate_or_fail_returns_ok_on_valid() {
        let f = temp_video("mp4");
        let meta = VideoMetadata::new("Test", f.path().to_path_buf());
        let result = VideoValidator::validate_or_fail(&meta, &[Platform::YouTube]);
        assert!(result.is_ok());
    }

    #[test]
    fn validates_against_multiple_platforms() {
        let f = temp_video("avi");
        let meta = VideoMetadata::new("Test", f.path().to_path_buf());
        let reports = VideoValidator::validate(&meta, &[Platform::YouTube, Platform::VK]);
        // avi is unsupported on YouTube but supported on VK
        assert!(!reports[0].is_valid());
        assert!(reports[1].is_valid());
    }

    #[test]
    fn validation_report_is_valid_with_no_errors() {
        let r = ValidationReport {
            platform: Platform::YouTube,
            errors: vec![],
        };
        assert!(r.is_valid());
    }

    #[test]
    fn validation_report_is_invalid_with_errors() {
        let r = ValidationReport {
            platform: Platform::YouTube,
            errors: vec!["bad".into()],
        };
        assert!(!r.is_valid());
    }
}
