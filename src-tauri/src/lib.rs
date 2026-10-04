//! App setup: state, windows, plugins, tray.

mod capture;
mod commands;
mod config;
mod hotkey;
mod preprocess;
mod provider;
mod session;

use std::sync::RwLock;

use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::TrayIconBuilder;
use tauri::{Manager, RunEvent, WindowEvent};

use config::{ConfigState, Secrets};
use session::Sessions;

/// NSScreenSaverWindowLevel: above the menu bar and full-screen apps.
#[cfg(target_os = "macos")]
const SELECTOR_LEVEL: isize = 1000;
/// NSPopUpMenuWindowLevel.
#[cfg(target_os = "macos")]
const OVERLAY_LEVEL: isize = 101;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let cfg = config::load();
    let hotkey_accel = cfg.hotkey.capture.clone();

    let app = tauri::Builder::default()
        .plugin(hotkey::plugin())
        .plugin(tauri_plugin_clipboard_manager::init())
        .manage(Sessions::default())
        .manage(ConfigState(RwLock::new(cfg)))
        .manage(Secrets::default())
        .invoke_handler(tauri::generate_handler![
            commands::start_capture,
            commands::capture_region,
            commands::run_action,
            commands::ask,
            commands::copy_last,
            commands::cancel_selection,
            commands::close_session,
            commands::set_overlay_collapsed,
            commands::get_config,
            commands::set_config,
            commands::set_api_key,
            commands::clear_api_key,
            commands::has_api_key,
            commands::cli_status,
            commands::list_custom_models,
            commands::open_settings,
            commands::screen_permission,
            commands::request_screen_permission,
            commands::open_screen_settings,
        ])
        .on_window_event(|window, event| {
            // Settings is reopened from the tray; hide instead of destroying it.
            if let WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == commands::SETTINGS {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .setup(move |app| {
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

            if let Err(err) = hotkey::register(app.handle(), &hotkey_accel) {
                eprintln!("[glance] {err}");
            }

            let capture_item = MenuItemBuilder::with_id("capture", "Capture Region")
                .accelerator(&hotkey_accel)
                .build(app)?;
            let settings_item = MenuItemBuilder::with_id("settings", "Settings…")
                .accelerator("CmdOrCtrl+,")
                .build(app)?;
            let quit_item = MenuItemBuilder::with_id("quit", "Quit Glance").build(app)?;
            let menu = MenuBuilder::new(app)
                .items(&[&capture_item, &settings_item])
                .separator()
                .items(&[&quit_item])
                .build()?;

            let mut tray = TrayIconBuilder::with_id("glance")
                .tooltip("Glance")
                .menu(&menu)
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "capture" => capture::begin_selection(app),
                    "settings" => commands::show_settings(app),
                    "quit" => app.exit(0),
                    _ => {}
                });
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            tray.build(app)?;

            // First run: nothing works without a key, so ask for one up front.
            let provider = app.state::<ConfigState>().get().models.provider;
            if provider == "anthropic" && app.state::<Secrets>().get("anthropic").is_none() {
                commands::show_settings(app.handle());
            }
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
