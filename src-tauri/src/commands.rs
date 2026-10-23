//! Settings, API key, language and history commands.

use tauri::{AppHandle, State};

use crate::config::Settings;
use crate::error::AppResult;
use crate::history::HistoryItem;
use crate::lang::{Language, LANGUAGES};
use crate::secrets::KeyKind;
use crate::selection;
use crate::state::AppState;

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Settings {
    state.config.get()
}

/// Validates and saves, re-registering the hotkey if it changed. Returns the
/// settings as saved.
#[tauri::command]
pub fn save_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: Settings,
) -> AppResult<Settings> {
    let settings = settings.normalized()?;
    let old = state.config.get();
    if settings.hotkey != old.hotkey {
        selection::replace(&app, &old.hotkey, &settings.hotkey)?;
    }
    state.config.save(settings.clone())?;
    Ok(settings)
}

/// An empty key deletes the stored one.
#[tauri::command]
pub fn set_api_key(state: State<'_, AppState>, kind: KeyKind, key: String) -> AppResult<()> {
    if matches!(kind, KeyKind::BaiduApiKey | KeyKind::BaiduSecretKey) {
        state.baidu.clear_token();
    }
    state.secrets.set(kind, &key)
}

#[tauri::command]
pub fn has_api_key(state: State<'_, AppState>, kind: KeyKind) -> bool {
    state.secrets.get(kind).is_some()
}

#[tauri::command]
pub fn list_languages() -> &'static [Language] {
    LANGUAGES
}

#[tauri::command]
pub fn list_history(
    state: State<'_, AppState>,
    query: Option<String>,
    limit: u32,
    offset: u32,
) -> AppResult<Vec<HistoryItem>> {
    state.history.list(query.as_deref(), limit.min(200), offset)
}

#[tauri::command]
pub fn delete_history(state: State<'_, AppState>, id: i64) -> AppResult<()> {
    state.history.delete(id)
}

#[tauri::command]
pub fn clear_history(state: State<'_, AppState>) -> AppResult<()> {
    state.history.clear()
}
