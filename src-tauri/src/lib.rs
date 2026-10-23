mod baidu;
mod client;
mod commands;
mod config;
mod error;
mod history;
mod lang;
#[cfg(target_os = "macos")]
mod popup_panel;
mod secrets;
mod selection;
mod state;
#[cfg(test)]
mod test_server;
mod translate;
mod tts;
mod windows;

use std::sync::Mutex;

use tauri::{Manager, WindowEvent};

use crate::baidu::BaiduTts;
use crate::client::OpenAiClient;
use crate::config::ConfigStore;
use crate::history::History;
use crate::secrets::SecretStore;
use crate::state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Must come first: a second launch just focuses the running app.
    let builder = tauri::Builder::default().plugin(tauri_plugin_single_instance::init(
        |app, _args, _cwd| {
            windows::show_main(app);
        },
    ));
    #[cfg(target_os = "macos")]
    let builder = builder.plugin(tauri_nspanel::init());
    builder
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(selection::plugin())
        .setup(|app| {
            // A regular app makes macOS jump to the desktop holding its windows
            // whenever it is activated, so the selection card would open on the
            // main card's desktop instead of the one being used. Running as a
            // menu bar app avoids that; the tray opens every window.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let config_dir = app.path().app_config_dir()?;
            let data_dir = app.path().app_data_dir()?;
            let state = AppState {
                config: ConfigStore::load(config_dir.join("settings.json")),
                secrets: SecretStore::new(config_dir.join("secrets.json")),
                client: OpenAiClient::new(),
                baidu: BaiduTts::new(),
                history: History::open(&data_dir.join("history.db"))?,
                tasks: Mutex::default(),
                pending_selection: Mutex::default(),
                manage_route: Mutex::default(),
            };
            let hotkey = state.config.get().hotkey;
            app.manage(state);

            if let Err(err) = selection::register(app.handle(), &hotkey) {
                log::warn!("{err}");
            }
            windows::create_tray(app.handle())?;
            #[cfg(target_os = "macos")]
            popup_panel::install(app.handle())?;
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the card or the popup only hides it; the app keeps
            // running in the tray. The settings/history window really closes.
            if let WindowEvent::CloseRequested { api, .. } = event {
                if window.label() != windows::MANAGE_LABEL {
                    api.prevent_close();
                    let _ = window.hide();
                }
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
            windows::open_manage,
            windows::take_manage_route,
        ])
        .run(tauri::generate_context!())
        .expect("error while running uni-translate");
}
