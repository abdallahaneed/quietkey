// The bar along the bottom of the window: the speech model in use with a menu to switch between downloaded ones,
// download progress while a model is fetched, and the app version.
import { useEffect, useState } from "react";
import { ChevronDown } from "lucide-react";
import { api, events, type Settings, type WhisperModel } from "../../lib/api";
import { Menu, cx } from "../../ui";
import { version } from "../../../package.json";
import type { Nav } from "../Hub";

export function Footer({ nav }: { nav: Nav }) {
  const [models, setModels] = useState<WhisperModel[]>([]);
  const [engine, setEngine] = useState<Settings["stt_engine"]>("local");
  const [busy, setBusy] = useState<{ id: string; pct: number | null } | null>(null);
  const [error, setError] = useState(false);

  const load = () => api.models().then(setModels).catch(() => {});
  useEffect(() => {
    load();
    api.settings().then((s) => setEngine(s.stt_engine)).catch(() => {});
    const unModel = events.onModel((e) => {
      if (e.kind === "progress") setBusy({ id: e.id, pct: e.total ? Math.floor((e.done / e.total) * 100) : null });
      else { setBusy(null); setError(e.kind === "error"); load(); }
    });
    const unSettings = events.onSettings((s) => setEngine(s.stt_engine));
    return () => { unModel.then((f) => f()); unSettings.then((f) => f()); };
  }, []);

  const active = models.find((m) => m.active && m.downloaded);
  const name = (id: string) => models.find((m) => m.model.id === id)?.model.label ?? id;
  const label = engine === "groq" ? "Groq Whisper (online)"
    : busy ? `${name(busy.id)}: ${busy.pct === null || busy.pct >= 100 ? "loading…" : `downloading ${busy.pct}%`}`
    : active?.model.label ?? "No speech model yet";
  const dot = engine === "groq" || active ? (busy ? "bg-warning" : error ? "bg-danger" : "bg-success") : "bg-warning";

  const pick = (id: string) => {
    setError(false);
    setBusy({ id, pct: null });
    const go = engine === "groq" ? api.updateSettings({ stt_engine: "local" }).then(() => api.modelSelect(id)) : api.modelSelect(id);
    go.catch(() => { setBusy(null); setError(true); });
  };

  return (
    <footer className="flex h-9 shrink-0 items-center justify-between border-t border-border bg-bg px-3 text-xs text-muted">
      <Menu
        align="start"
        side="top"
        trigger={
          <button type="button" aria-label={`Speech model: ${label}. Switch model`} className="flex h-7 items-center gap-2 rounded-md px-2 hover:bg-surface-2 hover:text-fg">
            <span className={cx("size-2 rounded-pill", dot)} aria-hidden />
            <span className="max-w-64 truncate">{label}</span>
            <ChevronDown className="size-3.5" aria-hidden />
          </button>
        }
        items={[
          ...models.filter((m) => m.downloaded).map((m) => ({
            label: m.model.label, checked: engine === "local" && m.active, disabled: !!busy, onSelect: () => !m.active || engine === "groq" ? pick(m.model.id) : undefined,
          })),
          { label: "All models…", onSelect: () => nav("models") },
        ]}
      />
      <span>Quietkey {version}</span>
    </footer>
  );
}
