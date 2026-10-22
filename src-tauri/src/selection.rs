//! Global hotkey: grab the selected text and show it in the popup window.

use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, State};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

use crate::error::{AppError, AppResult};
use crate::state::AppState;

pub const POPUP_LABEL: &str = "popup";
/// Tells the popup that new text is waiting in `take_pending_selection`.
pub const SELECTION_EVENT: &str = "popup://selection";
/// Logical size of the popup, as in `tauri.conf.json`.
const POPUP_SIZE: (f64, f64) = (420.0, 320.0);

pub fn plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri_plugin_global_shortcut::Builder::new()
        .with_handler(|app, _shortcut, event| {
            if event.state() == ShortcutState::Pressed {
                on_hotkey(app);
            }
        })
        .build()
}

fn parse(hotkey: &str) -> AppResult<Shortcut> {
    hotkey
        .parse()
        .map_err(|err| AppError::invalid(format!("快捷键格式无效「{hotkey}」：{err}")))
}

pub fn register(app: &AppHandle, hotkey: &str) -> AppResult<()> {
    let shortcut = parse(hotkey)?;
    let shortcuts = app.global_shortcut();
    let _ = shortcuts.unregister_all();
    shortcuts.register(shortcut).map_err(|err| {
        AppError::invalid(format!(
            "快捷键「{hotkey}」注册失败，可能已被其他程序占用：{err}"
        ))
    })
}

/// Switches to `new`, keeping `old` registered if `new` cannot be registered.
pub fn replace(app: &AppHandle, old: &str, new: &str) -> AppResult<()> {
    register(app, new).inspect_err(|_| {
        let _ = register(app, old);
    })
}

fn on_hotkey(app: &AppHandle) {
    let app = app.clone();
    // Grabbing the selection sleeps and may simulate key presses; keep it off
    // the event loop.
    std::thread::spawn(move || {
        let text = selected_text().unwrap_or_default();
        *app.state::<AppState>().pending_selection.lock().unwrap() = Some(text);
        if let Err(err) = show_popup(&app) {
            log::error!("failed to show popup: {err}");
        }
        let _ = app.emit_to(POPUP_LABEL, SELECTION_EVENT, ());
    });
}

/// Returns the text grabbed by the last hotkey press, once.
#[tauri::command]
pub fn take_pending_selection(state: State<'_, AppState>) -> Option<String> {
    state.pending_selection.lock().unwrap().take()
}

fn show_popup(app: &AppHandle) -> AppResult<()> {
    let popup = app
        .get_webview_window(POPUP_LABEL)
        .ok_or_else(|| AppError::invalid("popup window missing"))?;
    if let Ok(cursor) = app.cursor_position() {
        let size = popup.outer_size()?;
        let (width, height) = if size.width == 0 {
            // GTK reports 0x0 for a window that has never been shown.
            let scale = popup.scale_factor()?;
            (POPUP_SIZE.0 * scale, POPUP_SIZE.1 * scale)
        } else {
            (size.width as f64, size.height as f64)
        };
        let mut x = cursor.x + 12.0;
        let mut y = cursor.y + 16.0;
        // Keep the popup on the monitor under the cursor.
        if let Ok(Some(monitor)) = app.monitor_from_point(cursor.x, cursor.y) {
            let origin = monitor.position();
            let area = monitor.size();
            let right = origin.x as f64 + area.width as f64;
            let bottom = origin.y as f64 + area.height as f64;
            if x + width > right {
                x = (cursor.x - width - 12.0).max(origin.x as f64);
            }
            if y + height > bottom {
                y = (cursor.y - height - 16.0).max(origin.y as f64);
            }
        }
        popup.set_position(PhysicalPosition::new(x, y))?;
    }
    popup.show()?;
    popup.unminimize()?;
    popup.set_focus()?;
    Ok(())
}

/// Linux has a "primary selection" that always holds the selected text.
#[cfg(target_os = "linux")]
fn selected_text() -> Option<String> {
    use arboard::{Clipboard, GetExtLinux, LinuxClipboardKind};
    let text = Clipboard::new()
        .ok()?
        .get()
        .clipboard(LinuxClipboardKind::Primary)
        .text()
        .ok()?;
    Some(text.trim().to_string())
}

/// Elsewhere, simulate the copy shortcut and read the clipboard, then put the
/// user's clipboard back.
#[cfg(any(windows, target_os = "macos"))]
fn selected_text() -> Option<String> {
    use enigo::{Direction, Enigo, Key, Keyboard, Settings};
    use std::time::Duration;

    let mut clipboard = arboard::Clipboard::new().ok()?;
    let previous = clipboard.get_text().ok();
    let _ = clipboard.clear();

    let copied = (|| -> Option<String> {
        let mut enigo = Enigo::new(&Settings::default()).ok()?;
        // The hotkey's modifiers may still be held; release them so the copy
        // shortcut is not combined with them.
        for key in [Key::Alt, Key::Shift, Key::Control, Key::Meta] {
            let _ = enigo.key(key, Direction::Release);
        }
        #[cfg(windows)]
        let (modifier, c) = (Key::Control, Key::C);
        #[cfg(target_os = "macos")]
        let (modifier, c) = (Key::Meta, Key::Unicode('c'));
        enigo.key(modifier, Direction::Press).ok()?;
        let _ = enigo.key(c, Direction::Click);
        enigo.key(modifier, Direction::Release).ok()?;

        for _ in 0..20 {
            std::thread::sleep(Duration::from_millis(25));
            if let Ok(text) = clipboard.get_text() {
                if !text.trim().is_empty() {
                    return Some(text.trim().to_string());
                }
            }
        }
        None
    })();

    if let Some(previous) = previous {
        let _ = clipboard.set_text(previous);
    }
    copied
}

#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
fn selected_text() -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hotkeys() {
        assert!(parse("Alt+D").is_ok());
        assert!(parse("CommandOrControl+Shift+KeyT").is_ok());
        assert!(parse("Super+Alt+Digit1").is_ok());
        assert!(parse("Alt+").is_err());
    }
}
