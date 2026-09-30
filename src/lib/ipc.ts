// Typed wrappers over Tauri invoke/listen. The frontend never handles API keys.
import { invoke } from "@tauri-apps/api/core";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";

export type Rect = { x: number; y: number; width: number; height: number };

export type CapturedEvent = {
  session_id: string;
  thumb_b64: string;
  width: number;
  height: number;
};

export type ErrorEvent = { session_id: string | null; message: string };

export type PermissionEvent = { granted: boolean };

type Events = {
  "glance://select-start": null;
  "glance://captured": CapturedEvent;
  "glance://error": ErrorEvent;
  "glance://permission": PermissionEvent;
};

export const ipc = {
  startCapture: () => invoke<void>("start_capture"),
  captureRegion: (rect: Rect) => invoke<string>("capture_region", { rect }),
  cancelSelection: () => invoke<void>("cancel_selection"),
  closeSession: (sessionId: string | null) =>
    invoke<void>("close_session", { sessionId }),
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
