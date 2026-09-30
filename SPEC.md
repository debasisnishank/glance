# Glance — Screen → LLM → Actions

Working name: **Glance**. Rename freely.

Press a hotkey, select a screen region, get context-aware action chips (Explain, Debug, Extract, Translate, Ask), click one or type a question, get a streamed answer in a floating overlay. Follow-ups keep the same image and thread.

---

## 1. Goals

- Hotkey to answer on screen in under 2s for action chips.
- Zero-friction: no window switching, no copy-paste.
- Context-typed actions based on what's captured (code, error, form, chart, foreign text, UI).
- Local-first privacy: redact before upload, no image persistence by default.

## Non-goals (v1)

- Autonomous clicking/typing (computer use). Deferred to v3 behind confirm gates.
- Windows/Linux. macOS first, architecture must not block them.
- Accounts, sync, cloud backend.

---

## 2. Stack

| Layer | Choice | Reason |
|---|---|---|
| Shell | Tauri v2 (Rust) | Small binary, low RAM, native APIs |
| Frontend | React + TypeScript + Vite | Overlay UI |
| Styling | Tailwind, Inter (UI) + Fraunces (display) | Default font pairing |
| Capture | ScreenCaptureKit via Rust (`screencapturekit` crate), fallback `screencapture -i` | Region select, permission handled |
| Hotkey | `tauri-plugin-global-shortcut` | Default `Cmd+Shift+Space` |
| OCR | Apple Vision (`VNRecognizeTextRequest`) via Swift bridge or `objc2` | Offline text for redaction + cheap paths |
| LLM | Anthropic Messages API, streaming | Vision + JSON |
| Secrets | macOS Keychain (`keyring` crate) | API key storage |
| Config | `~/Library/Application Support/Glance/config.toml` | |

### Models

| Pass | Model | Purpose |
|---|---|---|
| Classify + suggest | `claude-haiku-4-5-20251001` | Fast action chips |
| Execute action / Q&A | `claude-sonnet-5-5` | Quality answer, streamed |

Both configurable. Provider layer must be pluggable (OpenAI, Gemini, local Ollama vision later).

---

## 3. Architecture

```
┌──────────────┐   hotkey    ┌──────────────┐  PNG   ┌──────────────┐
│ Global       │ ──────────▶ │ Capture      │ ─────▶ │ Preprocess   │
│ Shortcut     │             │ (region)     │        │ resize+redact│
└──────────────┘             └──────────────┘        └──────┬───────┘
                                                            │ b64
                                  ┌─────────────────────────▼──────┐
                                  │ Provider (Rust, reqwest)       │
                                  │  pass 1: classify → JSON       │
                                  │  pass 2: execute → SSE stream  │
                                  └─────────────┬──────────────────┘
                                                │ tauri events
                                  ┌─────────────▼──────────────────┐
                                  │ Overlay (React)                │
                                  │ chips · input · stream · copy  │
                                  └────────────────────────────────┘
```

All network calls happen in Rust. Frontend never sees the API key.

### Rust modules

```
src-tauri/src/
  main.rs            // app setup, plugins, tray
  hotkey.rs          // register/unregister shortcuts
  capture.rs         // region capture → Vec<u8> PNG
  preprocess.rs      // downscale, redact
  ocr.rs             // Apple Vision bridge
  provider/
    mod.rs           // trait Provider
    anthropic.rs     // classify(), stream_answer()
  session.rs         // in-memory thread: image + messages
  config.rs          // load/save toml, keychain
  commands.rs        // #[tauri::command] surface
```

### Frontend

```
src/
  App.tsx
  overlay/Overlay.tsx      // container, positioned near cursor
  overlay/ActionChips.tsx
  overlay/AskInput.tsx
  overlay/StreamView.tsx   // markdown + code highlighting
  settings/Settings.tsx
  lib/ipc.ts               // typed invoke/listen wrappers
  styles/fonts.css         // Inter + Fraunces
```

---

## 4. Contracts

### Provider trait

```rust
#[async_trait]
pub trait Provider: Send + Sync {
    async fn classify(&self, img_b64: &str) -> Result<Classification>;
    async fn stream_answer(
        &self,
        img_b64: &str,
        history: &[Message],
        prompt: &str,
        tx: mpsc::Sender<StreamEvent>,
    ) -> Result<()>;
}
```

### Classification JSON (pass 1)

System prompt:

```
You analyze a screenshot. Return ONLY JSON, no prose, no fences:
{"kind":"code|error|form|chart|text|ui|image|other",
 "summary":"<=20 words",
 "actions":[{"id":"snake_case","label":"<=3 words","prompt":"full instruction"}]}
Max 5 actions, most useful first.
```

Parse defensively: strip ``` fences, fall back to default chips on parse failure.

### Default chips per kind

| kind | chips |
|---|---|
| code | Explain · Find bugs · Refactor · Write tests |
| error | Explain error · Fix steps · Search query |
| form | Draft answers · Summarize |
| chart | Key takeaways · Extract data (CSV) |
| text | Summarize · Translate · Extract text |
| ui | Critique · How do I… |
| other | Describe · Ask |

### Tauri events

| Event | Payload |
|---|---|
| `glance://captured` | `{ session_id, thumb_b64 }` |
| `glance://classified` | `Classification` |
| `glance://token` | `{ session_id, delta }` |
| `glance://done` | `{ session_id, usage }` |
| `glance://error` | `{ session_id, message }` |

### Commands

```
capture_region() -> session_id
run_action(session_id, action_id)
ask(session_id, question)
copy_last(session_id)
close_session(session_id)
get_config() / set_config(cfg)
set_api_key(provider, key)
```

---

## 5. Preprocessing

- Downscale long edge to 1568px max, keep aspect.
- PNG for UI/code, JPEG q85 for photos (pick by colour-count heuristic).
- Redaction (on by default): OCR → regex match → fill black boxes before encode.
  - Emails, card numbers (Luhn-checked), `sk-`/`ghp_`/`AKIA` style tokens, JWTs, Aadhaar/PAN patterns, IBAN.
- Never write captures to disk unless `debug.save_captures = true`.

---

## 6. Overlay UX

- Frameless, transparent, always-on-top, positioned next to selection, clamped to screen.
- States: `capturing → classifying (skeleton chips) → ready → streaming → done`.
- Keys: `1-5` trigger chips, `/` focus ask input, `Cmd+C` copy answer, `Esc` close, `Cmd+Enter` follow-up.
- Answer renders markdown with code highlighting and per-block copy buttons.
- Display headings in Fraunces, everything else Inter.
- Menu-bar tray icon: Settings, Recent (session-only), Quit.

---

## 7. Config

```toml
[hotkey]
capture = "CmdOrCtrl+Shift+Space"

[models]
provider = "anthropic"
classify = "claude-haiku-4-5-20251001"
answer = "claude-sonnet-5-5"
max_tokens = 1024

[privacy]
redact = true
save_captures = false

[ui]
theme = "system"
```

---

## 8. Milestones

### v0.1 — Skeleton
- [ ] Tauri v2 + React/TS/Vite scaffold, Tailwind, fonts wired
- [ ] Global hotkey registers, opens empty overlay
- [ ] Region capture to PNG in memory, thumbnail in overlay
- [ ] Screen Recording permission check + prompt

### v0.2 — LLM loop
- [ ] Keychain API key storage + settings UI
- [ ] `anthropic.rs` classify (non-streaming) → chips
- [ ] `stream_answer` via SSE → `glance://token` events
- [ ] Session thread with follow-ups
- [ ] Downscale in `preprocess.rs`

### v0.3 — Polish
- [ ] Apple Vision OCR bridge
- [ ] Redaction pipeline
- [ ] Keyboard shortcuts, copy buttons, markdown render
- [ ] Error states, retries, timeout (30s)
- [ ] Token/cost display per session

### v1.0 — Ship
- [ ] Code signing + notarization
- [ ] Auto-update (`tauri-plugin-updater`)
- [ ] Onboarding flow (permission, key, hotkey test)
- [ ] Second provider (OpenAI or Ollama) behind trait

### v2+ — Later
- Clipboard-image trigger, drag-drop image onto tray
- Custom user actions (saved prompts)
- Windows support (Windows.Graphics.Capture)
- Computer use: propose click/type steps, each gated by explicit confirm

---

## 9. Acceptance criteria (v1)

- Hotkey to chips visible: p50 < 2.0s on a normal connection.
- First answer token after chip click: p50 < 1.5s.
- Idle RAM < 80MB, binary < 20MB.
- Zero captures on disk with default config.
- Redaction catches the test fixture set (see `tests/fixtures/redact/`).
- API key never present in frontend bundle, logs, or IPC payloads.

---

## 10. Claude Code kickoff

```bash
mkdir glance && cd glance
cp /path/to/SPEC.md .
claude
```

First prompt:

```
Read SPEC.md. Scaffold v0.1 exactly as specified: Tauri v2, React+TS+Vite,
Tailwind, Inter + Fraunces. Implement hotkey.rs and capture.rs with a working
region capture on macOS. Stop after v0.1 checklist is green and list what you
verified.
```
