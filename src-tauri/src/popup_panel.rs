//! macOS only: turns the selection card into a non-activating floating panel.
//!
//! A regular window makes macOS activate the app and jump to the Space that
//! holds the app's windows, and it can never be drawn over another app's
//! fullscreen Space. A non-activating panel can become the key window without
//! activating the app, so it opens on the Space the user is working in.

use tauri::{AppHandle, Manager};
use tauri_nspanel::{tauri_panel, CollectionBehavior, PanelLevel, StyleMask, WebviewWindowExt};

use crate::selection::POPUP_LABEL;

tauri_panel! {
    panel!(PopupPanel {
        config: {
            can_become_key_window: true,
            is_floating_panel: true
        }
    })
}

/// Converts the `popup` window declared in `tauri.conf.json` into a panel.
/// Must run on the main thread (the `setup` hook does).
pub fn install(app: &AppHandle) -> tauri::Result<()> {
    let window = app
        .get_webview_window(POPUP_LABEL)
        .ok_or(tauri::Error::WindowNotFound)?;
    let panel = window.to_panel::<PopupPanel>()?;

    if let Err(err) = panel.add_style_mask(StyleMask::empty().nonactivating_panel().value()) {
        log::warn!("popup panel: {err}");
    }
    panel.set_level(PanelLevel::Status.value());
    panel.set_collection_behavior(
        CollectionBehavior::new()
            .can_join_all_spaces()
            .full_screen_auxiliary()
            .ignores_cycle()
            .value(),
    );
    // The card hides itself when it loses focus; AppKit must not hide it
    // earlier just because the app is not active.
    panel.set_hides_on_deactivate(false);
    panel.hide();
    Ok(())
}
