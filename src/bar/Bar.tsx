// The floating dictation pill. The window is a non-activating panel (lib.rs make_panel), so clicking it never
// takes focus from the app being typed into. It is click-through except while it shows buttons.
import { useEffect, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { AlertCircle, Check, Square, X } from "lucide-react";
import { api, events, pushToTalk, shortcutText, type DictationState } from "../lib/api";

const BARS = 5;
// cancelled stays long enough to reach Undo (the backend keeps the audio 6 s)
const HOLD_MS: Partial<Record<DictationState, number>> = { success: 600, error: 4000, warning: 4000, cancelled: 5000 };
const btn = "grid size-6 place-items-center rounded-pill text-bar-muted hover:bg-bar-level/20 hover:text-bar-text";

export function Bar() {
  const [state, setState] = useState<DictationState>("idle");
  const [message, setMessage] = useState<string | null>(null);
  const [levels, setLevels] = useState<number[]>(Array(BARS).fill(0));
  const [keys, setKeys] = useState("fn");
  const timer = useRef<number | undefined>(undefined);

  useEffect(() => {
    api.settings().then((s) => setKeys(shortcutText(pushToTalk(s)))).catch(() => {});
    const un = [
      events.onSettings((s) => setKeys(shortcutText(pushToTalk(s)))),
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
  const clickable = live || state === "cancelled";
  useEffect(() => { getCurrentWindow().setIgnoreCursorEvents(!clickable).catch(() => {}); }, [clickable]);

  return (
    <div className="flex h-screen w-screen items-end justify-center pb-2">
      <div
        aria-label={label(state, message, keys)}
        className={[
          "flex items-center justify-center gap-2 rounded-pill bg-bar-bg text-bar-text shadow-bar",
          "transition-[width,height,opacity] duration-[var(--motion-base)] ease-[var(--motion-ease)]",
          active ? "h-9 min-w-[120px] px-3" : "h-2 w-11 opacity-60",
        ].join(" ")}
      >
        <span role="status" aria-live="polite" className="sr-only">{label(state, message, keys)}</span>
        {live && (
          <>
            <button type="button" className={btn} aria-label="Cancel" title="Cancel (esc)" onClick={() => api.dictationCancel()}>
              <X className="size-3.5" />
            </button>
            <span className="size-2 rounded-pill bg-bar-rec [animation:pulse-dot_1.2s_ease-in-out_infinite]" aria-hidden />
            <span className="flex h-5 items-center gap-[3px]" aria-hidden>
              {levels.map((v, i) => (
                <span key={i} className="w-[3px] rounded-pill bg-bar-level transition-[height] duration-75" style={{ height: `${Math.max(3, v * 20)}px` }} />
              ))}
            </span>
            {state === "hands_free" && (
              <button type="button" className={btn} aria-label="Stop and insert" title="Stop and insert" onClick={() => api.dictationToggle()}>
                <Square className="size-3 fill-current" />
              </button>
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
        {(state === "error" || state === "warning") && (
          <span className="flex items-center gap-1.5 whitespace-nowrap text-xs">
            <AlertCircle className="size-3.5 text-bar-rec" aria-hidden />
            {label(state, message, keys)}
          </span>
        )}
        {state === "cancelled" && (
          <span className="flex items-center gap-2 whitespace-nowrap text-xs">
            Discarded
            <button type="button" className="rounded-pill px-2 py-0.5 font-semibold text-bar-text underline-offset-2 hover:underline" onClick={() => api.dictationUndo()}>
              Undo
            </button>
          </span>
        )}
      </div>
    </div>
  );
}

function label(state: DictationState, message: string | null, keys: string): string {
  switch (state) {
    case "recording": return "Listening";
    case "hands_free": return `Listening, press ${keys} to stop`;
    case "processing": return "Writing it down";
    case "success": return "Text inserted";
    case "cancelled": return "Discarded. Undo to insert it after all";
    case "error":
    case "warning": return message ?? "Something went wrong";
    default: return `Ready, hold ${keys} to talk`;
  }
}
