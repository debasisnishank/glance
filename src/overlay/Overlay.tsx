import { useCallback, useEffect, useReducer, useRef, useState } from "react";
import {
  ipc,
  on,
  type CapturedEvent,
  type Classification,
  type ErrorEvent,
  type Totals,
} from "../lib/ipc";
import { ActionChips } from "./ActionChips";
import { AskInput } from "./AskInput";
import { StreamView } from "./StreamView";

export type Turn = {
  prompt: string;
  /** Chip turns render the label as a pill; questions render as text. */
  fromChip: boolean;
  answer: string;
  status: "streaming" | "done" | "error";
  error?: string;
  stopReason?: string | null;
};

type SessionState = {
  id: string;
  thumb: string;
  width: number;
  height: number;
  upload: CapturedEvent["upload"];
  /** null while the classifier is running. */
  classification: Classification | null;
  turns: Turn[];
  totals: Totals | null;
  /** Session-level problem (missing key, bad key, capture failure). */
  notice: ErrorEvent | null;
};

type State =
  | { kind: "idle" }
  | { kind: "permission" }
  | { kind: "capture-error"; message: string }
  | ({ kind: "session" } & SessionState);

type Action =
  | { type: "captured"; e: CapturedEvent }
  | { type: "classified"; id: string; c: Classification }
  | { type: "turn"; prompt: string; fromChip: boolean }
  | { type: "token"; id: string; delta: string }
  | { type: "done"; id: string; totals: Totals; stopReason: string | null }
  | { type: "error"; e: ErrorEvent }
  | { type: "permission" }
  | { type: "reset" };

const SESSION_NOTICES = new Set(["no_api_key", "no_cli", "setup", "auth"]);

function updateLastTurn(s: SessionState, f: (t: Turn) => Turn): Turn[] {
  const last = s.turns[s.turns.length - 1];
  if (!last || last.status !== "streaming") return s.turns;
  return [...s.turns.slice(0, -1), f(last)];
}

function reducer(state: State, a: Action): State {
  switch (a.type) {
    case "reset":
      return { kind: "idle" };
    case "permission":
      return { kind: "permission" };
    case "captured":
      return {
        kind: "session",
        id: a.e.session_id,
        thumb: `data:image/png;base64,${a.e.thumb_b64}`,
        width: a.e.width,
        height: a.e.height,
        upload: a.e.upload,
        classification: null,
        turns: [],
        totals: null,
        notice: null,
      };
  }

  if (a.type === "error" && a.e.code === "capture") {
    return { kind: "capture-error", message: a.e.message };
  }
  if (state.kind !== "session") return state;

  switch (a.type) {
    case "classified":
      return a.id === state.id ? { ...state, classification: a.c } : state;
    case "turn":
      return {
        ...state,
        notice: null,
        turns: [
          ...state.turns,
          { prompt: a.prompt, fromChip: a.fromChip, answer: "", status: "streaming" },
        ],
      };
    case "token":
      if (a.id !== state.id) return state;
      return {
        ...state,
        turns: updateLastTurn(state, (t) => ({ ...t, answer: t.answer + a.delta })),
      };
    case "done":
      if (a.id !== state.id) return state;
      return {
        ...state,
        totals: a.totals,
        turns: updateLastTurn(state, (t) => ({
          ...t,
          status: "done",
          stopReason: a.stopReason,
        })),
      };
    case "error": {
      if (a.e.session_id && a.e.session_id !== state.id) return state;
      const streaming = state.turns.at(-1)?.status === "streaming";
      if (SESSION_NOTICES.has(a.e.code)) {
        return {
          ...state,
          notice: a.e,
          turns: streaming ? state.turns.slice(0, -1) : state.turns,
        };
      }
      if (!streaming) return { ...state, notice: a.e };
      return {
        ...state,
        turns: updateLastTurn(state, (t) => ({ ...t, status: "error", error: a.e.message })),
      };
    }
  }
  return state;
}

export function Overlay() {
  const [state, dispatch] = useReducer(reducer, { kind: "idle" } as State);
  const inputRef = useRef<HTMLTextAreaElement>(null);
  const [copied, setCopied] = useState(false);
  const [collapsed, setCollapsed] = useState(false);

  const session = state.kind === "session" ? state : null;
  const busy = session?.turns.at(-1)?.status === "streaming";
  const sessionId = session?.id ?? null;

  const toggleCollapsed = useCallback(() => {
    setCollapsed((c) => {
      void ipc.setOverlayCollapsed(!c);
      return !c;
    });
  }, []);

  const close = useCallback(() => {
    dispatch({ type: "reset" });
    setCollapsed(false);
    void ipc.closeSession(sessionId);
  }, [sessionId]);

  const runChip = useCallback(
    (actionId: string, label: string) => {
      if (!sessionId || busy) return;
      dispatch({ type: "turn", prompt: label, fromChip: true });
      ipc.runAction(sessionId, actionId).catch((err) =>
        dispatch({
          type: "error",
          e: { session_id: sessionId, code: "api", message: String(err) },
        }),
      );
    },
    [sessionId, busy],
  );

  const ask = useCallback(
    (question: string) => {
      if (!sessionId || busy || !question.trim()) return false;
      dispatch({ type: "turn", prompt: question.trim(), fromChip: false });
      ipc.ask(sessionId, question).catch((err) =>
        dispatch({
          type: "error",
          e: { session_id: sessionId, code: "api", message: String(err) },
        }),
      );
      return true;
    },
    [sessionId, busy],
  );

  const copy = useCallback(async () => {
    if (!sessionId) return;
    if (await ipc.copyLast(sessionId)) {
      setCopied(true);
      setTimeout(() => setCopied(false), 1200);
    }
  }, [sessionId]);

  useEffect(() => {
    const subs = [
      on("glance://captured", (e) => {
        setCollapsed(false);
        dispatch({ type: "captured", e });
      }),
      on("glance://classified", ({ session_id, ...c }) =>
        dispatch({ type: "classified", id: session_id, c }),
      ),
      on("glance://token", (e) =>
        dispatch({ type: "token", id: e.session_id, delta: e.delta }),
      ),
      on("glance://done", (e) =>
        dispatch({
          type: "done",
          id: e.session_id,
          totals: e.totals,
          stopReason: e.stop_reason,
        }),
      ),
      on("glance://error", (e) => dispatch({ type: "error", e })),
      on("glance://permission", (e) => {
        if (!e.granted) dispatch({ type: "permission" });
      }),
    ];
    return () => subs.forEach((s) => void s.then((f) => f()));
  }, []);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        close();
        return;
      }
      const typing = e.target instanceof HTMLTextAreaElement;
      if (typing) return;
      if (e.key === "/") {
        e.preventDefault();
        inputRef.current?.focus();
      } else if (e.metaKey && e.key.toLowerCase() === "c") {
        if (!window.getSelection()?.toString()) {
          e.preventDefault();
          void copy();
        }
      } else if (/^[1-5]$/.test(e.key) && !e.metaKey && !e.ctrlKey) {
        const action = session?.classification?.actions[Number(e.key) - 1];
        if (action) runChip(action.id, action.label);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [close, copy, runChip, session?.classification]);

  return (
    <div className="h-full w-full p-2">
      <div className="flex h-full flex-col overflow-hidden rounded-2xl border border-black/10 bg-stone-50/95 text-stone-900 shadow-[0_12px_40px_-8px_rgba(0,0,0,0.35)] dark:border-white/10 dark:bg-stone-900/95 dark:text-stone-100">
        <header data-tauri-drag-region className="flex items-center gap-2 px-4 pb-2 pt-3">
          <h1 data-tauri-drag-region className="font-display text-[19px] font-semibold tracking-tight">
            Glance
          </h1>
          {session?.classification && session.classification.kind !== "other" && (
            <span className="rounded-full bg-stone-200 px-2 py-0.5 text-[10px] font-medium uppercase tracking-wider text-stone-600 dark:bg-stone-800 dark:text-stone-300">
              {session.classification.kind}
            </span>
          )}
          <div data-tauri-drag-region className="flex-1" />
          <button
            onClick={toggleCollapsed}
            aria-label={collapsed ? "Expand" : "Minimize"}
            title={collapsed ? "Expand" : "Minimize"}
            className="rounded-md px-2 py-0.5 text-[13px] leading-none text-stone-500 hover:bg-black/5 dark:text-stone-400 dark:hover:bg-white/10"
          >
            {collapsed ? "▢" : "–"}
          </button>
          <button
            onClick={() => void ipc.openSettings()}
            title="Settings"
            className="rounded-md px-1.5 py-0.5 text-[13px] text-stone-500 hover:bg-black/5 dark:text-stone-400 dark:hover:bg-white/10"
          >
            ⚙
          </button>
          <button
            onClick={close}
            className="rounded-md px-2 py-0.5 text-[11px] text-stone-500 hover:bg-black/5 dark:text-stone-400 dark:hover:bg-white/10"
          >
            Esc
          </button>
        </header>

        {!collapsed && session && (
          <SessionView
            session={session}
            busy={busy}
            copied={copied}
            inputRef={inputRef}
            onChip={runChip}
            onAsk={ask}
            onCopy={copy}
          />
        )}
        {!collapsed && state.kind === "permission" && <Permission />}
        {!collapsed && state.kind === "capture-error" && (
          <Notice title="Capture failed" body={state.message} />
        )}
        {!collapsed && state.kind === "idle" && (
          <Notice title="Ready" body="Press ⌘⇧Space to capture a region." />
        )}
      </div>
    </div>
  );
}

function SessionView({
  session,
  busy,
  copied,
  inputRef,
  onChip,
  onAsk,
  onCopy,
}: {
  session: SessionState;
  busy: boolean;
  copied: boolean;
  inputRef: React.RefObject<HTMLTextAreaElement | null>;
  onChip: (id: string, label: string) => void;
  onAsk: (q: string) => boolean;
  onCopy: () => void;
}) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const hasTurns = session.turns.length > 0;
  const lastAnswer = session.turns.at(-1)?.answer ?? "";

  useEffect(() => {
    const el = scrollRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [session.turns.length, lastAnswer.length]);

  return (
    <>
      <div ref={scrollRef} className="min-h-0 flex-1 overflow-y-auto px-4 pb-3">
        <figure
          className={`flex items-center justify-center overflow-hidden rounded-xl bg-stone-200/70 p-2 transition-all dark:bg-black/40 ${
            hasTurns ? "h-24" : "h-48"
          }`}
        >
          <img
            src={session.thumb}
            alt="Captured region"
            draggable={false}
            className="max-h-full max-w-full rounded-md object-contain shadow-sm"
          />
        </figure>
        <p className="mt-1.5 text-[11px] tabular-nums text-stone-500 dark:text-stone-400">
          {session.classification?.summary ||
            `${session.width} × ${session.height}px`}
          <span className="text-stone-400 dark:text-stone-500">
            {" "}
            · sent as {session.upload.width}×{session.upload.height}{" "}
            {session.upload.media_type === "image/png" ? "PNG" : "JPEG"}
          </span>
        </p>

        {session.notice && <NoticeBanner notice={session.notice} />}

        <ActionChips
          actions={session.classification?.actions ?? null}
          disabled={busy}
          onRun={onChip}
        />

        {hasTurns && <StreamView turns={session.turns} />}
      </div>

      <footer className="border-t border-black/5 px-4 pb-3 pt-2.5 dark:border-white/10">
        <AskInput ref={inputRef} disabled={busy} followUp={hasTurns} onSubmit={onAsk} />
        <div className="mt-1.5 flex items-center justify-between text-[10.5px] tabular-nums text-stone-400 dark:text-stone-500">
          <span>{session.totals ? formatTotals(session.totals) : "1–5 actions · / ask"}</span>
          {session.turns.some((t) => t.status === "done") && (
            <button
              onClick={onCopy}
              className="rounded px-1.5 py-0.5 hover:bg-black/5 hover:text-stone-600 dark:hover:bg-white/10 dark:hover:text-stone-300"
            >
              {copied ? "Copied" : "Copy answer ⌘C"}
            </button>
          )}
        </div>
      </footer>
    </>
  );
}

function k(n: number) {
  return n >= 1000 ? `${(n / 1000).toFixed(1)}k` : String(n);
}

function formatTotals(t: Totals) {
  const input = t.input_tokens + t.cache_read_input_tokens + t.cache_creation_input_tokens;
  const cached = t.cache_read_input_tokens ? ` (${k(t.cache_read_input_tokens)} cached)` : "";
  return `${k(input)} in${cached} · ${k(t.output_tokens)} out`;
}

function NoticeBanner({ notice }: { notice: ErrorEvent }) {
  const needsKey = SESSION_NOTICES.has(notice.code);
  return (
    <div className="mt-3 flex items-center gap-3 rounded-lg border border-amber-600/20 bg-amber-50 px-3 py-2 text-[12px] text-amber-900 dark:border-amber-400/20 dark:bg-amber-950/40 dark:text-amber-200">
      <span className="flex-1">{notice.message}</span>
      {needsKey && (
        <button
          onClick={() => void ipc.openSettings()}
          className="shrink-0 rounded-md bg-amber-900 px-2 py-1 text-[11px] font-medium text-white hover:bg-amber-800 dark:bg-amber-200 dark:text-amber-950"
        >
          Open Settings
        </button>
      )}
    </div>
  );
}

function Permission() {
  const [checking, setChecking] = useState(false);
  const [stillMissing, setStillMissing] = useState(false);

  const recheck = async () => {
    setChecking(true);
    const granted = await ipc.screenPermission();
    setChecking(false);
    if (granted) void ipc.startCapture();
    else setStillMissing(true);
  };

  return (
    <div className="flex flex-1 flex-col justify-center gap-3 px-4 pb-4">
      <h2 className="font-display text-[22px] leading-tight">
        Glance needs Screen Recording access
      </h2>
      <p className="text-[13px] leading-relaxed text-stone-600 dark:text-stone-300">
        Captures stay in memory and are never written to disk. Enable Glance in
        System Settings → Privacy &amp; Security → Screen &amp; System Audio
        Recording.
      </p>
      {stillMissing && (
        <p className="text-[12px] text-amber-700 dark:text-amber-400">
          Still not granted. macOS may require quitting and reopening Glance
          after you switch it on.
        </p>
      )}
      <div className="mt-1 flex gap-2">
        <button
          onClick={() => void ipc.openScreenSettings()}
          className="rounded-lg bg-stone-900 px-3 py-1.5 text-[13px] font-medium text-white hover:bg-stone-700 dark:bg-stone-100 dark:text-stone-900 dark:hover:bg-white"
        >
          Open System Settings
        </button>
        <button
          onClick={recheck}
          disabled={checking}
          className="rounded-lg border border-black/10 px-3 py-1.5 text-[13px] hover:bg-black/5 dark:border-white/15 dark:hover:bg-white/10"
        >
          {checking ? "Checking…" : "Check again"}
        </button>
      </div>
    </div>
  );
}

function Notice({ title, body }: { title: string; body: string }) {
  return (
    <div className="flex flex-1 flex-col justify-center gap-1.5 px-4 pb-4">
      <h2 className="font-display text-[20px]">{title}</h2>
      <p className="text-[13px] text-stone-600 dark:text-stone-300">{body}</p>
    </div>
  );
}
