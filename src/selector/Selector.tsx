import { useCallback, useEffect, useState, type PointerEvent } from "react";
import { ipc, on, type Rect } from "../lib/ipc";

type Point = { x: number; y: number };

const MIN_SIZE = 4;

function toRect(a: Point, b: Point): Rect {
  return {
    x: Math.min(a.x, b.x),
    y: Math.min(a.y, b.y),
    width: Math.abs(a.x - b.x),
    height: Math.abs(a.y - b.y),
  };
}

/** Wait for the browser to paint so the dim layer is gone before capture. */
function nextPaint(): Promise<void> {
  return new Promise((resolve) =>
    requestAnimationFrame(() => requestAnimationFrame(() => resolve())),
  );
}

export function Selector() {
  const [origin, setOrigin] = useState<Point | null>(null);
  const [cursor, setCursor] = useState<Point | null>(null);
  const [capturing, setCapturing] = useState(false);

  const reset = useCallback(() => {
    setOrigin(null);
    setCursor(null);
    setCapturing(false);
  }, []);

  const cancel = useCallback(() => {
    reset();
    void ipc.cancelSelection();
  }, [reset]);

  useEffect(() => {
    const unlisten = on("glance://select-start", reset);
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") cancel();
    };
    window.addEventListener("keydown", onKey);
    return () => {
      void unlisten.then((f) => f());
      window.removeEventListener("keydown", onKey);
    };
  }, [reset, cancel]);

  const onPointerDown = (e: PointerEvent<HTMLDivElement>) => {
    if (e.button !== 0 || capturing) return;
    e.currentTarget.setPointerCapture(e.pointerId);
    const p = { x: e.clientX, y: e.clientY };
    setOrigin(p);
    setCursor(p);
  };

  const onPointerMove = (e: PointerEvent<HTMLDivElement>) => {
    if (origin) setCursor({ x: e.clientX, y: e.clientY });
  };

  const onPointerUp = async (e: PointerEvent<HTMLDivElement>) => {
    if (!origin || capturing) return;
    const rect = toRect(origin, { x: e.clientX, y: e.clientY });
    if (rect.width < MIN_SIZE || rect.height < MIN_SIZE) {
      cancel();
      return;
    }
    setCapturing(true);
    await nextPaint();
    try {
      await ipc.captureRegion(rect);
    } catch (err) {
      console.error("capture failed", err);
    } finally {
      reset();
    }
  };

  if (capturing) return <div className="h-full w-full" />;

  const rect = origin && cursor ? toRect(origin, cursor) : null;

  return (
    <div
      className="relative h-full w-full cursor-crosshair"
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerUp}
    >
      {rect ? (
        <>
          <div
            className="pointer-events-none absolute rounded-[2px] outline outline-1 outline-white/90"
            style={{
              left: rect.x,
              top: rect.y,
              width: rect.width,
              height: rect.height,
              boxShadow: "0 0 0 100vmax rgba(10, 10, 12, 0.38)",
            }}
          />
          <div
            className="pointer-events-none absolute rounded-md bg-black/75 px-1.5 py-0.5 font-mono text-[11px] text-white tabular-nums"
            style={{ left: rect.x, top: rect.y + rect.height + 6 }}
          >
            {Math.round(rect.width)} × {Math.round(rect.height)}
          </div>
        </>
      ) : (
        <div className="pointer-events-none absolute inset-0 bg-black/20">
          <div className="absolute left-1/2 top-10 -translate-x-1/2 rounded-full bg-black/70 px-4 py-1.5 text-[13px] text-white/90 shadow-lg">
            Drag to capture · <kbd className="font-sans">Esc</kbd> to cancel
          </div>
        </div>
      )}
    </div>
  );
}
