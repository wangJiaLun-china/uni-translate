use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::config::write_atomic;
use crate::error::AppResult;

/// Which API key is being stored. The account name is also the JSON field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyKind {
    /// OpenAI-compatible translation API key.
    Chat,
    /// OpenAI-compatible speech API key.
    Tts,
    /// Baidu speech API key.
    BaiduApiKey,
    /// Baidu speech secret key.
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

/// API keys live in a user-only file in the app config directory.
/// The system keychain prompts for the login password, so it is not used.
pub struct SecretStore {
    path: PathBuf,
    /// Avoids reading the file again on every request.
    cache: Mutex<HashMap<KeyKind, Option<String>>>,
}

impl SecretStore {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            cache: Mutex::default(),
        }
    }

    pub fn get(&self, kind: KeyKind) -> Option<String> {
        if let Some(cached) = self.cache.lock().unwrap().get(&kind) {
            return cached.clone();
        }
        let value = self.read_file().remove(kind.account());
        self.cache.lock().unwrap().insert(kind, value.clone());
        value
    }

    /// Stores `key`, or deletes it when it is empty.
    pub fn set(&self, kind: KeyKind, key: &str) -> AppResult<()> {
        let key = key.trim();
        let mut file = self.read_file();
        if key.is_empty() {
            file.remove(kind.account());
        } else {
            file.insert(kind.account().to_string(), key.to_string());
        }
        self.write_file(&file)?;

        let value = (!key.is_empty()).then(|| key.to_string());
        self.cache.lock().unwrap().insert(kind, value);
        Ok(())
    }

    fn read_file(&self) -> HashMap<String, String> {
        fs::read_to_string(&self.path)
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default()
    }

    fn write_file(&self, map: &HashMap<String, String>) -> AppResult<()> {
        if map.is_empty() {
            let _ = fs::remove_file(&self.path);
            return Ok(());
        }
        write_atomic(&self.path, &serde_json::to_vec(map)?)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&self.path, fs::Permissions::from_mode(0o600))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store() -> (SecretStore, PathBuf) {
        let dir = std::env::temp_dir().join(format!("uni-translate-test-{}", uuid::Uuid::new_v4()));
        let store = SecretStore::new(dir.join("secrets.json"));
        (store, dir)
    }

    #[test]
    fn stores_keys_in_a_private_file_and_deletes_them() {
        let (store, dir) = temp_store();
        assert_eq!(store.get(KeyKind::Chat), None);

        store.set(KeyKind::Chat, " sk-123 ").unwrap();
        store.set(KeyKind::Tts, "sk-tts").unwrap();
        assert_eq!(store.get(KeyKind::Chat).as_deref(), Some("sk-123"));

        let reloaded = SecretStore::new(store.path.clone());
        assert_eq!(reloaded.get(KeyKind::Tts).as_deref(), Some("sk-tts"));

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&store.path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }

        store.set(KeyKind::Chat, "").unwrap();
        store.set(KeyKind::Tts, "").unwrap();
        assert_eq!(store.get(KeyKind::Chat), None);
        assert!(!store.path.exists());
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
