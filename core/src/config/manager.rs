use std::fs;
use std::path::{Path, PathBuf};

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

        Self::from_dir(project_dirs.config_dir().to_path_buf())
    }

    pub fn from_dir(config_dir: PathBuf) -> Result<Self> {
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

    pub fn config_dir(&self) -> &Path {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::model::ThemePreference;

    #[test]
    fn creates_default_config_in_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let mgr = ConfigManager::from_dir(dir.path().to_path_buf()).unwrap();

        assert_eq!(mgr.config().theme, ThemePreference::System);
        assert!(mgr.config_file_path().exists());
    }

    #[test]
    fn loads_existing_config() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.toml");
        let mut config = AppConfig::default();
        config.theme = ThemePreference::Dark;
        let content = toml::to_string_pretty(&config).unwrap();
        fs::write(&config_path, content).unwrap();

        let mgr = ConfigManager::from_dir(dir.path().to_path_buf()).unwrap();
        assert_eq!(mgr.config().theme, ThemePreference::Dark);
    }

    #[test]
    fn save_persists_changes() {
        let dir = tempfile::tempdir().unwrap();
        let mut mgr = ConfigManager::from_dir(dir.path().to_path_buf()).unwrap();
        mgr.config_mut().theme = ThemePreference::Light;
        mgr.save().unwrap();

        let mgr2 = ConfigManager::from_dir(dir.path().to_path_buf()).unwrap();
        assert_eq!(mgr2.config().theme, ThemePreference::Light);
    }

    #[test]
    fn update_applies_closure_and_saves() {
        let dir = tempfile::tempdir().unwrap();
        let mut mgr = ConfigManager::from_dir(dir.path().to_path_buf()).unwrap();
        mgr.update(|c| c.default_title = "My Title".into()).unwrap();

        let mgr2 = ConfigManager::from_dir(dir.path().to_path_buf()).unwrap();
        assert_eq!(mgr2.config().default_title, "My Title");
    }

    #[test]
    fn config_dir_returns_correct_path() {
        let dir = tempfile::tempdir().unwrap();
        let mgr = ConfigManager::from_dir(dir.path().to_path_buf()).unwrap();
        assert_eq!(mgr.config_dir(), dir.path());
    }
}
