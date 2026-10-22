mod client;
mod commands;
mod config;
mod error;
mod history;
mod lang;
mod secrets;
mod selection;
mod state;
mod translate;
mod tray;
mod tts;

use std::sync::Mutex;

use tauri::{Manager, WindowEvent};

use crate::client::OpenAiClient;
use crate::config::ConfigStore;
use crate::history::History;
use crate::secrets::SecretStore;
use crate::state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Must come first: a second launch just focuses the running app.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            tray::show_main(app, None);
        }))
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(selection::plugin())
        .setup(|app| {
            let config_dir = app.path().app_config_dir()?;
            let data_dir = app.path().app_data_dir()?;
            let state = AppState {
                config: ConfigStore::load(config_dir.join("settings.json")),
                secrets: SecretStore::new(config_dir.join("secrets.json")),
                client: OpenAiClient::new(),
                history: History::open(&data_dir.join("history.db"))?,
                tasks: Mutex::default(),
                pending_selection: Mutex::default(),
            };
            let hotkey = state.config.get().hotkey;
            app.manage(state);

            if let Err(err) = selection::register(app.handle(), &hotkey) {
                log::warn!("{err}");
            }
            tray::create(app.handle())?;
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing only hides; the app keeps running in the tray.
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::save_settings,
            commands::set_api_key,
            commands::has_api_key,
            commands::list_languages,
            commands::list_history,
            commands::delete_history,
            commands::clear_history,
            translate::translate,
            translate::cancel_translation,
            translate::test_connection,
            translate::list_models,
            tts::speak,
            tts::split_tts_text,
            selection::take_pending_selection,
        ])
        .run(tauri::generate_context!())
        .expect("error while running uni-translate");
}
