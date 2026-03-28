use std::fs;
use std::path::PathBuf;

use directories::ProjectDirs;

use crate::config::AppConfig;
use crate::error::{CoreError, Result};

const CONFIG_FILE: &str = "config.toml";

pub struct ConfigManager {
    config_dir: PathBuf,
    config: AppConfig,
}

impl ConfigManager {
    pub fn new() -> Result<Self> {
        let project_dirs = ProjectDirs::from("com", "CrossPost", "CrossPost-Rust")
            .ok_or_else(|| CoreError::Config("Unable to determine config directory".into()))?;

        let config_dir = project_dirs.config_dir().to_path_buf();
        fs::create_dir_all(&config_dir)?;

        let config_path = config_dir.join(CONFIG_FILE);
        let config = if config_path.exists() {
            let content = fs::read_to_string(&config_path)?;
            toml::from_str(&content)?
        } else {
            let config = AppConfig::default();
            let content = toml::to_string_pretty(&config)?;
            fs::write(&config_path, content)?;
            config
        };

        Ok(Self { config_dir, config })
    }

    pub fn config(&self) -> &AppConfig {
        &self.config
    }

    pub fn config_mut(&mut self) -> &mut AppConfig {
        &mut self.config
    }

    pub fn save(&self) -> Result<()> {
        let config_path = self.config_dir.join(CONFIG_FILE);
        let content = toml::to_string_pretty(&self.config)?;
        fs::write(&config_path, content)?;
        Ok(())
    }

    pub fn config_dir(&self) -> &PathBuf {
        &self.config_dir
    }

    pub fn config_file_path(&self) -> PathBuf {
        self.config_dir.join(CONFIG_FILE)
    }

    pub fn update<F>(&mut self, f: F) -> Result<()>
    where
        F: FnOnce(&mut AppConfig),
    {
        f(&mut self.config);
        self.save()
    }
}
