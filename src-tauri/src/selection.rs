//! Global hotkey: grab the selected text and show it in the popup window.

#[cfg(not(target_os = "macos"))]
use tauri::PhysicalPosition;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

use crate::error::{AppError, AppResult};
use crate::state::AppState;

pub const POPUP_LABEL: &str = "popup";
/// Tells the popup that new text is waiting in `take_pending_selection`.
pub const SELECTION_EVENT: &str = "popup://selection";
/// Logical size of the popup, as in `tauri.conf.json`.
const POPUP_SIZE: (f64, f64) = (380.0, 220.0);

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
    // Read the cursor before the copy shortcut, which can take half a second.
    // Waiting until then puts the card wherever the pointer moved.
    #[cfg(target_os = "macos")]
    let cursor = macos_mouse_location();
    // Waiting for the copied text can take half a second, so stay off the
    // event loop. The key simulation itself is sent back to the main thread.
    std::thread::spawn(move || {
        let text = selected_text(&app).unwrap_or_default();
        *app.state::<AppState>().pending_selection.lock().unwrap() = Some(text);
        #[cfg(target_os = "macos")]
        let shown = show_popup_macos(&app, cursor);
        #[cfg(not(target_os = "macos"))]
        let shown = show_popup(&app);
        if let Err(err) = shown {
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

#[cfg(not(target_os = "macos"))]
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

/// Gap between the pointer and the card, in screen points.
const CURSOR_GAP: (f64, f64) = (12.0, 16.0);

/// Top-left of the card in a bottom-left screen coordinate system.
/// `screen` is `(x, y, width, height)` of the visible area under the pointer.
fn popup_top_left(
    cursor: (f64, f64),
    window: (f64, f64),
    screen: (f64, f64, f64, f64),
) -> (f64, f64) {
    let (cx, cy) = cursor;
    let (width, height) = window;
    let (sx, sy, sw, sh) = screen;
    let right = sx + sw;
    let top = sy + sh;

    let mut x = cx + CURSOR_GAP.0;
    if x + width > right {
        x = cx - width - CURSOR_GAP.0;
    }
    x = x.clamp(sx, (right - width).max(sx));

    // `y` is the top edge. A window that would fall through the bottom of the
    // screen is flipped above the pointer.
    let mut y = cy - CURSOR_GAP.1;
    if y - height < sy {
        y = cy + CURSOR_GAP.1 + height;
    }
    y = y.clamp(sy + height, top.max(sy + height));
    (x, y)
}

/// Places the card beside the pointer on the screen it is on.
/// Tauri's position is wrong across displays with different scale factors.
#[cfg(target_os = "macos")]
fn show_popup_macos(app: &AppHandle, cursor: objc2_foundation::NSPoint) -> AppResult<()> {
    let (tx, rx) = std::sync::mpsc::channel();
    let app_for_ui = app.clone();
    app.run_on_main_thread(move || {
        let _ = tx.send(place_popup_macos(&app_for_ui, cursor));
    })?;
    rx.recv_timeout(std::time::Duration::from_secs(2))
        .map_err(|_| AppError::invalid("显示弹窗超时"))?
}

#[cfg(target_os = "macos")]
fn macos_mouse_location() -> objc2_foundation::NSPoint {
    objc2_app_kit::NSEvent::mouseLocation()
}

#[cfg(target_os = "macos")]
fn place_popup_macos(app: &AppHandle, cursor: objc2_foundation::NSPoint) -> AppResult<()> {
    use objc2_foundation::{NSPoint, NSRect};
    use tauri_nspanel::ManagerExt;

    let panel = app
        .get_webview_panel(POPUP_LABEL)
        .map_err(|_| AppError::invalid("popup panel missing"))?;

    let frame = panel.as_panel().frame();
    let mut size = (frame.size.width, frame.size.height);
    if size.0 < 1.0 || size.1 < 1.0 {
        size = POPUP_SIZE;
    }
    let screen = visible_screen_containing(cursor).unwrap_or(NSRect {
        origin: NSPoint { x: 0.0, y: 0.0 },
        size: objc2_foundation::NSSize {
            width: size.0 + CURSOR_GAP.0 * 2.0,
            height: size.1 + CURSOR_GAP.1 * 2.0,
        },
    });
    let (x, y) = popup_top_left(
        (cursor.x, cursor.y),
        size,
        (
            screen.origin.x,
            screen.origin.y,
            screen.size.width,
            screen.size.height,
        ),
    );

    panel.as_panel().setFrameTopLeftPoint(NSPoint { x, y });
    // Takes keyboard focus without activating the app, so macOS neither
    // switches desktops nor leaves the full-screen app in front.
    panel.show_and_make_key();
    Ok(())
}

#[cfg(target_os = "macos")]
fn visible_screen_containing(
    cursor: objc2_foundation::NSPoint,
) -> Option<objc2_foundation::NSRect> {
    use objc2::MainThreadMarker;
    use objc2_app_kit::NSScreen;

    let marker = MainThreadMarker::new()?;
    let screens = NSScreen::screens(marker);
    let mut fallback = None;
    for index in 0..screens.count() {
        let screen = screens.objectAtIndex(index);
        let frame = screen.frame();
        if contains_point(frame, cursor) {
            return Some(screen.visibleFrame());
        }
        if fallback.is_none() {
            fallback = Some(screen.visibleFrame());
        }
    }
    fallback
}

#[cfg(target_os = "macos")]
fn contains_point(rect: objc2_foundation::NSRect, point: objc2_foundation::NSPoint) -> bool {
    point.x >= rect.origin.x
        && point.y >= rect.origin.y
        && point.x < rect.origin.x + rect.size.width
        && point.y < rect.origin.y + rect.size.height
}

/// Linux has a "primary selection" that always holds the selected text.
#[cfg(target_os = "linux")]
fn selected_text(_app: &AppHandle) -> Option<String> {
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
fn selected_text(app: &AppHandle) -> Option<String> {
    use std::time::Duration;

    let mut clipboard = arboard::Clipboard::new().ok()?;
    let previous = clipboard.get_text().ok();
    let _ = clipboard.clear();

    let copied = (|| -> Option<String> {
        if !simulate_copy(app) {
            return None;
        }
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

/// Posts the copy shortcut. On macOS this has to happen on the main thread:
/// looking up a character on the current input source from a background
/// thread trips `dispatch_assert_queue` and aborts the process.
#[cfg(target_os = "macos")]
fn simulate_copy(app: &AppHandle) -> bool {
    let (tx, rx) = std::sync::mpsc::channel();
    if app
        .run_on_main_thread(move || {
            let _ = tx.send(press_copy());
        })
        .is_err()
    {
        return false;
    }
    rx.recv_timeout(std::time::Duration::from_secs(2))
        .unwrap_or(false)
}

#[cfg(windows)]
fn simulate_copy(_app: &AppHandle) -> bool {
    press_copy()
}

#[cfg(any(windows, target_os = "macos"))]
fn press_copy() -> bool {
    use enigo::{Direction, Enigo, Key, Keyboard, Settings};

    let Ok(mut enigo) = Enigo::new(&Settings::default()) else {
        return false;
    };
    // The hotkey's modifiers may still be held; release them so the copy
    // shortcut is not combined with them.
    for key in [Key::Alt, Key::Shift, Key::Control, Key::Meta] {
        let _ = enigo.key(key, Direction::Release);
    }
    #[cfg(windows)]
    let (modifier, c) = (Key::Control, Key::C);
    #[cfg(target_os = "macos")]
    // kVK_ANSI_C. The copy shortcut is this key, not whichever key currently
    // produces the letter "c" under the active input source.
    let (modifier, c) = (Key::Meta, Key::Other(0x08));
    if enigo.key(modifier, Direction::Press).is_err() {
        return false;
    }
    let _ = enigo.key(c, Direction::Click);
    enigo.key(modifier, Direction::Release).is_ok()
}

#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
fn selected_text(_app: &AppHandle) -> Option<String> {
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

    #[test]
    fn places_the_card_beside_the_pointer() {
        let screen = (0.0, 0.0, 1440.0, 900.0);
        let (x, y) = popup_top_left((400.0, 500.0), (380.0, 220.0), screen);
        assert_eq!((x, y), (412.0, 484.0));
    }

    #[test]
    fn keeps_the_card_on_the_screen_under_the_pointer() {
        let screen = (-1920.0, 0.0, 1920.0, 1080.0);
        let (x, y) = popup_top_left((-20.0, 40.0), (380.0, 220.0), screen);
        assert!(x >= -1920.0 && x + 380.0 <= 0.0);
        assert!(y - 220.0 >= 0.0 && y <= 1080.0);
        assert!(
            x < -20.0,
            "card should sit to the left of a pointer at the edge"
        );
        assert!(y > 40.0, "card should sit above a pointer at the bottom");
    }
}
