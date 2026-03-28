pub mod manager;

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::domain::model::{Platform, ThemePreference};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub theme: ThemePreference,
    pub default_title: String,
    pub default_description: String,
    pub default_tags: Vec<String>,
    pub enabled_platforms: HashSet<Platform>,
    #[serde(default)]
    pub youtube: OAuthCredentials,
    #[serde(default)]
    pub tiktok: OAuthCredentials,
    #[serde(default)]
    pub instagram: InstagramCredentials,
    #[serde(default)]
    pub vk: OAuthCredentials,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OAuthCredentials {
    pub client_id: String,
    pub client_secret: String,
}

impl OAuthCredentials {
    pub fn is_configured(&self) -> bool {
        !self.client_id.is_empty() && !self.client_secret.is_empty()
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InstagramCredentials {
    pub client_id: String,
    pub client_secret: String,
    pub ig_user_id: String,
}

impl InstagramCredentials {
    pub fn is_configured(&self) -> bool {
        !self.client_id.is_empty()
            && !self.client_secret.is_empty()
            && !self.ig_user_id.is_empty()
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            theme: ThemePreference::default(),
            default_title: String::new(),
            default_description: String::new(),
            default_tags: Vec::new(),
            enabled_platforms: Platform::ALL.into_iter().collect(),
            youtube: OAuthCredentials::default(),
            tiktok: OAuthCredentials::default(),
            instagram: InstagramCredentials::default(),
            vk: OAuthCredentials::default(),
        }
    }
}
