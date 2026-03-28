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
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            theme: ThemePreference::default(),
            default_title: String::new(),
            default_description: String::new(),
            default_tags: Vec::new(),
            enabled_platforms: Platform::ALL.into_iter().collect(),
        }
    }
}
