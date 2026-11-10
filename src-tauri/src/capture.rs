//! Screenshot translation: pick a region of the screen, then let the popup
//! card read its text and translate it.
//!
//! macOS uses the system's own region picker (`screencapture -i`). Windows
//! and Linux capture the monitor under the pointer and show it in a
//! full-screen `capture` window where the user drags out the region.

use std::io::Cursor;

use image::{DynamicImage, ImageFormat};
use serde::Deserialize;
use tauri::ipc::Response;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::error::{AppError, AppResult};
use crate::selection::{self, POPUP_LABEL};
use crate::state::AppState;

/// Tells the popup that a screenshot is waiting in `recognize_image`.
pub const CAPTURE_EVENT: &str = "popup://capture";
/// Longest side sent to the model; larger screenshots are scaled down.
const MAX_SIDE: u32 = 2000;
/// Smaller selections are treated as a stray click.
const MIN_SIDE: u32 = 8;

pub fn on_hotkey(app: &AppHandle) {
    let app = app.clone();
    // The region picker blocks until the user is done; stay off the event loop.
    std::thread::spawn(move || {
        if let Err(err) = start(&app) {
            log::error!("screenshot failed: {err}");
        }
    });
}

/// Scales `image` down so neither side exceeds `MAX_SIDE`, then encodes PNG.
fn to_png(image: DynamicImage) -> AppResult<Vec<u8>> {
    let image = if image.width().max(image.height()) > MAX_SIDE {
        image.resize(MAX_SIDE, MAX_SIDE, image::imageops::FilterType::Triangle)
    } else {
        image
    };
    let mut png = Vec::new();
    image.write_to(&mut Cursor::new(&mut png), ImageFormat::Png)?;
    Ok(png)
}

/// Hands a finished screenshot to the popup card next to the pointer.
fn deliver(app: &AppHandle, image: DynamicImage) -> AppResult<()> {
    let png = to_png(image)?;
    *app.state::<AppState>().pending_image.lock().unwrap() = Some(png);
    selection::show_popup_at_pointer(app)?;
    app.emit_to(POPUP_LABEL, CAPTURE_EVENT, ())?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn start(app: &AppHandle) -> AppResult<()> {
    let path = std::env::temp_dir().join(format!("uni-translate-{}.png", uuid::Uuid::new_v4()));
    // -i: let the user pick a region or window; -x: no shutter sound.
    // Pressing Esc cancels and writes no file.
    let status = std::process::Command::new("/usr/sbin/screencapture")
        .arg("-i")
        .arg("-x")
        .arg(&path)
        .status()?;
    let bytes = std::fs::read(&path).unwrap_or_default();
    let _ = std::fs::remove_file(&path);
    if !status.success() || bytes.is_empty() {
        return Ok(());
    }
    deliver(
        app,
        image::load_from_memory_with_format(&bytes, ImageFormat::Png)?,
    )
}

#[cfg(not(target_os = "macos"))]
use overlay::start;

#[cfg(not(target_os = "macos"))]
mod overlay {
    use tauri::{
        AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindow,
        WebviewWindowBuilder,
    };

    use crate::error::{AppError, AppResult};
    use crate::state::AppState;

    pub const CAPTURE_LABEL: &str = "capture";
    /// Tells the overlay to load the new screenshot from `screen_image`.
    const START_EVENT: &str = "capture://start";

    pub fn start(app: &AppHandle) -> AppResult<()> {
        let cursor = app.cursor_position()?;
        let monitor =
            xcap::Monitor::from_point(cursor.x as i32, cursor.y as i32).map_err(capture_error)?;
        let image = monitor.capture_image().map_err(capture_error)?;
        let origin = PhysicalPosition::new(
            monitor.x().map_err(capture_error)?,
            monitor.y().map_err(capture_error)?,
        );
        let size = PhysicalSize::new(image.width(), image.height());
        *app.state::<AppState>().screen.lock().unwrap() = Some(image);

        // The window is transparent and stays empty until the page has loaded
        // the new screenshot, so it never flashes the previous one.
        let window = overlay_window(app)?;
        window.set_position(origin)?;
        window.set_size(size)?;
        window.show()?;
        window.set_focus()?;
        app.emit_to(CAPTURE_LABEL, START_EVENT, ())?;
        Ok(())
    }

    fn overlay_window(app: &AppHandle) -> AppResult<WebviewWindow> {
        if let Some(window) = app.get_webview_window(CAPTURE_LABEL) {
            return Ok(window);
        }
        Ok(
            WebviewWindowBuilder::new(app, CAPTURE_LABEL, WebviewUrl::App("index.html".into()))
                .title("uni-translate 截图")
                .decorations(false)
                .always_on_top(true)
                .skip_taskbar(true)
                .resizable(false)
                .shadow(false)
                .transparent(true)
                .visible(false)
                .build()?,
        )
    }

    fn capture_error(err: xcap::XCapError) -> AppError {
        AppError::invalid(format!("截图失败：{err}"))
    }
}

// The overlay's commands. They exist on every platform so the command list
// is the same everywhere; on macOS nothing calls them.

/// The full screenshot as JPEG, for the overlay's background.
#[tauri::command]
pub fn screen_image(state: State<'_, AppState>) -> AppResult<Response> {
    let screen = state.screen.lock().unwrap();
    let image = screen
        .as_ref()
        .ok_or_else(|| AppError::invalid("没有正在进行的截图"))?;
    let rgb = DynamicImage::ImageRgba8(image.clone()).into_rgb8();
    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut Cursor::new(&mut jpeg), 85)
        .encode_image(&rgb)?;
    Ok(Response::new(jpeg))
}

/// A dragged-out region and the overlay's size, in CSS pixels.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Selection {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    view_width: f64,
    view_height: f64,
}

/// Crops the selection and hands it to the popup card. Tiny selections
/// count as a stray click.
#[tauri::command]
pub fn finish_capture(
    app: AppHandle,
    state: State<'_, AppState>,
    selection: Selection,
) -> AppResult<()> {
    let Some(image) = state.screen.lock().unwrap().take() else {
        return Ok(());
    };
    let s = selection;
    let rect = crop_rect(
        (image.width(), image.height()),
        (s.x, s.y, s.width, s.height),
        (s.view_width, s.view_height),
    );
    let Some((cx, cy, cw, ch)) = rect else {
        return Ok(());
    };
    let cropped = image::imageops::crop_imm(&image, cx, cy, cw, ch).to_image();
    deliver(&app, DynamicImage::ImageRgba8(cropped))
}

#[tauri::command]
pub fn cancel_capture(state: State<'_, AppState>) {
    state.screen.lock().unwrap().take();
}

/// Maps a selection in the overlay's CSS pixels onto the screenshot's pixels.
/// Returns `None` for a selection smaller than `MIN_SIDE` on either side.
fn crop_rect(
    image: (u32, u32),
    selection: (f64, f64, f64, f64),
    view: (f64, f64),
) -> Option<(u32, u32, u32, u32)> {
    let (iw, ih) = image;
    let (x, y, w, h) = selection;
    let (vw, vh) = view;
    if vw <= 0.0 || vh <= 0.0 {
        return None;
    }
    let (sx, sy) = (iw as f64 / vw, ih as f64 / vh);
    let left = (x * sx).round().clamp(0.0, iw as f64) as u32;
    let top = (y * sy).round().clamp(0.0, ih as f64) as u32;
    let right = ((x + w) * sx).round().clamp(0.0, iw as f64) as u32;
    let bottom = ((y + h) * sy).round().clamp(0.0, ih as f64) as u32;
    let (cw, ch) = (right.saturating_sub(left), bottom.saturating_sub(top));
    (cw >= MIN_SIDE && ch >= MIN_SIDE).then_some((left, top, cw, ch))
}

impl From<image::ImageError> for AppError {
    fn from(err: image::ImageError) -> Self {
        AppError::invalid(format!("图片处理失败：{err}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_css_pixels_to_image_pixels() {
        // A 2x display: the overlay is 1440x900 CSS pixels over 2880x1800.
        assert_eq!(
            crop_rect((2880, 1800), (100.0, 50.0, 300.0, 40.0), (1440.0, 900.0)),
            Some((200, 100, 600, 80))
        );
        // Same size on a 1x display.
        assert_eq!(
            crop_rect((1280, 800), (10.0, 20.0, 30.0, 40.0), (1280.0, 800.0)),
            Some((10, 20, 30, 40))
        );
    }

    #[test]
    fn clamps_to_the_screen_and_ignores_stray_clicks() {
        assert_eq!(
            crop_rect((1280, 800), (1200.0, 760.0, 500.0, 500.0), (1280.0, 800.0)),
            Some((1200, 760, 80, 40))
        );
        assert_eq!(
            crop_rect((1280, 800), (5.0, 5.0, 3.0, 40.0), (1280.0, 800.0)),
            None
        );
        assert_eq!(
            crop_rect((1280, 800), (5.0, 5.0, 30.0, 40.0), (0.0, 800.0)),
            None
        );
    }

    #[test]
    fn scales_large_screenshots_down() {
        let big = DynamicImage::new_rgba8(4000, 1000);
        let png = to_png(big).unwrap();
        let back = image::load_from_memory(&png).unwrap();
        assert_eq!((back.width(), back.height()), (2000, 500));

        let small = DynamicImage::new_rgba8(300, 80);
        let back = image::load_from_memory(&to_png(small).unwrap()).unwrap();
        assert_eq!((back.width(), back.height()), (300, 80));
    }
}
