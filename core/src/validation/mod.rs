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
            if let Some(err) = report.errors.first() {
                return Err(CoreError::Validation {
                    platform: report.platform,
                    reason: err.clone(),
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

        if !constraints.supported_formats.contains(&ext) {
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
        let file_size = fs::metadata(path)
            .map(|m| m.len())
            .unwrap_or(0);

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
