# Glance

Screen → LLM → Actions. Press a hotkey, drag a region, get context-aware actions.
See [SPEC.md](SPEC.md) for the full design.

## Status: v0.2 — LLM loop

- `⌘⇧Space` → drag a region → capture held in memory (ScreenCaptureKit, macOS 15.2+;
  `screencapture -R` fallback)
- Downscaled to 1568px long edge; PNG for UI/text, JPEG q85 for photo-like captures
- **Pass 1:** `claude-haiku-4-5-20251001` classifies the capture into action chips
  (parsed defensively, falls back to per-kind default chips)
- **Pass 2:** `claude-sonnet-5-5` streams the answer over SSE into the overlay (Markdown);
  follow-ups reuse the same image and thread, with prompt caching
- Refusal fallback on by default (`fallbacks: "default"`), toggle in Settings
- API key in the macOS Keychain; never sent to the frontend, config, or logs

### Connections

Pick one in Settings → Connect:

| Connection | Who it's for | How |
|---|---|---|
| Claude subscription | Claude Pro / Max | Drives your signed-in `claude` CLI (Claude Code) with `--safe-mode`, no tools; image sent over stdin |
| ChatGPT subscription | ChatGPT Plus / Pro | Drives your signed-in `codex exec` read-only; image via a 0600 temp file deleted after each call |
| Anthropic API key | Pay per use | Direct Messages API calls from Rust |
| Custom endpoint | Local or self-hosted models, other clouds | Any OpenAI-compatible `/chat/completions` (Ollama, LM Studio, OpenRouter, OpenAI, Groq, vLLM) or Anthropic-compatible `/v1/messages` (LiteLLM, gateways); optional key in Keychain |

Glance never reads the CLIs' credentials; it runs them as you would in Terminal, so usage counts
against your plan's limits.
- Settings window (tray → Settings…, or ⚙ in the overlay): key, models, effort, max tokens, hotkey
- Overlay keys: `1–5` chips · `/` ask · `Enter` send · `⌘C` copy answer · `Esc` close

Config lives at `~/Library/Application Support/Glance/config.toml`.

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
ANTHROPIC_API_KEY=sk-ant-... cargo test -- --ignored live_anthropic --nocapture   # real API, < 1 cent
cargo test -- --ignored live_claude_code --nocapture   # via your signed-in Claude Code
GLANCE_TEST_BASE=http://localhost:11434/v1 GLANCE_TEST_MODEL=llava \
  cargo test -- --ignored live_openai_compat --nocapture  # any OpenAI-compatible server
```

## Layout

```
src-tauri/src/
  lib.rs        app setup, windows, tray (the spec's main.rs role)
  hotkey.rs     global shortcut registration
  capture.rs    selector → global rect → ScreenCaptureKit → PNG; permission; overlay placement
  preprocess.rs downscale + PNG/JPEG choice
  config.rs     config.toml + Keychain secrets
  provider/     Provider trait; anthropic.rs (API, SSE), claude_code.rs + codex.rs
                (subscription CLIs), cli.rs (discovery, process plumbing),
                openai_compat.rs (custom endpoints)
  session.rs    in-memory sessions: image + thread
  commands.rs   #[tauri::command] surface, classify/answer tasks
src/
  selector/Selector.tsx   drag-to-select layer
  overlay/                floating card: chips, ask input, streamed answer
  settings/Settings.tsx   key, models, hotkey
  lib/ipc.ts              typed invoke/listen wrappers
```
