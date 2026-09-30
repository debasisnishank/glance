//! Region capture: a transparent selector window collects a rectangle, then the
//! region is grabbed into an in-memory PNG. Primary path is ScreenCaptureKit
//! (macOS 15.2+); older systems fall back to `screencapture -R`.

use std::io::Cursor;
use std::time::Duration;

use base64::Engine;
use image::{imageops, ImageFormat, RgbaImage};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, LogicalPosition, Manager, Monitor, Runtime, WebviewWindow};

pub const SELECTOR: &str = "selector";
pub const OVERLAY: &str = "overlay";

const OVERLAY_W: f64 = 440.0;
const OVERLAY_H: f64 = 460.0;
const OVERLAY_GAP: f64 = 12.0;
const SCREEN_MARGIN: f64 = 8.0;
const THUMB_EDGE: u32 = 640;
/// Time for the window server to drop the selector before we grab pixels.
const HIDE_SETTLE: Duration = Duration::from_millis(120);

/// A rectangle in points (logical pixels).
#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

pub struct Captured {
    pub png: Vec<u8>,
    pub thumb_png: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Serialize)]
pub struct CapturedEvent {
    pub session_id: String,
    pub thumb_b64: String,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Serialize)]
pub struct PermissionEvent {
    pub granted: bool,
}

/// Hotkey entry point: cover the monitor under the cursor with the selector.
pub fn begin_selection<R: Runtime>(app: &AppHandle<R>) {
    if let Some(overlay) = app.get_webview_window(OVERLAY) {
        let _ = overlay.hide();
    }

    if !permission::has_access() {
        permission::request_access();
        let _ = app.emit_to(OVERLAY, "glance://permission", PermissionEvent { granted: false });
        if let Some(overlay) = app.get_webview_window(OVERLAY) {
            let _ = overlay.center();
            let _ = overlay.show();
            let _ = overlay.set_focus();
        }
        return;
    }

    let Some(selector) = app.get_webview_window(SELECTOR) else {
        eprintln!("[glance] selector window missing");
        return;
    };
    if let Some(monitor) = monitor_at_cursor(app) {
        let _ = selector.set_position(*monitor.position());
        let _ = selector.set_size(*monitor.size());
    }
    let _ = app.emit_to(SELECTOR, "glance://select-start", ());
    let _ = selector.show();
    let _ = selector.set_focus();
}

pub fn cancel_selection<R: Runtime>(app: &AppHandle<R>) {
    if let Some(selector) = app.get_webview_window(SELECTOR) {
        let _ = selector.hide();
    }
}

/// Convert a rect in selector-window coordinates to global screen points.
pub fn to_global<R: Runtime>(selector: &WebviewWindow<R>, rect: Rect) -> Result<Rect, String> {
    let scale = selector.scale_factor().map_err(|e| e.to_string())?;
    let origin = selector
        .outer_position()
        .map_err(|e| e.to_string())?
        .to_logical::<f64>(scale);
    Ok(Rect {
        x: origin.x + rect.x,
        y: origin.y + rect.y,
        ..rect
    })
}

/// Hide the selector, wait for it to leave the screen, then grab `global`.
pub async fn grab_after_hide<R: Runtime>(
    selector: &WebviewWindow<R>,
    global: Rect,
) -> Result<Captured, String> {
    selector.hide().map_err(|e| e.to_string())?;
    tauri::async_runtime::spawn_blocking(move || {
        std::thread::sleep(HIDE_SETTLE);
        grab(global)
    })
    .await
    .map_err(|e| e.to_string())?
}

fn grab(rect: Rect) -> Result<Captured, String> {
    let img = grab_rgba(rect)?;
    let (width, height) = img.dimensions();
    let png = encode_png(&img)?;
    let (tw, th) = fit(width, height, THUMB_EDGE);
    let thumb_png = encode_png(&imageops::thumbnail(&img, tw, th))?;
    Ok(Captured {
        png,
        thumb_png,
        width,
        height,
    })
}

#[cfg(target_os = "macos")]
fn grab_rgba(rect: Rect) -> Result<RgbaImage, String> {
    match sck::capture(rect) {
        Ok(img) => Ok(img),
        Err(err) => {
            eprintln!("[glance] ScreenCaptureKit failed ({err}); falling back to screencapture");
            fallback::capture(rect)
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn grab_rgba(_rect: Rect) -> Result<RgbaImage, String> {
    Err("region capture is only implemented on macOS".into())
}

fn encode_png(img: &RgbaImage) -> Result<Vec<u8>, String> {
    let mut out = Cursor::new(Vec::new());
    img.write_to(&mut out, ImageFormat::Png)
        .map_err(|e| format!("png encode: {e}"))?;
    Ok(out.into_inner())
}

fn fit(w: u32, h: u32, max_edge: u32) -> (u32, u32) {
    let long = w.max(h);
    if long <= max_edge {
        return (w.max(1), h.max(1));
    }
    let s = max_edge as f64 / long as f64;
    (((w as f64 * s).round() as u32).max(1), ((h as f64 * s).round() as u32).max(1))
}

pub fn b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

/// Place the overlay beside the selection, clamped to the selection's monitor.
pub fn show_overlay_near<R: Runtime>(app: &AppHandle<R>, selection: Rect) -> Result<(), String> {
    let overlay = app
        .get_webview_window(OVERLAY)
        .ok_or("overlay window missing")?;

    if let Some(bounds) = monitor_bounds_for(app, selection) {
        let pos = overlay_position(selection, bounds);
        overlay
            .set_position(LogicalPosition::new(pos.0, pos.1))
            .map_err(|e| e.to_string())?;
    } else {
        let _ = overlay.center();
    }
    overlay.show().map_err(|e| e.to_string())?;
    overlay.set_focus().map_err(|e| e.to_string())
}

fn overlay_position(sel: Rect, mon: Rect) -> (f64, f64) {
    let (left, top) = (mon.x + SCREEN_MARGIN, mon.y + SCREEN_MARGIN);
    let right = mon.x + mon.width - SCREEN_MARGIN - OVERLAY_W;
    let bottom = mon.y + mon.height - SCREEN_MARGIN - OVERLAY_H;

    let beside_right = sel.x + sel.width + OVERLAY_GAP;
    let beside_left = sel.x - OVERLAY_GAP - OVERLAY_W;
    let x = if beside_right <= right {
        beside_right
    } else if beside_left >= left {
        beside_left
    } else {
        sel.x
    };
    (x.clamp(left, right.max(left)), sel.y.clamp(top, bottom.max(top)))
}

fn monitor_at_cursor<R: Runtime>(app: &AppHandle<R>) -> Option<Monitor> {
    app.cursor_position()
        .ok()
        .and_then(|p| app.monitor_from_point(p.x, p.y).ok().flatten())
        .or_else(|| app.primary_monitor().ok().flatten())
}

fn monitor_bounds_for<R: Runtime>(app: &AppHandle<R>, sel: Rect) -> Option<Rect> {
    let (cx, cy) = (sel.x + sel.width / 2.0, sel.y + sel.height / 2.0);
    let monitors = app.available_monitors().ok()?;
    let bounds = |m: &Monitor| {
        let scale = m.scale_factor();
        let pos = m.position().to_logical::<f64>(scale);
        let size = m.size().to_logical::<f64>(scale);
        Rect {
            x: pos.x,
            y: pos.y,
            width: size.width,
            height: size.height,
        }
    };
    monitors
        .iter()
        .map(bounds)
        .find(|b| cx >= b.x && cx < b.x + b.width && cy >= b.y && cy < b.y + b.height)
        .or_else(|| monitors.first().map(bounds))
}

/// Float a window above everything, including full-screen apps and the menu bar.
#[cfg(target_os = "macos")]
pub fn elevate<R: Runtime>(window: &WebviewWindow<R>, level: isize) {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;

    const CAN_JOIN_ALL_SPACES: usize = 1 << 0;
    const STATIONARY: usize = 1 << 4;
    const IGNORES_CYCLE: usize = 1 << 6;
    const FULL_SCREEN_AUXILIARY: usize = 1 << 8;

    let Ok(ptr) = window.ns_window() else { return };
    let ns_window = ptr.cast::<AnyObject>();
    if ns_window.is_null() {
        return;
    }
    let behavior = CAN_JOIN_ALL_SPACES | STATIONARY | IGNORES_CYCLE | FULL_SCREEN_AUXILIARY;
    unsafe {
        let _: () = msg_send![ns_window, setLevel: level];
        let _: () = msg_send![ns_window, setCollectionBehavior: behavior];
    }
}

#[cfg(target_os = "macos")]
mod sck {
    use super::Rect;
    use image::RgbaImage;
    use screencapturekit::cg::CGRect;
    use screencapturekit::screenshot_manager::{CGImageExt, SCScreenshotManager};

    pub fn capture(rect: Rect) -> Result<RgbaImage, String> {
        let cg_rect = CGRect::new(rect.x, rect.y, rect.width, rect.height);
        let image = SCScreenshotManager::capture_image_in_rect(cg_rect).map_err(|e| e.to_string())?;
        let (w, h) = (image.width() as u32, image.height() as u32);
        let rgba = image.rgba_data().map_err(|e| e.to_string())?;
        RgbaImage::from_raw(w, h, rgba).ok_or_else(|| format!("unexpected buffer size for {w}x{h}"))
    }
}

/// Fallback for macOS < 15.2. `screencapture` can only write to a file, so the
/// capture lives briefly in a private temp dir and is removed immediately.
#[cfg(target_os = "macos")]
mod fallback {
    use super::Rect;
    use image::RgbaImage;
    use std::fs;
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    use std::process::Command;

    pub fn capture(rect: Rect) -> Result<RgbaImage, String> {
        let dir = std::env::temp_dir().join(format!("glance-{}", std::process::id()));
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&dir)
            .map_err(|e| e.to_string())?;
        let _ = fs::set_permissions(&dir, fs::Permissions::from_mode(0o700));
        let path = dir.join("capture.png");

        let region = format!(
            "-R{},{},{},{}",
            rect.x.round(),
            rect.y.round(),
            rect.width.round(),
            rect.height.round()
        );
        let status = Command::new("/usr/sbin/screencapture")
            .args(["-x", "-t", "png", &region])
            .arg(&path)
            .status()
            .map_err(|e| format!("screencapture: {e}"));
        let bytes = fs::read(&path);
        let _ = fs::remove_file(&path);
        let _ = fs::remove_dir(&dir);

        if !status?.success() {
            return Err("screencapture exited with an error".into());
        }
        let bytes = bytes.map_err(|e| format!("screencapture produced no image: {e}"))?;
        image::load_from_memory(&bytes)
            .map(|img| img.to_rgba8())
            .map_err(|e| e.to_string())
    }
}

pub mod permission {
    #[cfg(target_os = "macos")]
    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGPreflightScreenCaptureAccess() -> bool;
        fn CGRequestScreenCaptureAccess() -> bool;
    }

    pub fn has_access() -> bool {
        #[cfg(target_os = "macos")]
        unsafe {
            CGPreflightScreenCaptureAccess()
        }
        #[cfg(not(target_os = "macos"))]
        true
    }

    /// Shows the system prompt the first time; afterwards the user must toggle
    /// it in System Settings and relaunch.
    pub fn request_access() -> bool {
        #[cfg(target_os = "macos")]
        unsafe {
            CGRequestScreenCaptureAccess()
        }
        #[cfg(not(target_os = "macos"))]
        true
    }

    pub fn open_settings() {
        #[cfg(target_os = "macos")]
        let _ = std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture")
            .spawn();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MON: Rect = Rect { x: 0.0, y: 0.0, width: 1440.0, height: 900.0 };

    #[test]
    fn fit_keeps_small_images() {
        assert_eq!(fit(300, 200, 640), (300, 200));
    }

    #[test]
    fn fit_scales_long_edge() {
        assert_eq!(fit(2560, 1600, 640), (640, 400));
        assert_eq!(fit(100, 3000, 640), (21, 640));
    }

    #[test]
    fn overlay_goes_right_when_room() {
        let sel = Rect { x: 100.0, y: 100.0, width: 300.0, height: 200.0 };
        assert_eq!(overlay_position(sel, MON), (412.0, 100.0));
    }

    #[test]
    fn overlay_flips_left_near_right_edge() {
        let sel = Rect { x: 1000.0, y: 100.0, width: 300.0, height: 200.0 };
        assert_eq!(overlay_position(sel, MON), (1000.0 - 12.0 - 440.0, 100.0));
    }

    #[test]
    fn overlay_clamped_inside_screen() {
        let sel = Rect { x: 50.0, y: 800.0, width: 1300.0, height: 90.0 };
        let (x, y) = overlay_position(sel, MON);
        assert!(x >= 8.0 && x + 440.0 <= 1432.0);
        assert_eq!(y, 900.0 - 8.0 - 460.0);
    }
}

/// Real capture against the screen. Needs Screen Recording permission for the
/// terminal running the tests: `cargo test -- --ignored live_capture`.
#[cfg(all(test, target_os = "macos"))]
mod live {
    use super::*;

    #[test]
    #[ignore]
    fn live_capture_region_in_memory() {
        assert!(permission::has_access(), "grant Screen Recording to this terminal");
        let rect = Rect { x: 0.0, y: 0.0, width: 400.0, height: 300.0 };
        let started = std::time::Instant::now();
        let captured = grab(rect).expect("capture");
        eprintln!(
            "captured {}x{} -> {} KB png in {:?}",
            captured.width,
            captured.height,
            captured.png.len() / 1024,
            started.elapsed()
        );
        assert!(captured.width >= 400 && captured.height >= 300);
        assert_eq!(&captured.png[..8], b"\x89PNG\r\n\x1a\n");
    }
}
