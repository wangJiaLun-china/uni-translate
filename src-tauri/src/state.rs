use std::collections::HashMap;
use std::sync::Mutex;

use tauri::async_runtime::JoinHandle;

use crate::baidu::BaiduTts;
use crate::client::{Endpoint, OpenAiClient};
use crate::config::{ConfigStore, Settings};
use crate::history::History;
use crate::secrets::{KeyKind, SecretStore};

/// A running translation, at most one per window.
pub struct Task {
    pub id: String,
    pub handle: JoinHandle<()>,
}

pub struct AppState {
    pub config: ConfigStore,
    pub secrets: SecretStore,
    pub client: OpenAiClient,
    pub baidu: BaiduTts,
    pub history: History,
    /// Keyed by window label.
    pub tasks: Mutex<HashMap<String, Task>>,
    /// Text grabbed by the hotkey, waiting for the popup to pick it up.
    pub pending_selection: Mutex<Option<String>>,
    /// Page the settings/history window should open on once it loads.
    pub manage_route: Mutex<Option<String>>,
}

impl AppState {
    pub fn chat_endpoint(&self, settings: &Settings) -> Endpoint {
        Endpoint {
            base_url: settings.base_url.clone(),
            api_key: self.secrets.get(KeyKind::Chat),
        }
    }

    pub fn tts_endpoint(&self, settings: &Settings) -> Endpoint {
        if settings.tts.use_same_endpoint {
            self.chat_endpoint(settings)
        } else {
            Endpoint {
                base_url: settings.tts.base_url.clone(),
                api_key: self.secrets.get(KeyKind::Tts),
            }
        }
    }
}
