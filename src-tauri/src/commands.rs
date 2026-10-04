//! `#[tauri::command]` surface. Payloads never carry API keys.

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tokio::sync::mpsc;

use crate::capture::{self, permission, CapturedEvent, Rect, UploadInfo, OVERLAY, SELECTOR};
use crate::config::{self, Config, ConfigState, Secrets};
use crate::hotkey;
use crate::provider::cli::CliStatus;
use crate::provider::{self, default_actions, Classification, Message, ProviderError, StreamEvent, Usage};
use crate::session::{Session, Sessions, Totals};

pub const SETTINGS: &str = "settings";

#[derive(Clone, Serialize)]
struct ClassifiedEvent<'a> {
    session_id: &'a str,
    #[serde(flatten)]
    classification: &'a Classification,
}

#[derive(Clone, Serialize)]
struct TokenEvent<'a> {
    session_id: &'a str,
    delta: &'a str,
}

#[derive(Clone, Serialize)]
struct DoneEvent<'a> {
    session_id: &'a str,
    usage: &'a Usage,
    totals: &'a Totals,
    stop_reason: Option<&'a str>,
}

#[derive(Clone, Serialize)]
struct ErrorEvent<'a> {
    session_id: Option<&'a str>,
    code: &'a str,
    message: &'a str,
}

fn emit_error(app: &AppHandle, session_id: Option<&str>, err: &ProviderError) {
    let _ = app.emit_to(
        OVERLAY,
        "glance://error",
        ErrorEvent {
            session_id,
            code: err.code,
            message: &err.message,
        },
    );
}

/// Open the region selector (same as pressing the hotkey).
#[tauri::command]
pub fn start_capture(app: AppHandle) {
    capture::begin_selection(&app);
}

/// Called by the selector with the dragged rect in selector-window points.
/// Captures into memory, opens the overlay, then classifies in the background.
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
            emit_error(&app, None, &ProviderError::new("capture", message.clone()));
            let _ = capture::show_overlay_near(&app, global);
            return Err(message);
        }
    };

    // One overlay, one live session: drop (and abort) anything older.
    sessions.clear();
    let thumb_b64 = capture::b64(&captured.thumb_png);
    let upload = UploadInfo {
        media_type: captured.prepared.media_type,
        width: captured.prepared.width,
        height: captured.prepared.height,
    };
    let session_id = sessions.insert(Session::new(captured.prepared, global));
    app.emit_to(
        OVERLAY,
        "glance://captured",
        CapturedEvent {
            session_id: session_id.clone(),
            thumb_b64,
            width: captured.width,
            height: captured.height,
            upload,
        },
    )
    .map_err(|e| e.to_string())?;
    capture::show_overlay_near(&app, global)?;

    spawn_classify(app.clone(), session_id.clone());
    Ok(session_id)
}

fn spawn_classify(app: AppHandle, session_id: String) {
    tauri::async_runtime::spawn(async move {
        let sessions = app.state::<Sessions>();
        let Some(image) = sessions.with(&session_id, |s| s.image.clone()) else {
            return;
        };
        let cfg = app.state::<ConfigState>().get();
        let result = match provider::build(&cfg, &app.state::<Secrets>()) {
            Ok(p) => p.classify(&image).await,
            Err(e) => Err(e),
        };
        let classification = match result {
            Ok(c) => c,
            // Missing or bad key: tell the user, and still offer default
            // chips so they work as soon as a key is saved.
            Err(e) if matches!(e.code, "no_api_key" | "no_cli" | "setup" | "auth") => {
                emit_error(&app, Some(&session_id), &e);
                provider::fallback_classification()
            }
            // Anything else degrades to default chips (SPEC §4).
            Err(e) => {
                eprintln!("[glance] classify failed: {e}");
                provider::fallback_classification()
            }
        };
        let stored = sessions.with(&session_id, |s| s.classification = Some(classification.clone()));
        if stored.is_some() {
            let _ = app.emit_to(
                OVERLAY,
                "glance://classified",
                ClassifiedEvent {
                    session_id: &session_id,
                    classification: &classification,
                },
            );
        }
    });
}

/// Run one of the session's action chips.
#[tauri::command]
pub fn run_action(
    app: AppHandle,
    sessions: State<'_, Sessions>,
    session_id: String,
    action_id: String,
) -> Result<(), String> {
    let prompt = sessions
        .with(&session_id, |s| {
            let actions = match &s.classification {
                Some(c) => c.actions.clone(),
                None => default_actions("other"),
            };
            actions.into_iter().find(|a| a.id == action_id).map(|a| a.prompt)
        })
        .ok_or("session not found")?
        .ok_or_else(|| format!("unknown action {action_id:?}"))?;
    start_turn(app, &sessions, session_id, prompt)
}

/// Ask a free-form question (first turn or follow-up) about the capture.
#[tauri::command]
pub fn ask(
    app: AppHandle,
    sessions: State<'_, Sessions>,
    session_id: String,
    question: String,
) -> Result<(), String> {
    let question = question.trim().to_string();
    if question.is_empty() {
        return Err("question is empty".into());
    }
    start_turn(app, &sessions, session_id, question)
}

fn start_turn(app: AppHandle, sessions: &Sessions, session_id: String, prompt: String) -> Result<(), String> {
    let cfg = app.state::<ConfigState>().get();
    let provider = match provider::build(&cfg, &app.state::<Secrets>()) {
        Ok(p) => p,
        Err(e) => {
            emit_error(&app, Some(&session_id), &e);
            return Err(e.message);
        }
    };

    let (image, history) = sessions
        .with(&session_id, |s| {
            if s.busy() {
                Err("an answer is already streaming".to_string())
            } else {
                Ok((s.image.clone(), s.history.clone()))
            }
        })
        .ok_or("session not found")??;

    let task_app = app.clone();
    let task_id = session_id.clone();
    let task = tauri::async_runtime::spawn(async move {
        let (tx, mut rx) = mpsc::channel::<StreamEvent>(128);
        let forward_app = task_app.clone();
        let forward_id = task_id.clone();
        let forward = tauri::async_runtime::spawn(async move {
            while let Some(StreamEvent::Delta(delta)) = rx.recv().await {
                let _ = forward_app.emit_to(
                    OVERLAY,
                    "glance://token",
                    TokenEvent {
                        session_id: &forward_id,
                        delta: &delta,
                    },
                );
            }
        });

        let result = provider.stream_answer(&image, &history, &prompt, tx).await;
        let _ = forward.await;

        let sessions = task_app.state::<Sessions>();
        match result {
            Ok(answer) => {
                let totals = sessions.with(&task_id, |s| {
                    // A turn with nothing replayable would leave two user
                    // messages in a row; skip it rather than corrupt the thread.
                    if !answer.blocks.is_empty() {
                        s.history.push(Message::User(prompt));
                        s.history.push(Message::Assistant(answer.blocks.clone()));
                    }
                    s.last_answer = Some(answer.text.clone());
                    s.totals.add(&answer.usage);
                    s.totals.clone()
                });
                if let Some(totals) = totals {
                    let _ = task_app.emit_to(
                        OVERLAY,
                        "glance://done",
                        DoneEvent {
                            session_id: &task_id,
                            usage: &answer.usage,
                            totals: &totals,
                            stop_reason: answer.stop_reason.as_deref(),
                        },
                    );
                }
            }
            Err(e) if e.code == "cancelled" => {}
            Err(e) => emit_error(&task_app, Some(&task_id), &e),
        }
    });

    sessions.with(&session_id, |s| s.task = Some(task));
    Ok(())
}

/// Copy the latest answer's Markdown to the clipboard.
#[tauri::command]
pub fn copy_last(app: AppHandle, sessions: State<'_, Sessions>, session_id: String) -> Result<bool, String> {
    let Some(Some(text)) = sessions.with(&session_id, |s| s.last_answer.clone()) else {
        return Ok(false);
    };
    app.clipboard().write_text(text).map_err(|e| e.to_string())?;
    Ok(true)
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

/// Minimize the overlay to a draggable header bar, or expand it again.
#[tauri::command]
pub fn set_overlay_collapsed(app: AppHandle, collapsed: bool) -> Result<(), String> {
    capture::set_overlay_collapsed(&app, collapsed)
}

#[tauri::command]
pub fn get_config(config: State<'_, ConfigState>) -> Config {
    config.get()
}

#[tauri::command]
pub fn set_config(app: AppHandle, config: State<'_, ConfigState>, cfg: Config) -> Result<Config, String> {
    cfg.validate()?;
    let old = config.get();
    if cfg.hotkey.capture != old.hotkey.capture {
        hotkey::unregister_all(&app)?;
        if let Err(err) = hotkey::register(&app, &cfg.hotkey.capture) {
            let _ = hotkey::register(&app, &old.hotkey.capture);
            return Err(err);
        }
    }
    config::save(&cfg)?;
    config.set(cfg.clone());
    Ok(cfg)
}

#[tauri::command]
pub fn set_api_key(secrets: State<'_, Secrets>, provider: String, key: String) -> Result<(), String> {
    secrets.set(&provider, &key)
}

#[tauri::command]
pub fn clear_api_key(secrets: State<'_, Secrets>, provider: String) -> Result<(), String> {
    secrets.clear(&provider)
}

#[tauri::command]
pub fn has_api_key(secrets: State<'_, Secrets>, provider: String) -> bool {
    secrets.get(&provider).is_some()
}

/// Install and sign-in state of the subscription CLIs, for Settings.
#[tauri::command]
pub async fn cli_status(config: State<'_, ConfigState>) -> Result<Vec<CliStatus>, String> {
    let cfg = config.get();
    let (claude, codex) = tokio::join!(
        provider::claude_code::status(&cfg.claude_code.path),
        provider::codex::status(&cfg.codex.path),
    );
    Ok(vec![claude, codex])
}

/// Test a custom endpoint by listing its models, using the saved custom key.
#[tauri::command]
pub async fn list_custom_models(
    secrets: State<'_, Secrets>,
    format: String,
    base_url: String,
) -> Result<Vec<String>, String> {
    let key = secrets.get("custom");
    provider::list_custom_models(&format, &base_url, key)
        .await
        .map_err(|e| e.message)
}

#[tauri::command]
pub fn open_settings(app: AppHandle) {
    show_settings(&app);
}

pub fn show_settings(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(SETTINGS) {
        let _ = w.center();
        let _ = w.show();
        let _ = w.set_focus();
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
