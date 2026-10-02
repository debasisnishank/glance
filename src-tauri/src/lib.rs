//! App setup: windows, plugins, tray.

mod capture;
mod commands;
mod hotkey;
mod session;

use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::TrayIconBuilder;
use tauri::{Manager, RunEvent};

use session::Sessions;

/// NSScreenSaverWindowLevel: above the menu bar and full-screen apps.
#[cfg(target_os = "macos")]
const SELECTOR_LEVEL: isize = 1000;
/// NSPopUpMenuWindowLevel.
#[cfg(target_os = "macos")]
const OVERLAY_LEVEL: isize = 101;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(hotkey::plugin())
        .manage(Sessions::default())
        .invoke_handler(tauri::generate_handler![
            commands::start_capture,
            commands::capture_region,
            commands::cancel_selection,
            commands::close_session,
            commands::set_overlay_collapsed,
            commands::screen_permission,
            commands::request_screen_permission,
            commands::open_screen_settings,
        ])
        .setup(|app| {
            #[cfg(target_os = "macos")]
            {
                // Menu-bar app: no Dock icon, no app switcher entry.
                app.set_activation_policy(tauri::ActivationPolicy::Accessory);
                if let Some(w) = app.get_webview_window(capture::SELECTOR) {
                    capture::elevate(&w, SELECTOR_LEVEL);
                }
                if let Some(w) = app.get_webview_window(capture::OVERLAY) {
                    capture::elevate(&w, OVERLAY_LEVEL);
                }
            }

            if let Err(err) = hotkey::register(app.handle(), hotkey::DEFAULT_CAPTURE) {
                eprintln!("[glance] {err}");
            }

            let capture_item = MenuItemBuilder::with_id("capture", "Capture Region")
                .accelerator(hotkey::DEFAULT_CAPTURE)
                .build(app)?;
            let quit_item = MenuItemBuilder::with_id("quit", "Quit Glance").build(app)?;
            let menu = MenuBuilder::new(app)
                .items(&[&capture_item])
                .separator()
                .items(&[&quit_item])
                .build()?;

            let mut tray = TrayIconBuilder::with_id("glance")
                .tooltip("Glance")
                .menu(&menu)
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "capture" => capture::begin_selection(app),
                    "quit" => app.exit(0),
                    _ => {}
                });
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            tray.build(app)?;
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building glance");

    app.run(|handle, event| match event {
        // Windows are hidden, never closed; keep running in the menu bar.
        RunEvent::ExitRequested { code: None, api, .. } => api.prevent_exit(),
        RunEvent::Exit => {
            let _ = hotkey::unregister_all(handle);
            handle.state::<Sessions>().clear();
        }
        _ => {}
    });
}
