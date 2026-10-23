use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::config::write_atomic;
use crate::error::{AppError, AppResult};

const SERVICE: &str = "uni-translate";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyKind {
    Chat,
    Tts,
    BaiduApiKey,
    BaiduSecretKey,
}

impl KeyKind {
    fn account(self) -> &'static str {
        match self {
            KeyKind::Chat => "chat",
            KeyKind::Tts => "tts",
            KeyKind::BaiduApiKey => "baidu_api_key",
            KeyKind::BaiduSecretKey => "baidu_secret_key",
        }
    }
}

/// API keys live in the OS keychain. When no keychain is available (common on
/// minimal Linux desktops) they fall back to a user-only file in the config dir.
pub struct SecretStore {
    fallback_path: PathBuf,
    use_keyring: bool,
    /// Avoids hitting the keychain (and its permission prompts) on every request.
    cache: Mutex<HashMap<KeyKind, Option<String>>>,
}

impl SecretStore {
    pub fn new(fallback_path: PathBuf) -> Self {
        Self {
            fallback_path,
            use_keyring: true,
            cache: Mutex::default(),
        }
    }

    pub fn get(&self, kind: KeyKind) -> Option<String> {
        if let Some(cached) = self.cache.lock().unwrap().get(&kind) {
            return cached.clone();
        }
        let value = self
            .keyring_get(kind)
            .or_else(|| self.read_file().remove(kind.account()));
        self.cache.lock().unwrap().insert(kind, value.clone());
        value
    }

    /// Stores `key`, or deletes it when it is empty.
    pub fn set(&self, kind: KeyKind, key: &str) -> AppResult<()> {
        let key = key.trim();
        let mut file = self.read_file();
        let had_file_entry = file.remove(kind.account()).is_some();

        if key.is_empty() {
            self.keyring_delete(kind);
        } else if let Err(err) = self.keyring_set(kind, key) {
            log::warn!("keychain unavailable, storing API key in file: {err}");
            file.insert(kind.account().to_string(), key.to_string());
        }
        if had_file_entry || file.contains_key(kind.account()) {
            self.write_file(&file)?;
        }

        let value = (!key.is_empty()).then(|| key.to_string());
        self.cache.lock().unwrap().insert(kind, value);
        Ok(())
    }

    fn keyring_get(&self, kind: KeyKind) -> Option<String> {
        if !self.use_keyring {
            return None;
        }
        keyring::Entry::new(SERVICE, kind.account())
            .and_then(|entry| entry.get_password())
            .ok()
    }

    fn keyring_set(&self, kind: KeyKind, key: &str) -> AppResult<()> {
        if !self.use_keyring {
            return Err(AppError::Secret("keychain disabled".into()));
        }
        keyring::Entry::new(SERVICE, kind.account())
            .and_then(|entry| entry.set_password(key))
            .map_err(|err| AppError::Secret(err.to_string()))
    }

    fn keyring_delete(&self, kind: KeyKind) {
        if !self.use_keyring {
            return;
        }
        if let Ok(entry) = keyring::Entry::new(SERVICE, kind.account()) {
            let _ = entry.delete_credential();
        }
    }

    fn read_file(&self) -> HashMap<String, String> {
        fs::read_to_string(&self.fallback_path)
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default()
    }

    fn write_file(&self, map: &HashMap<String, String>) -> AppResult<()> {
        if map.is_empty() {
            let _ = fs::remove_file(&self.fallback_path);
            return Ok(());
        }
        write_atomic(&self.fallback_path, &serde_json::to_vec(map)?)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&self.fallback_path, fs::Permissions::from_mode(0o600))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file_only_store() -> (SecretStore, PathBuf) {
        let dir = std::env::temp_dir().join(format!("uni-translate-test-{}", uuid::Uuid::new_v4()));
        let store = SecretStore {
            fallback_path: dir.join("secrets.json"),
            use_keyring: false,
            cache: Mutex::default(),
        };
        (store, dir)
    }

    #[test]
    fn falls_back_to_file_and_deletes() {
        let (store, dir) = file_only_store();
        assert_eq!(store.get(KeyKind::Chat), None);

        store.set(KeyKind::Chat, " sk-123 ").unwrap();
        store.set(KeyKind::Tts, "sk-tts").unwrap();
        assert_eq!(store.get(KeyKind::Chat).as_deref(), Some("sk-123"));

        // A fresh store (no cache) reads the same values from disk.
        let reloaded = SecretStore {
            fallback_path: store.fallback_path.clone(),
            use_keyring: false,
            cache: Mutex::default(),
        };
        assert_eq!(reloaded.get(KeyKind::Tts).as_deref(), Some("sk-tts"));

        store.set(KeyKind::Chat, "").unwrap();
        store.set(KeyKind::Tts, "").unwrap();
        assert_eq!(store.get(KeyKind::Chat), None);
        assert!(!store.fallback_path.exists());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn key_kinds_serialize_for_the_frontend() {
        for (kind, name) in [
            (KeyKind::Chat, "chat"),
            (KeyKind::Tts, "tts"),
            (KeyKind::BaiduApiKey, "baidu_api_key"),
            (KeyKind::BaiduSecretKey, "baidu_secret_key"),
        ] {
            assert_eq!(serde_json::to_string(&kind).unwrap(), format!("\"{name}\""));
            assert_eq!(kind.account(), name);
        }
    }
}
