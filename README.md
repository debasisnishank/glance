# Glance

Screen → LLM → Actions. Press a hotkey, drag a region, get context-aware actions.
See [SPEC.md](SPEC.md) for the full design.

## Status: v0.1 skeleton

- Tauri v2 + React/TS/Vite, Tailwind v4, Inter + Fraunces (bundled locally)
- `⌘⇧Space` global hotkey opens a full-screen region selector on the monitor under the cursor
- Region captured via ScreenCaptureKit (`SCScreenshotManager.captureImageInRect`, macOS 15.2+)
  into an in-memory PNG; falls back to `screencapture -R` on older macOS
- Overlay opens beside the selection with a thumbnail; `Esc` closes and drops the session
- Screen Recording permission check, system prompt, and a settings deep-link
- Menu-bar tray (Capture Region, Quit); no Dock icon

## Develop

```bash
npm install
npm run tauri dev
```

Requires Rust, Node, and Xcode or the Command Line Tools (the ScreenCaptureKit bridge is Swift).
In dev, Screen Recording permission is granted to your terminal app, not Glance.

```bash
cd src-tauri
cargo test                                   # unit tests
cargo test -- --ignored live_capture         # real screen capture (needs permission)
```

## Layout

```
src-tauri/src/
  lib.rs        app setup, windows, tray (the spec's main.rs role)
  hotkey.rs     global shortcut registration
  capture.rs    selector → global rect → ScreenCaptureKit → PNG; permission; overlay placement
  session.rs    in-memory sessions
  commands.rs   #[tauri::command] surface
src/
  selector/Selector.tsx   drag-to-select layer
  overlay/Overlay.tsx     floating result card
  lib/ipc.ts              typed invoke/listen wrappers
```
