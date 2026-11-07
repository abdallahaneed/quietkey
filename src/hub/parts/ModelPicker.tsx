// F6: pick the Whisper model. A model not on disk is downloaded first; the old one keeps working until the new one is loaded.
import { useEffect, useState } from "react";
import { api, events, type WhisperModel } from "../../lib/api";
import { Notice, RadioCards } from "../../ui";

export function ModelPicker() {
  const [models, setModels] = useState<WhisperModel[] | null>(null);
  const [busy, setBusy] = useState<{ id: string; pct: number | null } | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = () => api.models().then(setModels).catch((e) => setError(String(e)));
  useEffect(() => {
    load();
    const un = events.onModel((e) => {
      if (e.kind === "progress") setBusy({ id: e.id, pct: e.total ? Math.floor((e.done / e.total) * 100) : null });
      else {
        setBusy(null);
        if (e.kind === "error") setError(`Could not switch the model: ${e.message}`);
        load();
      }
    });
    return () => { un.then((f) => f()); };
  }, []);

  const choose = (id: string) => {
    if (busy) return;
    setError(null);
    setBusy({ id, pct: null });
    api.modelSelect(id).catch((e) => { setBusy(null); setError(String(e)); });
  };

  if (!models) return error ? <Notice tone="error">{error}</Notice> : null;
  const active = models.find((m) => m.active)?.model.id ?? "";
  const status = (m: WhisperModel) => {
    if (busy?.id === m.model.id) return busy.pct === null || busy.pct >= 100 ? "Loading…" : `Downloading ${busy.pct}%`;
    return m.downloaded ? "On this Mac" : `${m.model.size_mb} MB download`;
  };
  return (
    <div className="flex flex-col gap-2">
      {error && <Notice tone="error">{error}</Notice>}
      <RadioCards label="Speech model" value={active} onChange={choose} options={models.map((m) => ({
        value: m.model.id,
        title: m.model.label,
        body: `${m.model.note}. Uses about ${m.model.ram_mb} MB of memory. ${status(m)}.`,
        disabled: !!busy && busy.id !== m.model.id,
      }))} />
    </div>
  );
}
