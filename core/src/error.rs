use thiserror::Error;

use crate::domain::model::Platform;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("Config error: {0}")]
    Config(String),

    #[error("Validation error for {platform}: {reason}")]
    Validation { platform: Platform, reason: String },

    #[error("Upload failed for {platform}: {reason}")]
    Upload { platform: Platform, reason: String },

    #[error("Authentication error for {platform}: {reason}")]
    Auth { platform: Platform, reason: String },

    #[error("File not found: {0}")]
    FileNotFound(String),

    #[error("Unsupported format: {0}")]
    UnsupportedFormat(String),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Serialization(#[from] toml::ser::Error),

    #[error(transparent)]
    Deserialization(#[from] toml::de::Error),
}

pub type Result<T> = std::result::Result<T, CoreError>;
