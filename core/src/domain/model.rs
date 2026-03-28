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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformConstraints {
    pub max_file_size_mb: u64,
    pub max_duration_secs: u64,
    pub min_aspect_ratio: f64,
    pub max_aspect_ratio: f64,
    pub supported_formats: Vec<String>,
}

impl PlatformConstraints {
    pub fn for_platform(platform: Platform) -> Self {
        match platform {
            Platform::YouTube => Self {
                max_file_size_mb: 256,
                max_duration_secs: 60,
                min_aspect_ratio: 0.5,
                max_aspect_ratio: 1.0,
                supported_formats: vec!["mp4".into(), "webm".into(), "mov".into()],
            },
            Platform::Instagram => Self {
                max_file_size_mb: 250,
                max_duration_secs: 90,
                min_aspect_ratio: 0.5625,
                max_aspect_ratio: 1.0,
                supported_formats: vec!["mp4".into(), "mov".into()],
            },
            Platform::TikTok => Self {
                max_file_size_mb: 287,
                max_duration_secs: 180,
                min_aspect_ratio: 0.5,
                max_aspect_ratio: 1.0,
                supported_formats: vec!["mp4".into(), "mov".into(), "webm".into()],
            },
            Platform::VK => Self {
                max_file_size_mb: 256,
                max_duration_secs: 60,
                min_aspect_ratio: 0.5,
                max_aspect_ratio: 1.0,
                supported_formats: vec!["mp4".into(), "avi".into(), "mov".into()],
            },
        }
    }
}
