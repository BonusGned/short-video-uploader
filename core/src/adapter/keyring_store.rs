use crate::domain::model::Platform;
use crate::error::{CoreError, Result};

const SERVICE_NAME: &str = "crosspost-rust";

pub struct KeyringStore;

impl KeyringStore {
    pub fn store_token(platform: Platform, token: &str) -> Result<()> {
        let entry = keyring::Entry::new(SERVICE_NAME, &platform.to_string())
            .map_err(|e| CoreError::Auth {
                platform,
                reason: e.to_string(),
            })?;

        entry.set_password(token).map_err(|e| CoreError::Auth {
            platform,
            reason: e.to_string(),
        })
    }

    pub fn get_token(platform: Platform) -> Result<Option<String>> {
        let entry = keyring::Entry::new(SERVICE_NAME, &platform.to_string())
            .map_err(|e| CoreError::Auth {
                platform,
                reason: e.to_string(),
            })?;

        match entry.get_password() {
            Ok(token) => Ok(Some(token)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(CoreError::Auth {
                platform,
                reason: e.to_string(),
            }),
        }
    }

    pub fn delete_token(platform: Platform) -> Result<()> {
        let entry = keyring::Entry::new(SERVICE_NAME, &platform.to_string())
            .map_err(|e| CoreError::Auth {
                platform,
                reason: e.to_string(),
            })?;

        match entry.delete_credential() {
            Ok(()) => Ok(()),
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(CoreError::Auth {
                platform,
                reason: e.to_string(),
            }),
        }
    }
}
