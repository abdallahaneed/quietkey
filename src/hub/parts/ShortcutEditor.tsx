// Records a new hold-to-dictate shortcut. Modifier keys only: they type nothing, so holding them in any app is safe.
// The webview never sees fn, so fn comes back through "Use fn".
import { useEffect, useRef, useState } from "react";
import { api, pushToTalk, type Settings } from "../../lib/api";
import { Button, KeyCaps } from "../../ui";

const CODE_TO_KEY: Record<string, string> = {
  ControlLeft: "LeftCtrl", ControlRight: "RightCtrl", AltLeft: "LeftOpt", AltRight: "RightOpt",
  MetaLeft: "LeftCmd", MetaRight: "RightCmd", ShiftLeft: "LeftShift", ShiftRight: "RightShift",
};

export function ShortcutEditor({ settings, onSaved }: { settings: Settings; onSaved: (s: Settings) => void }) {
  const [recording, setRecording] = useState(false);
  const [preview, setPreview] = useState<string[]>([]);
  const [error, setError] = useState<string | null>(null);
  const held = useRef(new Set<string>());
  const peak = useRef<string[]>([]);
  const current = pushToTalk(settings);

  const save = async (keys: string[]) => {
    setError(null);
    try {
      onSaved(await api.updateSettings({
        shortcuts: { ...settings.shortcuts, push_to_talk: [keys], hands_free: [[...keys, "Space"]] },
      }));
    } catch (e) {
      setError(String(e)); // the backend explains why, e.g. a key used while typing
    }
  };

  const stop = () => {
    setRecording(false);
    held.current.clear();
    peak.current = [];
    api.hotkeyPause(false).catch(() => {});
  };

  useEffect(() => {
    if (!recording) return;
    const down = (e: KeyboardEvent) => {
      e.preventDefault();
      if (e.code === "Escape") { stop(); setPreview([]); return; }
      const k = CODE_TO_KEY[e.code];
      if (!k) { setError("Use modifier keys only: ⌃ ⌥ ⌘ ⇧, alone or together."); return; }
      held.current.add(k);
      if (held.current.size >= peak.current.length) peak.current = [...held.current];
      setPreview(peak.current);
    };
    const up = (e: KeyboardEvent) => {
      const k = CODE_TO_KEY[e.code];
      if (!k) return;
      held.current.delete(k);
      if (held.current.size === 0 && peak.current.length) {
        const keys = peak.current;
        stop();
        save(keys);
      }
    };
    window.addEventListener("keydown", down);
    window.addEventListener("keyup", up);
    return () => { window.removeEventListener("keydown", down); window.removeEventListener("keyup", up); };
  }, [recording]);

  useEffect(() => () => { api.hotkeyPause(false).catch(() => {}); }, []);

  return (
    <div className="flex flex-col items-end gap-2">
      <div className="flex items-center gap-2">
        {recording ? (
          <span className="text-sm text-muted" role="status" aria-live="polite">
            {preview.length ? <KeyCaps keys={preview} /> : "Press the keys, then let go. Esc stops."}
          </span>
        ) : (
          <KeyCaps keys={current} />
        )}
        {recording ? (
          <Button size="sm" onClick={() => { stop(); setPreview([]); }}>Stop</Button>
        ) : (
          <Button size="sm" onClick={() => { setError(null); setPreview([]); setRecording(true); api.hotkeyPause(true).catch(() => {}); }}>Change</Button>
        )}
        {!recording && current.join() !== "Fn" && <Button size="sm" variant="ghost" onClick={() => save(["Fn"])}>Use fn</Button>}
      </div>
      {error && <p className="max-w-80 text-right text-sm text-danger" role="alert">{error}</p>}
    </div>
  );
}
