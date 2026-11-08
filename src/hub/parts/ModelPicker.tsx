// The speech model list: a grade per language (English, German, Arabic), speed, size, search, download and delete.
// A model not on disk is downloaded first; the old one keeps working until the new one is loaded.
import { useEffect, useMemo, useState } from "react";
import { CheckCircle2, Circle, Download, Loader2, Search, Trash2 } from "lucide-react";
import { RadioGroup } from "radix-ui";
import { api, events, type WhisperModel } from "../../lib/api";
import { IconButton, Notice, cx } from "../../ui";

const LANGS: [string, string][] = [["en", "English"], ["de", "German"], ["ar", "Arabic"]];

/** A=4 … F=0, "+"/"-" move a third; "-" alone (not supported) is -1. */
export function gradeScore(g: string): number {
  if (g === "-") return -1;
  const base = { A: 4, B: 3, C: 2, D: 1, F: 0 }[g[0]] ?? 0;
  return base + (g.endsWith("+") ? 0.33 : g.length > 1 && g.endsWith("-") ? -0.33 : 0);
}

function Grade({ lang, grade }: { lang: string; grade: string }) {
  const s = gradeScore(grade);
  return (
    <span
      aria-label={`${lang}: ${grade === "-" ? "not supported" : grade}`}
      className={cx(
        "inline-flex h-6 w-9 items-center justify-center rounded-sm text-xs font-semibold",
        s >= 3.67 ? "bg-accent text-on-accent" : s >= 2.67 ? "bg-secondary-soft text-secondary" : s >= 1.67 ? "border border-border text-fg" : s >= 0 ? "border border-dashed border-border text-muted" : "text-muted",
      )}
    >
      {grade === "-" ? "—" : grade.replace("-", "−")}
    </span>
  );
}

function Speed({ n }: { n: number }) {
  return (
    <span aria-label={`Speed ${n} of 4`} className="inline-flex h-4 items-end gap-0.5">
      {[1, 2, 3, 4].map((i) => (
        <span key={i} className={cx("w-1 rounded-[1px]", i <= n ? "bg-fg" : "bg-border")} style={{ height: `${4 + i * 3}px` }} />
      ))}
    </span>
  );
}

const size = (mb: number) => (mb >= 1000 ? `${(mb / 1000).toFixed(1)} GB` : `${mb} MB`);

export function ModelPicker({ search = true, languages = ["en"] }: { search?: boolean; languages?: string[] }) {
  const [models, setModels] = useState<WhisperModel[] | null>(null);
  const [busy, setBusy] = useState<{ id: string; pct: number | null } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [query, setQuery] = useState("");

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
  const remove = async (m: WhisperModel) => {
    setError(null);
    try { await api.modelDelete(m.model.id); await load(); }
    catch (e) { setError(String(e)); }
  };

  // Best for the user's languages first; languages without grades count as English.
  const fit = (m: WhisperModel) => {
    const g = Object.fromEntries(m.model.grades);
    const ls = languages.filter((l) => l in g);
    return (ls.length ? ls : ["en"]).reduce((n, l) => n + gradeScore(g[l] ?? "-"), 0) / Math.max(ls.length, 1);
  };
  const shown = useMemo(() => {
    const q = query.trim().toLowerCase();
    return (models ?? []).filter((m) => !q || `${m.model.label} ${m.model.note}`.toLowerCase().includes(q)).sort((a, b) => fit(b) - fit(a));
  }, [models, query, languages]);

  if (!models) return error ? <Notice tone="error">{error}</Notice> : null;
  const active = models.find((m) => m.active)?.model.id ?? "";

  const row = (m: WhisperModel) => {
    const g = Object.fromEntries(m.model.grades);
    const mine = busy?.id === m.model.id;
    const status = mine ? (busy.pct === null || busy.pct >= 100 ? "Loading…" : `Downloading ${busy.pct}%`) : null;
    return (
      <div key={m.model.id} className={cx("flex items-center gap-2 rounded-md pr-2", m.active && "bg-accent-soft")}>
        <RadioGroup.Item
          value={m.model.id}
          disabled={!!busy && !mine}
          aria-label={`${m.model.label}. ${m.model.note}. ${m.downloaded ? "On this Mac" : `${size(m.model.size_mb)} download`}`}
          className="flex min-w-0 flex-1 items-center gap-3 rounded-md px-2 py-2 text-left hover:bg-surface-2 disabled:opacity-40"
        >
          <span className="shrink-0 text-muted" aria-hidden>
            {mine ? <Loader2 className="size-4 animate-spin" /> : m.active ? <CheckCircle2 className="size-4 text-accent" /> : m.downloaded ? <Circle className="size-4" /> : <Download className="size-4" />}
          </span>
          <span className="flex min-w-0 flex-1 flex-col">
            <span className="text-base font-semibold">{m.model.label}</span>
            <span className="truncate text-sm text-muted" title={m.model.note}>{status ?? m.model.note}</span>
          </span>
          {LANGS.map(([code, name]) => <Grade key={code} lang={name} grade={g[code] ?? "-"} />)}
          <span className="flex w-10 justify-center"><Speed n={m.model.speed} /></span>
          <span className="w-14 text-right text-xs text-muted">{size(m.model.size_mb)}</span>
        </RadioGroup.Item>
        <span className="w-7">
          {m.downloaded && !m.active && !mine && (
            <IconButton label={`Delete ${m.model.label}`} onClick={() => remove(m)}><Trash2 className="size-4" /></IconButton>
          )}
        </span>
      </div>
    );
  };
  const section = (title: string, list: WhisperModel[]) => list.length > 0 && (
    <div className="flex flex-col gap-0.5">
      <p className="px-2 pt-2 text-xs font-semibold uppercase tracking-wide text-muted">{title}</p>
      {list.map(row)}
    </div>
  );

  return (
    <div className="flex flex-col gap-2">
      {error && <Notice tone="error">{error}</Notice>}
      <div className="rounded-lg border border-border bg-surface p-1.5">
        <div className="flex items-center gap-3 border-b border-border px-2 pb-1.5">
          {search ? (
            <label className="flex flex-1 items-center gap-2">
              <Search className="size-4 text-muted" aria-hidden />
              <input value={query} onChange={(e) => setQuery(e.target.value)} placeholder="Search models" aria-label="Search models"
                className="h-8 flex-1 bg-transparent text-base outline-none placeholder:text-muted" />
            </label>
          ) : <span className="flex-1" />}
          <span className="flex items-center gap-3 pr-9 text-xs font-semibold text-muted" aria-hidden>
            {LANGS.map(([code]) => <span key={code} className="w-9 text-center uppercase">{code}</span>)}
            <span className="w-10 text-center">SPEED</span>
            <span className="w-14 text-right">SIZE</span>
          </span>
        </div>
        <RadioGroup.Root aria-label="Speech model" value={active} onValueChange={choose} className="flex flex-col gap-1">
          {section("On this Mac", shown.filter((m) => m.downloaded))}
          {section("Download", shown.filter((m) => !m.downloaded))}
          {shown.length === 0 && <p className="px-2 py-6 text-center text-sm text-muted">No model matches "{query}".</p>}
        </RadioGroup.Root>
      </div>
    </div>
  );
}
