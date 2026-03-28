pub mod instagram;
pub mod keyring_store;
pub mod mock_uploader;
pub mod oauth;
pub mod tiktok;
pub mod vk;
pub mod youtube;

use std::sync::Arc;

use crate::config::AppConfig;
use crate::domain::model::Platform;
use crate::domain::port::AsyncUploader;

pub fn create_uploaders(config: &AppConfig) -> Vec<Arc<dyn AsyncUploader>> {
    let mut uploaders: Vec<Arc<dyn AsyncUploader>> = Vec::new();

    for &platform in &config.enabled_platforms {
        if let Some(uploader) = create_uploader(platform, config) {
            uploaders.push(uploader);
        }
    }

    uploaders
}

fn create_uploader(platform: Platform, config: &AppConfig) -> Option<Arc<dyn AsyncUploader>> {
    match platform {
        Platform::YouTube if config.youtube.is_configured() => Some(Arc::new(
            youtube::YouTubeUploader::new(
                config.youtube.client_id.clone(),
                config.youtube.client_secret.clone(),
            ),
        )),
        Platform::TikTok if config.tiktok.is_configured() => Some(Arc::new(
            tiktok::TikTokUploader::new(
                config.tiktok.client_id.clone(),
                config.tiktok.client_secret.clone(),
            ),
        )),
        Platform::Instagram if config.instagram.is_configured() => Some(Arc::new(
            instagram::InstagramUploader::new(
                config.instagram.client_id.clone(),
                config.instagram.client_secret.clone(),
                config.instagram.ig_user_id.clone(),
            ),
        )),
        Platform::VK if config.vk.is_configured() => Some(Arc::new(
            vk::VKUploader::new(
                config.vk.client_id.clone(),
                config.vk.client_secret.clone(),
            ),
        )),
        _ => {
            log::warn!("{platform} credentials not configured, using mock uploader");
            Some(Arc::new(mock_uploader::MockUploader::new(platform)))
        }
    }
}
