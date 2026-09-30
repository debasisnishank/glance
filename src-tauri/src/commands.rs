//! `#[tauri::command]` surface. Keep payloads free of secrets.

use tauri::{AppHandle, Emitter, Manager, State};

use crate::capture::{self, permission, CapturedEvent, Rect, OVERLAY, SELECTOR};
use crate::session::{Session, Sessions};

/// Open the region selector (same as pressing the hotkey).
#[tauri::command]
pub fn start_capture(app: AppHandle) {
    capture::begin_selection(&app);
}

/// Called by the selector with the dragged rect in selector-window points.
/// Captures the region into memory and opens the overlay beside it.
#[tauri::command]
pub async fn capture_region(
    app: AppHandle,
    sessions: State<'_, Sessions>,
    rect: Rect,
) -> Result<String, String> {
    let selector = app
        .get_webview_window(SELECTOR)
        .ok_or("selector window missing")?;
    let global = capture::to_global(&selector, rect)?;

    let captured = match capture::grab_after_hide(&selector, global).await {
        Ok(c) => c,
        Err(message) => {
            let _ = app.emit_to(OVERLAY, "glance://error", serde_json::json!({ "session_id": null, "message": message }));
            let _ = capture::show_overlay_near(&app, global);
            return Err(message);
        }
    };

    let thumb_b64 = capture::b64(&captured.thumb_png);
    let session_id = sessions.insert(Session {
        png: captured.png,
        rect: global,
    });
    app.emit_to(
        OVERLAY,
        "glance://captured",
        CapturedEvent {
            session_id: session_id.clone(),
            thumb_b64,
            width: captured.width,
            height: captured.height,
        },
    )
    .map_err(|e| e.to_string())?;
    capture::show_overlay_near(&app, global)?;
    Ok(session_id)
}

#[tauri::command]
pub fn cancel_selection(app: AppHandle) {
    capture::cancel_selection(&app);
}

#[tauri::command]
pub fn close_session(app: AppHandle, sessions: State<'_, Sessions>, session_id: Option<String>) {
    if let Some(id) = session_id {
        sessions.remove(&id);
    }
    if let Some(overlay) = app.get_webview_window(OVERLAY) {
        let _ = overlay.hide();
    }
}

#[tauri::command]
pub fn screen_permission() -> bool {
    permission::has_access()
}

#[tauri::command]
pub fn request_screen_permission() -> bool {
    permission::request_access()
}

#[tauri::command]
pub fn open_screen_settings() {
    permission::open_settings();
}
