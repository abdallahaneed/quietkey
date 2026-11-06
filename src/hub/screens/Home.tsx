// Home: dictation history grouped by day, newest first, live-updating.
import { useEffect, useMemo, useState } from "react";
import { Copy, Mic, MoreHorizontal } from "lucide-react";
import { api, events, type Permissions, type Transcript } from "../../lib/api";
import { EmptyState, IconButton, KeyCaps, Menu, Notice, PageHeader, SkeletonRows, Button, cx } from "../../ui";
import type { Nav } from "../Hub";

function dayLabel(ms: number): string {
  const d = new Date(ms);
  const today = new Date();
  const yest = new Date(); yest.setDate(today.getDate() - 1);
  if (d.toDateString() === today.toDateString()) return "Today";
  if (d.toDateString() === yest.toDateString()) return "Yesterday";
  return d.toLocaleDateString(undefined, { weekday: "short", day: "numeric", month: "short" });
}
const time = (ms: number) => new Date(ms).toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });

export function Home({ nav }: { nav: Nav }) {
  const [rows, setRows] = useState<Transcript[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [perm, setPerm] = useState<Permissions | null>(null);
  const [copied, setCopied] = useState<string | null>(null);

  useEffect(() => {
    api.transcriptsList().then(setRows).catch((e) => setError(String(e)));
    api.permissions().then(setPerm).catch(() => {});
    const un = events.onTranscript((t) => setRows((r) => [t, ...(r ?? [])]));
    return () => { un.then((f) => f()); };
  }, []);

  const groups = useMemo(() => {
    const g: [string, Transcript[]][] = [];
    for (const t of rows ?? []) {
      const label = dayLabel(t.created_at);
      if (g.length && g[g.length - 1][0] === label) g[g.length - 1][1].push(t);
      else g.push([label, [t]]);
    }
    return g;
  }, [rows]);

  const weekWords = useMemo(() => {
    const since = Date.now() - 7 * 864e5;
    return (rows ?? []).filter((t) => t.created_at >= since && t.status !== "cancelled").reduce((n, t) => n + t.word_count, 0);
  }, [rows]);

  const copy = async (t: Transcript) => {
    await api.transcriptCopy(t.id);
    setCopied(t.id);
    setTimeout(() => setCopied(null), 1500);
  };

  const needsSetup = perm && !(perm.accessibility && perm.hotkey_running && !perm.fn_key_conflict);

  return (
    <>
      <PageHeader
        title="Your dictations"
        description={rows ? `${weekWords.toLocaleString()} words in the last 7 days` : undefined}
        actions={<span className="flex items-center gap-2 text-sm text-muted">Hold <KeyCaps keys={["Fn"]} /> and talk</span>}
      />
      {needsSetup && (
        <div className="mb-4">
          <Notice tone="warning" action={<Button size="sm" onClick={() => nav("setup")}>Finish setup</Button>}>
            Dictation needs two Mac permissions before it can type for you.
          </Notice>
        </div>
      )}
      {error && <Notice tone="error">Could not load history: {error}</Notice>}
      {!rows && !error && <SkeletonRows />}
      {rows && rows.length === 0 && (
        <EmptyState
          icon={<Mic className="size-8" />}
          title="Nothing dictated yet"
          body="Click into any text field, hold the fn key, say a sentence, then let go. It shows up here too."
        />
      )}
      {groups.map(([label, items]) => (
        <section key={label} className="mb-6" aria-labelledby={`d-${label}`}>
          <h2 id={`d-${label}`} className="sticky top-[52px] z-[5] bg-bg py-1 text-xs font-semibold uppercase tracking-wide text-muted">{label}</h2>
          <ul className="divide-y divide-border">
            {items.map((t) => (
              <li key={t.id} className="group flex items-start gap-4 py-3">
                <span className="w-16 shrink-0 pt-0.5 text-xs text-muted">{time(t.created_at)}</span>
                <div className="min-w-0 flex-1">
                  <p className="line-clamp-3 select-text text-base">{t.final_text}</p>
                  <p className="mt-1 flex gap-2 text-xs text-muted">
                    {t.app_name && <span>{t.app_name}</span>}
                    {t.status === "paste_failed" && <span className="text-warning">Not inserted, copied instead</span>}
                    <span>{t.word_count} words</span>
                  </p>
                </div>
                <div className={cx("flex gap-1 opacity-0 group-hover:opacity-100 focus-within:opacity-100")}>
                  <IconButton label={copied === t.id ? "Copied" : "Copy"} onClick={() => copy(t)}><Copy className="size-4" /></IconButton>
                  <Menu
                    trigger={<IconButton label="More"><MoreHorizontal className="size-4" /></IconButton>}
                    items={[
                      { label: "Copy", onSelect: () => copy(t) },
                      // TODO: wire delete, retry and undo cleanup (backend commands exist)
                      { label: "Delete", onSelect: () => {}, danger: true, disabled: true },
                    ]}
                  />
                </div>
              </li>
            ))}
          </ul>
        </section>
      ))}
    </>
  );
}
