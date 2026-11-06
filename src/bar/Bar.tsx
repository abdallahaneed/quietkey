// The floating dictation pill. The window is click-through for now (see lib.rs place_bar),
// so everything here is display only; keyboard shortcuts drive it.
import { useEffect, useRef, useState } from "react";
import { AlertCircle, Check, Square, X } from "lucide-react";
import { events, type DictationState } from "../lib/api";

const BARS = 5;
const HOLD_MS: Partial<Record<DictationState, number>> = { success: 600, error: 4000, warning: 4000, cancelled: 2000 };

export function Bar() {
  const [state, setState] = useState<DictationState>("idle");
  const [message, setMessage] = useState<string | null>(null);
  const [levels, setLevels] = useState<number[]>(Array(BARS).fill(0));
  const timer = useRef<number | undefined>(undefined);

  useEffect(() => {
    const un = [
      events.onState(({ state, message }) => {
        window.clearTimeout(timer.current);
        setState(state);
        setMessage(message);
        const hold = HOLD_MS[state];
        if (hold) timer.current = window.setTimeout(() => setState("idle"), hold);
        if (state !== "recording" && state !== "hands_free") setLevels(Array(BARS).fill(0));
      }),
      events.onLevel((rms) => {
        // Speech RMS is small (≈0.01–0.2); scale it so normal talking fills the bars.
        const v = Math.min(1, Math.sqrt(rms) * 2.2);
        setLevels((prev) => [...prev.slice(1), v]);
      }),
    ];
    return () => un.forEach((p) => p.then((f) => f()));
  }, []);

  const active = state !== "idle";
  const live = state === "recording" || state === "hands_free";

  return (
    <div className="flex h-screen w-screen items-end justify-center pb-2">
      <div
        role="status"
        aria-live="polite"
        aria-label={label(state, message)}
        className={[
          "flex items-center justify-center gap-2 rounded-pill bg-bar-bg text-bar-text shadow-bar",
          "transition-[width,height,opacity] duration-[var(--motion-base)] ease-[var(--motion-ease)]",
          active ? "h-9 min-w-[120px] px-3" : "h-2 w-11 opacity-60",
        ].join(" ")}
      >
        {live && (
          <>
            <span className="size-2 rounded-pill bg-bar-rec [animation:pulse-dot_1.2s_ease-in-out_infinite]" aria-hidden />
            <span className="flex h-5 items-center gap-[3px]" aria-hidden>
              {levels.map((v, i) => (
                <span key={i} className="w-[3px] rounded-pill bg-bar-level transition-[height] duration-75" style={{ height: `${Math.max(3, v * 20)}px` }} />
              ))}
            </span>
            {state === "hands_free" && (
              <span className="flex items-center gap-1 text-bar-muted" aria-hidden>
                <Square className="size-3" /> <X className="size-3.5" />
              </span>
            )}
          </>
        )}
        {state === "processing" && (
          <span className="flex gap-1" aria-hidden>
            {[0, 1, 2].map((i) => (
              <span key={i} className="size-1.5 rounded-pill bg-bar-text" style={{ animation: `shimmer 1s ${i * 0.15}s ease-in-out infinite` }} />
            ))}
          </span>
        )}
        {state === "success" && <Check className="size-4 text-bar-text" aria-hidden />}
        {(state === "error" || state === "warning" || state === "cancelled") && (
          <span className="flex items-center gap-1.5 whitespace-nowrap text-xs">
            {state !== "cancelled" && <AlertCircle className="size-3.5 text-bar-rec" aria-hidden />}
            {label(state, message)}
          </span>
        )}
      </div>
    </div>
  );
}

function label(state: DictationState, message: string | null): string {
  switch (state) {
    case "recording": return "Listening";
    case "hands_free": return "Listening, press fn to stop";
    case "processing": return "Writing it down";
    case "success": return "Text inserted";
    case "cancelled": return "Discarded";
    case "error":
    case "warning": return message ?? "Something went wrong";
    default: return "Ready, hold fn to talk";
  }
}
