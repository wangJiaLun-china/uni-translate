use std::fs;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

pub const DEFAULT_HOTKEY: &str = "Alt+D";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    /// OpenAI-compatible API root, e.g. `https://api.deepseek.com/v1`.
    pub base_url: String,
    pub model: String,
    pub temperature: f32,
    pub target_lang: String,
    /// Used instead of `target_lang` when the source text is already in it.
    pub secondary_lang: String,
    pub tts: TtsSettings,
    pub hotkey: String,
    pub history_enabled: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            base_url: String::new(),
            model: String::new(),
            temperature: 0.3,
            target_lang: "zh-CN".into(),
            secondary_lang: "en".into(),
            tts: TtsSettings::default(),
            hotkey: DEFAULT_HOTKEY.into(),
            history_enabled: true,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TtsProvider {
    /// An OpenAI-compatible `/audio/speech` endpoint.
    #[default]
    Openai,
    /// Baidu AI Cloud short-text speech synthesis.
    Baidu,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TtsSettings {
    pub provider: TtsProvider,
    /// Reuse the translation `base_url` and API key.
    pub use_same_endpoint: bool,
    pub base_url: String,
    pub model: String,
    pub voice: String,
    /// Playback speed as a multiple of normal, for both providers.
    pub speed: f32,
    /// Baidu voice id (`per`), e.g. 0 for 度小美.
    pub baidu_voice: u32,
}

impl Default for TtsSettings {
    fn default() -> Self {
        Self {
            provider: TtsProvider::Openai,
            use_same_endpoint: true,
            base_url: String::new(),
            model: "tts-1".into(),
            voice: "alloy".into(),
            speed: 1.0,
            baidu_voice: 0,
        }
    }
}

impl Settings {
    /// Trims user input and checks the fields that would otherwise fail later
    /// with a confusing network error.
    pub fn normalized(mut self) -> AppResult<Self> {
        self.base_url = normalize_base_url(&self.base_url);
        self.model = self.model.trim().to_string();
        self.hotkey = self.hotkey.trim().to_string();
        self.tts.base_url = normalize_base_url(&self.tts.base_url);
        self.tts.model = self.tts.model.trim().to_string();
        self.tts.voice = self.tts.voice.trim().to_string();
        validate_base_url(&self.base_url)?;
        validate_base_url(&self.tts.base_url)?;
        self.temperature = self.temperature.clamp(0.0, 2.0);
        self.tts.speed = self.tts.speed.clamp(0.25, 4.0);
        if self.hotkey.is_empty() {
            self.hotkey = DEFAULT_HOTKEY.into();
        }
        Ok(self)
    }
}

pub fn normalize_base_url(url: &str) -> String {
    url.trim().trim_end_matches('/').to_string()
}

fn validate_base_url(url: &str) -> AppResult<()> {
    if url.is_empty() || url.starts_with("http://") || url.starts_with("https://") {
        Ok(())
    } else {
        Err(AppError::invalid(format!(
            "接口地址必须以 http:// 或 https:// 开头：{url}"
        )))
    }
}

/// Joins an API root and an endpoint path, tolerating extra slashes.
pub fn join_url(base_url: &str, path: &str) -> String {
    format!(
        "{}/{}",
        normalize_base_url(base_url),
        path.trim_start_matches('/')
    )
}

/// Settings persisted as JSON in the app config directory.
pub struct ConfigStore {
    path: PathBuf,
    settings: RwLock<Settings>,
}

impl ConfigStore {
    pub fn load(path: PathBuf) -> Self {
        let settings = match fs::read_to_string(&path) {
            Ok(raw) => serde_json::from_str(&raw).unwrap_or_else(|err| {
                log::warn!("ignoring unreadable settings file {path:?}: {err}");
                Settings::default()
            }),
            Err(_) => Settings::default(),
        };
        Self {
            path,
            settings: RwLock::new(settings),
        }
    }

    pub fn get(&self) -> Settings {
        self.settings.read().unwrap().clone()
    }

    pub fn save(&self, settings: Settings) -> AppResult<()> {
        write_atomic(&self.path, &serde_json::to_vec_pretty(&settings)?)?;
        *self.settings.write().unwrap() = settings;
        Ok(())
    }
}

/// Writes through a temp file so a crash never leaves half a settings file.
pub fn write_atomic(path: &Path, contents: &[u8]) -> AppResult<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, contents)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn join_url_handles_slashes() {
        assert_eq!(
            join_url("https://api.x.com/v1/", "/chat/completions"),
            "https://api.x.com/v1/chat/completions"
        );
        assert_eq!(
            join_url(" https://api.x.com/v1 ", "models"),
            "https://api.x.com/v1/models"
        );
    }

    #[test]
    fn missing_fields_fall_back_to_defaults() {
        let s: Settings =
            serde_json::from_str(r#"{"baseUrl":"https://a/v1","tts":{"voice":"nova"}}"#).unwrap();
        assert_eq!(s.base_url, "https://a/v1");
        assert_eq!(s.target_lang, "zh-CN");
        assert_eq!(s.tts.voice, "nova");
        assert_eq!(s.tts.model, "tts-1");
        assert!(s.tts.use_same_endpoint);
        assert_eq!(s.tts.provider, TtsProvider::Openai);
        assert_eq!(s.tts.baidu_voice, 0);

        let s: Settings =
            serde_json::from_str(r#"{"tts":{"provider":"baidu","baiduVoice":5003}}"#).unwrap();
        assert_eq!(s.tts.provider, TtsProvider::Baidu);
        assert_eq!(s.tts.baidu_voice, 5003);
    }

    #[test]
    fn normalized_trims_and_validates() {
        let s = Settings {
            base_url: " https://a/v1/ ".into(),
            hotkey: " ".into(),
            temperature: 9.0,
            ..Settings::default()
        }
        .normalized()
        .unwrap();
        assert_eq!(s.base_url, "https://a/v1");
        assert_eq!(s.hotkey, DEFAULT_HOTKEY);
        assert_eq!(s.temperature, 2.0);

        let bad = Settings {
            base_url: "api.x.com".into(),
            ..Settings::default()
        };
        assert!(bad.normalized().is_err());
    }

    #[test]
    fn store_round_trips() {
        let dir = std::env::temp_dir().join(format!("uni-translate-test-{}", uuid::Uuid::new_v4()));
        let path = dir.join("settings.json");
        let store = ConfigStore::load(path.clone());
        assert_eq!(store.get(), Settings::default());

        let s = Settings {
            model: "deepseek-chat".into(),
            ..Settings::default()
        };
        store.save(s.clone()).unwrap();
        assert_eq!(ConfigStore::load(path).get(), s);
        fs::remove_dir_all(dir).unwrap();
    }
}
