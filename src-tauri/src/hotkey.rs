//! Global shortcut registration. The handler fires the capture flow on key-down.

use tauri::{plugin::TauriPlugin, AppHandle, Runtime};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

pub const DEFAULT_CAPTURE: &str = "CmdOrCtrl+Shift+Space";

pub fn plugin<R: Runtime>() -> TauriPlugin<R> {
    tauri_plugin_global_shortcut::Builder::new()
        .with_handler(|app, _shortcut, event| {
            if event.state() == ShortcutState::Pressed {
                crate::capture::begin_selection(app);
            }
        })
        .build()
}

pub fn register<R: Runtime>(app: &AppHandle<R>, accelerator: &str) -> Result<(), String> {
    let shortcut: Shortcut = accelerator
        .parse()
        .map_err(|e| format!("invalid hotkey {accelerator:?}: {e}"))?;
    let shortcuts = app.global_shortcut();
    if shortcuts.is_registered(shortcut) {
        return Ok(());
    }
    shortcuts
        .register(shortcut)
        .map_err(|e| format!("could not register {accelerator:?}: {e}"))
}

pub fn unregister_all<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    app.global_shortcut().unregister_all().map_err(|e| e.to_string())
}
