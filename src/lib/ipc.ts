// Typed wrappers over Tauri invoke/listen. API keys go in, never come out.
import { invoke } from "@tauri-apps/api/core";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";

export type Rect = { x: number; y: number; width: number; height: number };

export type CapturedEvent = {
  session_id: string;
  thumb_b64: string;
  width: number;
  height: number;
  upload: { media_type: string; width: number; height: number };
};

export type Action = { id: string; label: string; prompt: string };

export type Classification = {
  kind: string;
  summary: string;
  actions: Action[];
};

export type ClassifiedEvent = Classification & { session_id: string };

export type TokenEvent = { session_id: string; delta: string };

export type Usage = {
  model: string;
  input_tokens: number;
  output_tokens: number;
  cache_read_input_tokens: number;
  cache_creation_input_tokens: number;
};

export type Totals = Omit<Usage, "model">;

export type DoneEvent = {
  session_id: string;
  usage: Usage;
  totals: Totals;
  stop_reason: string | null;
};

export type ErrorCode =
  | "no_api_key"
  | "no_cli"
  | "setup"
  | "auth"
  | "rate_limited"
  | "overloaded"
  | "refusal"
  | "bad_request"
  | "network"
  | "timeout"
  | "api"
  | "parse"
  | "capture";

export type ErrorEvent = {
  session_id: string | null;
  code: ErrorCode;
  message: string;
};

export type PermissionEvent = { granted: boolean };

export type ProviderId = "anthropic" | "claude_code" | "codex" | "custom";

export type CliStatus = {
  provider: "claude_code" | "codex";
  found: boolean;
  path: string | null;
  version: string | null;
  logged_in: boolean | null;
  detail: string | null;
};

export type Config = {
  hotkey: { capture: string };
  models: {
    provider: ProviderId;
    classify: string;
    answer: string;
    max_tokens: number;
    effort: "low" | "medium" | "high" | "xhigh" | "max";
    fallbacks: boolean;
  };
  claude_code: { path: string; classify: string; answer: string; effort: string };
  codex: { path: string; model: string; effort: string };
  custom: {
    format: "openai" | "anthropic";
    base_url: string;
    classify_model: string;
    answer_model: string;
    max_tokens: number;
  };
  privacy: { redact: boolean; save_captures: boolean };
  ui: { theme: string };
};

type Events = {
  "glance://select-start": null;
  "glance://captured": CapturedEvent;
  "glance://classified": ClassifiedEvent;
  "glance://token": TokenEvent;
  "glance://done": DoneEvent;
  "glance://error": ErrorEvent;
  "glance://permission": PermissionEvent;
};

export const ipc = {
  startCapture: () => invoke<void>("start_capture"),
  captureRegion: (rect: Rect) => invoke<string>("capture_region", { rect }),
  cancelSelection: () => invoke<void>("cancel_selection"),
  runAction: (sessionId: string, actionId: string) =>
    invoke<void>("run_action", { sessionId, actionId }),
  ask: (sessionId: string, question: string) =>
    invoke<void>("ask", { sessionId, question }),
  copyLast: (sessionId: string) => invoke<boolean>("copy_last", { sessionId }),
  closeSession: (sessionId: string | null) =>
    invoke<void>("close_session", { sessionId }),
  setOverlayCollapsed: (collapsed: boolean) =>
    invoke<void>("set_overlay_collapsed", { collapsed }),
  getConfig: () => invoke<Config>("get_config"),
  setConfig: (cfg: Config) => invoke<Config>("set_config", { cfg }),
  setApiKey: (provider: string, key: string) =>
    invoke<void>("set_api_key", { provider, key }),
  clearApiKey: (provider: string) => invoke<void>("clear_api_key", { provider }),
  hasApiKey: (provider: string) => invoke<boolean>("has_api_key", { provider }),
  cliStatus: () => invoke<CliStatus[]>("cli_status"),
  listCustomModels: (format: string, baseUrl: string) =>
    invoke<string[]>("list_custom_models", { format, baseUrl }),
  openSettings: () => invoke<void>("open_settings"),
  screenPermission: () => invoke<boolean>("screen_permission"),
  requestScreenPermission: () => invoke<boolean>("request_screen_permission"),
  openScreenSettings: () => invoke<void>("open_screen_settings"),
};

/** Listen for an event targeted at this window. */
export function on<K extends keyof Events>(
  event: K,
  handler: (payload: Events[K]) => void,
): Promise<UnlistenFn> {
  return getCurrentWebviewWindow().listen<Events[K]>(event, (e) =>
    handler(e.payload),
  );
}

export function windowLabel(): string {
  return getCurrentWebviewWindow().label;
}
