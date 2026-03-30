use std::fmt;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    YouTube,
    Instagram,
    TikTok,
    VK,
}

impl fmt::Display for Platform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::YouTube => write!(f, "YouTube"),
            Self::Instagram => write!(f, "Instagram"),
            Self::TikTok => write!(f, "TikTok"),
            Self::VK => write!(f, "VK"),
        }
    }
}

impl Platform {
    pub const ALL: [Platform; 4] = [
        Platform::YouTube,
        Platform::Instagram,
        Platform::TikTok,
        Platform::VK,
    ];
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoMetadata {
    pub title: String,
    pub description: String,
    pub tags: Vec<String>,
    pub video_path: PathBuf,
    pub thumbnail_path: Option<PathBuf>,
}

impl VideoMetadata {
    pub fn new(title: impl Into<String>, video_path: PathBuf) -> Self {
        Self {
            title: title.into(),
            description: String::new(),
            tags: Vec::new(),
            video_path,
            thumbnail_path: None,
        }
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    pub fn with_thumbnail(mut self, path: PathBuf) -> Self {
        self.thumbnail_path = Some(path);
        self
    }
}

#[derive(Debug, Clone)]
pub struct UploadResult {
    pub platform: Platform,
    pub status: UploadStatus,
}

#[derive(Debug, Clone)]
pub enum UploadStatus {
    Success { url: String },
    Failed { reason: String },
}

impl UploadResult {
    pub fn success(platform: Platform, url: impl Into<String>) -> Self {
        Self {
            platform,
            status: UploadStatus::Success { url: url.into() },
        }
    }

    pub fn failed(platform: Platform, reason: impl Into<String>) -> Self {
        Self {
            platform,
            status: UploadStatus::Failed {
                reason: reason.into(),
            },
        }
    }

    pub fn is_success(&self) -> bool {
        matches!(self.status, UploadStatus::Success { .. })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ThemePreference {
    Light,
    Dark,
    #[default]
    System,
}

impl fmt::Display for ThemePreference {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Light => write!(f, "Light"),
            Self::Dark => write!(f, "Dark"),
            Self::System => write!(f, "System"),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PlatformConstraints {
    pub max_file_size_mb: u64,
    pub max_duration_secs: u64,
    pub min_aspect_ratio: f64,
    pub max_aspect_ratio: f64,
    pub supported_formats: &'static [&'static str],
}

impl PlatformConstraints {
    pub fn for_platform(platform: Platform) -> Self {
        match platform {
            Platform::YouTube => Self {
                max_file_size_mb: 256,
                max_duration_secs: 60,
                min_aspect_ratio: 0.5,
                max_aspect_ratio: 1.0,
                supported_formats: &["mp4", "webm", "mov"],
            },
            Platform::Instagram => Self {
                max_file_size_mb: 250,
                max_duration_secs: 90,
                min_aspect_ratio: 0.5625,
                max_aspect_ratio: 1.0,
                supported_formats: &["mp4", "mov"],
            },
            Platform::TikTok => Self {
                max_file_size_mb: 287,
                max_duration_secs: 180,
                min_aspect_ratio: 0.5,
                max_aspect_ratio: 1.0,
                supported_formats: &["mp4", "mov", "webm"],
            },
            Platform::VK => Self {
                max_file_size_mb: 256,
                max_duration_secs: 60,
                min_aspect_ratio: 0.5,
                max_aspect_ratio: 1.0,
                supported_formats: &["mp4", "avi", "mov"],
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_display() {
        assert_eq!(Platform::YouTube.to_string(), "YouTube");
        assert_eq!(Platform::Instagram.to_string(), "Instagram");
        assert_eq!(Platform::TikTok.to_string(), "TikTok");
        assert_eq!(Platform::VK.to_string(), "VK");
    }

    #[test]
    fn platform_all_has_four_variants() {
        assert_eq!(Platform::ALL.len(), 4);
        let set: std::collections::HashSet<Platform> = Platform::ALL.into_iter().collect();
        assert!(set.contains(&Platform::YouTube));
        assert!(set.contains(&Platform::Instagram));
        assert!(set.contains(&Platform::TikTok));
        assert!(set.contains(&Platform::VK));
    }

    #[test]
    fn platform_serde_roundtrip() {
        for p in Platform::ALL {
            let json = serde_json::to_string(&p).unwrap();
            let deserialized: Platform = serde_json::from_str(&json).unwrap();
            assert_eq!(p, deserialized);
        }
    }

    #[test]
    fn video_metadata_new_defaults() {
        let meta = VideoMetadata::new("Title", PathBuf::from("video.mp4"));
        assert_eq!(meta.title, "Title");
        assert!(meta.description.is_empty());
        assert!(meta.tags.is_empty());
        assert_eq!(meta.video_path, PathBuf::from("video.mp4"));
        assert!(meta.thumbnail_path.is_none());
    }

    #[test]
    fn video_metadata_builder_chain() {
        let meta = VideoMetadata::new("Test", PathBuf::from("v.mp4"))
            .with_description("Desc")
            .with_tags(vec!["a".into(), "b".into()])
            .with_thumbnail(PathBuf::from("thumb.jpg"));

        assert_eq!(meta.description, "Desc");
        assert_eq!(meta.tags, vec!["a", "b"]);
        assert_eq!(meta.thumbnail_path, Some(PathBuf::from("thumb.jpg")));
    }

    #[test]
    fn upload_result_success_variant() {
        let r = UploadResult::success(Platform::YouTube, "https://yt.com/123");
        assert!(r.is_success());
        assert!(matches!(r.status, UploadStatus::Success { ref url } if url == "https://yt.com/123"));
    }

    #[test]
    fn upload_result_failed_variant() {
        let r = UploadResult::failed(Platform::TikTok, "timeout");
        assert!(!r.is_success());
        assert!(matches!(r.status, UploadStatus::Failed { ref reason } if reason == "timeout"));
    }

    #[test]
    fn theme_preference_default_is_system() {
        assert_eq!(ThemePreference::default(), ThemePreference::System);
    }

    #[test]
    fn theme_preference_display() {
        assert_eq!(ThemePreference::Light.to_string(), "Light");
        assert_eq!(ThemePreference::Dark.to_string(), "Dark");
        assert_eq!(ThemePreference::System.to_string(), "System");
    }

    #[test]
    fn platform_constraints_all_have_supported_formats() {
        for p in Platform::ALL {
            let c = PlatformConstraints::for_platform(p);
            assert!(!c.supported_formats.is_empty(), "{p} has no supported formats");
            assert!(c.max_file_size_mb > 0);
            assert!(c.max_duration_secs > 0);
            assert!(c.min_aspect_ratio < c.max_aspect_ratio);
        }
    }

    #[test]
    fn platform_constraints_youtube_supports_mp4() {
        let c = PlatformConstraints::for_platform(Platform::YouTube);
        assert!(c.supported_formats.contains(&"mp4"));
    }
}
