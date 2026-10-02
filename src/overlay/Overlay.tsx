import { useCallback, useEffect, useState } from "react";
import { ipc, on } from "../lib/ipc";

type View =
  | { kind: "idle" }
  | { kind: "permission" }
  | { kind: "error"; message: string }
  | {
      kind: "captured";
      sessionId: string;
      thumb: string;
      width: number;
      height: number;
    };

export function Overlay() {
  const [view, setView] = useState<View>({ kind: "idle" });
  const [collapsed, setCollapsed] = useState(false);

  const toggleCollapsed = useCallback(() => {
    setCollapsed((c) => {
      void ipc.setOverlayCollapsed(!c);
      return !c;
    });
  }, []);

  const close = useCallback(() => {
    const sessionId = view.kind === "captured" ? view.sessionId : null;
    setView({ kind: "idle" });
    setCollapsed(false);
    void ipc.closeSession(sessionId);
  }, [view]);

  useEffect(() => {
    const subs = [
      on("glance://captured", (e) => {
        setCollapsed(false);
        setView({
          kind: "captured",
          sessionId: e.session_id,
          thumb: `data:image/png;base64,${e.thumb_b64}`,
          width: e.width,
          height: e.height,
        });
      }),
      on("glance://permission", (e) => {
        if (!e.granted) setView({ kind: "permission" });
      }),
      on("glance://error", (e) =>
        setView({ kind: "error", message: e.message }),
      ),
    ];
    return () => subs.forEach((s) => void s.then((f) => f()));
  }, []);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") close();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [close]);

  return (
    <div className="h-full w-full p-2">
      <div className="flex h-full flex-col overflow-hidden rounded-2xl border border-black/10 bg-stone-50/95 text-stone-900 shadow-[0_12px_40px_-8px_rgba(0,0,0,0.35)] dark:border-white/10 dark:bg-stone-900/95 dark:text-stone-100">
        <header
          data-tauri-drag-region
          className="flex items-center justify-between px-4 pb-2 pt-3"
        >
          <h1
            data-tauri-drag-region
            className="font-display text-[19px] font-semibold tracking-tight"
          >
            Glance
          </h1>
          <div className="flex items-center gap-1">
            <button
              onClick={toggleCollapsed}
              aria-label={collapsed ? "Expand" : "Minimize"}
              title={collapsed ? "Expand" : "Minimize"}
              className="rounded-md px-2 py-0.5 text-[13px] leading-none text-stone-500 hover:bg-black/5 dark:text-stone-400 dark:hover:bg-white/10"
            >
              {collapsed ? "▢" : "–"}
            </button>
            <button
              onClick={close}
              className="rounded-md px-2 py-0.5 text-[11px] text-stone-500 hover:bg-black/5 dark:text-stone-400 dark:hover:bg-white/10"
            >
              Esc
            </button>
          </div>
        </header>
        <div
          className={`min-h-0 flex-1 flex-col px-4 pb-4 ${collapsed ? "hidden" : "flex"}`}
        >
          {view.kind === "captured" && <Captured view={view} />}
          {view.kind === "permission" && <Permission />}
          {view.kind === "error" && (
            <Notice title="Capture failed" body={view.message} />
          )}
          {view.kind === "idle" && (
            <Notice title="Ready" body="Press ⌘⇧Space to capture a region." />
          )}
        </div>
      </div>
    </div>
  );
}

function Captured({ view }: { view: Extract<View, { kind: "captured" }> }) {
  return (
    <>
      <figure className="flex min-h-0 flex-1 items-center justify-center overflow-hidden rounded-xl bg-stone-200/70 p-2 dark:bg-black/40">
        <img
          src={view.thumb}
          alt="Captured region"
          draggable={false}
          className="max-h-full max-w-full rounded-md object-contain shadow-sm"
        />
      </figure>
      <p className="mt-2 text-[11px] tabular-nums text-stone-500 dark:text-stone-400">
        {view.width} × {view.height}px · held in memory only
      </p>
      {/* Action chips arrive with the classifier in v0.2. */}
      <div className="mt-3 flex flex-wrap gap-1.5" aria-hidden>
        {[72, 88, 64, 96].map((w, i) => (
          <span
            key={i}
            className="h-7 animate-pulse rounded-full bg-stone-200 dark:bg-stone-800"
            style={{ width: w }}
          />
        ))}
      </div>
      <input
        disabled
        placeholder="Ask about this…  (v0.2)"
        className="mt-3 w-full rounded-lg border border-black/10 bg-white/70 px-3 py-2 text-[13px] placeholder:text-stone-400 disabled:cursor-not-allowed dark:border-white/10 dark:bg-black/30"
      />
    </>
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
    <div className="flex flex-1 flex-col justify-center gap-3">
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
    <div className="flex flex-1 flex-col justify-center gap-1.5">
      <h2 className="font-display text-[20px]">{title}</h2>
      <p className="text-[13px] text-stone-600 dark:text-stone-300">{body}</p>
    </div>
  );
}
