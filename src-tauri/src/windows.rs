//! The translation card (main window), the settings/history window and the tray.

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};

use crate::error::{AppError, AppResult};
use crate::state::AppState;

pub const MAIN_LABEL: &str = "main";
pub const MANAGE_LABEL: &str = "manage";
/// Asks the manage window to switch to a route, e.g. `/settings`.
pub const NAVIGATE_EVENT: &str = "app://navigate";

const MANAGE_ROUTES: &[&str] = &["/settings", "/history"];

pub fn create_tray(app: &AppHandle) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "显示翻译卡片", true, None::<&str>)?;
    let history = MenuItem::with_id(app, "history", "历史记录", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "设置", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&show, &history, &settings, &separator, &quit])?;

    let mut tray = TrayIconBuilder::with_id("main")
        .tooltip("uni-translate")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| {
            let result = match event.id.as_ref() {
                "show" => {
                    show_main(app);
                    Ok(())
                }
                "history" => open_manage_window(app, "/history"),
                "settings" => open_manage_window(app, "/settings"),
                "quit" => {
                    app.exit(0);
                    Ok(())
                }
                _ => Ok(()),
            };
            if let Err(err) = result {
                log::error!("tray action failed: {err}");
            }
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

pub fn show_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(MAIN_LABEL) {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

/// Shows the settings/history window at `route`, creating it on first use.
fn open_manage_window(app: &AppHandle, route: &str) -> AppResult<()> {
    if !MANAGE_ROUTES.contains(&route) {
        return Err(AppError::invalid(format!("unknown page: {route}")));
    }
    if let Some(window) = app.get_webview_window(MANAGE_LABEL) {
        window.show()?;
        window.unminimize()?;
        window.set_focus()?;
        app.emit_to(MANAGE_LABEL, NAVIGATE_EVENT, route)?;
        return Ok(());
    }
    // A new window cannot receive events before it loads; it asks for its
    // route with `take_manage_route` instead.
    *app.state::<AppState>().manage_route.lock().unwrap() = Some(route.to_string());
    WebviewWindowBuilder::new(app, MANAGE_LABEL, WebviewUrl::App("index.html".into()))
        .title("uni-translate 设置与历史")
        .inner_size(820.0, 600.0)
        .min_inner_size(560.0, 420.0)
        .center()
        .build()?;
    Ok(())
}

/// Async so that creating the window does not deadlock on Windows.
#[tauri::command]
pub async fn open_manage(app: AppHandle, route: String) -> AppResult<()> {
    open_manage_window(&app, &route)
}

#[tauri::command]
pub fn take_manage_route(state: State<'_, AppState>) -> Option<String> {
    state.manage_route.lock().unwrap().take()
}
