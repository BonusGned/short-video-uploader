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
        !self.client_id.is_empty() && !self.client_secret.is_empty() && !self.ig_user_id.is_empty()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oauth_credentials_empty_is_not_configured() {
        let creds = OAuthCredentials::default();
        assert!(!creds.is_configured());
    }

    #[test]
    fn oauth_credentials_partial_is_not_configured() {
        let creds = OAuthCredentials {
            client_id: "id".into(),
            client_secret: String::new(),
        };
        assert!(!creds.is_configured());
    }

    #[test]
    fn oauth_credentials_full_is_configured() {
        let creds = OAuthCredentials {
            client_id: "id".into(),
            client_secret: "secret".into(),
        };
        assert!(creds.is_configured());
    }

    #[test]
    fn instagram_credentials_empty_is_not_configured() {
        let creds = InstagramCredentials::default();
        assert!(!creds.is_configured());
    }

    #[test]
    fn instagram_credentials_missing_user_id_not_configured() {
        let creds = InstagramCredentials {
            client_id: "id".into(),
            client_secret: "secret".into(),
            ig_user_id: String::new(),
        };
        assert!(!creds.is_configured());
    }

    #[test]
    fn instagram_credentials_full_is_configured() {
        let creds = InstagramCredentials {
            client_id: "id".into(),
            client_secret: "secret".into(),
            ig_user_id: "12345".into(),
        };
        assert!(creds.is_configured());
    }

    #[test]
    fn app_config_default_has_all_platforms_enabled() {
        let config = AppConfig::default();
        for p in Platform::ALL {
            assert!(config.enabled_platforms.contains(&p), "{p} not in defaults");
        }
    }

    #[test]
    fn app_config_default_theme_is_system() {
        let config = AppConfig::default();
        assert_eq!(config.theme, ThemePreference::System);
    }

    #[test]
    fn app_config_serde_roundtrip() {
        let config = AppConfig::default();
        let toml_str = toml::to_string_pretty(&config).unwrap();
        let restored: AppConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(restored.theme, config.theme);
        assert_eq!(restored.enabled_platforms, config.enabled_platforms);
    }
}
