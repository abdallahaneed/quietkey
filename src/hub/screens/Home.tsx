// Home: dictation history grouped by day, newest first, live-updating.
import { useEffect, useMemo, useState } from "react";
import { Copy, Mic, MoreHorizontal } from "lucide-react";
import { api, events, pushToTalk, type Permissions, type Transcript } from "../../lib/api";
import { EmptyState, IconButton, KeyCaps, Menu, Modal, Notice, PageHeader, SkeletonRows, Button, TextInput, cx } from "../../ui";
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
  const [showRaw, setShowRaw] = useState<Set<string>>(new Set());
  const [confirmDelete, setConfirmDelete] = useState<Transcript | null>(null);
  const [keys, setKeys] = useState<string[]>(["Fn"]);
  // "Add to dictionary" from an entry: a name Whisper got wrong is fixed where the user sees it.
  const [adding, setAdding] = useState<{ word: string; heard: string } | null>(null);
  const [addError, setAddError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const openAdd = () => {
    // Text selected in the entry is usually the misheard word: start both fields from it.
    const sel = (window.getSelection()?.toString() ?? "").trim().slice(0, 60);
    setAddError(null);
    setAdding({ word: sel, heard: sel });
  };
  const saveWord = async () => {
    if (!adding?.word.trim()) return;
    const heard = adding.heard.trim() && adding.heard.trim() !== adding.word.trim() ? adding.heard : null;
    try {
      await api.dictionaryAdd(adding.word, heard);
      setAdding(null);
      setNotice(`"${adding.word.trim()}" added to your dictionary.`);
    } catch (e) {
      setAddError(String(e));
    }
  };
  useEffect(() => { api.settings().then((s) => setKeys(pushToTalk(s))).catch(() => {}); }, []);

  const remove = async (t: Transcript) => {
    setConfirmDelete(null);
    try {
      await api.transcriptDelete(t.id);
      setRows((r) => (r ?? []).filter((x) => x.id !== t.id));
    } catch (e) {
      setError(`Could not delete: ${e}`);
    }
  };

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

  const toggleRaw = (id: string) => setShowRaw((s) => { const n = new Set(s); n.has(id) ? n.delete(id) : n.add(id); return n; });
  // Cleanup changed something worth showing (not just spacing or a capital letter).
  const edited = (t: Transcript) => t.raw_text.trim().toLowerCase() !== t.final_text.trim().toLowerCase();

  const copy = async (t: Transcript, raw = false) => {
    await api.transcriptCopy(t.id, raw);
    setCopied(t.id);
    setTimeout(() => setCopied(null), 1500);
  };

  const needsSetup = perm && !(perm.accessibility && perm.hotkey_running && !(perm.fn_key_conflict && keys.includes("Fn")));

  return (
    <>
      <PageHeader
        title="Your dictations"
        description={rows ? `${weekWords.toLocaleString()} words in the last 7 days` : undefined}
        actions={<span className="flex items-center gap-2 text-sm text-muted">Hold <KeyCaps keys={keys} /> and talk</span>}
      />
      {needsSetup && (
        <div className="mb-4">
          <Notice tone="warning" action={<Button size="sm" onClick={() => nav("setup")}>Finish setup</Button>}>
            Dictation needs two Mac permissions before it can type for you.
          </Notice>
        </div>
      )}
      {notice && <div className="mb-4"><Notice tone="success">{notice}</Notice></div>}
      {error && <Notice tone="error">{error.startsWith("Could not") ? error : `Could not load history: ${error}`}</Notice>}
      {!rows && !error && <SkeletonRows />}
      {rows && rows.length === 0 && (
        <EmptyState
          icon={<Mic className="size-8" />}
          title="Nothing dictated yet"
          body="Click into any text field, hold your dictation keys, say a sentence, then let go. It shows up here too."
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
                  {showRaw.has(t.id) && (
                    <p className="mt-1 select-text border-l-2 border-border pl-2 text-sm text-muted">
                      <span className="sr-only">What you said: </span>{t.raw_text}
                    </p>
                  )}
                  <p className="mt-1 flex gap-2 text-xs text-muted">
                    {t.app_name && <span>{t.app_name}</span>}
                    {t.status === "paste_failed" && <span className="text-warning">Not inserted, copied instead</span>}
                    <span>{t.word_count} words</span>
                    {edited(t) && (
                      <button type="button" className="underline hover:text-fg" aria-expanded={showRaw.has(t.id)} onClick={() => toggleRaw(t.id)}>
                        {showRaw.has(t.id) ? "Hide what you said" : "Show what you said"}
                      </button>
                    )}
                  </p>
                </div>
                <div className={cx("flex gap-1 opacity-0 group-hover:opacity-100 focus-within:opacity-100")}>
                  <IconButton label={copied === t.id ? "Copied" : "Copy"} onClick={() => copy(t)}><Copy className="size-4" /></IconButton>
                  <Menu
                    trigger={<IconButton label="More"><MoreHorizontal className="size-4" /></IconButton>}
                    items={[
                      { label: "Copy", onSelect: () => copy(t) },
                      ...(edited(t) ? [{ label: "Copy what you said", onSelect: () => copy(t, true) }] : []),
                      { label: "Add to dictionary", onSelect: openAdd },
                      { label: "Delete", onSelect: () => setConfirmDelete(t), danger: true },
                    ]}
                  />
                </div>
              </li>
            ))}
          </ul>
        </section>
      ))}
      <Modal
        open={!!adding}
        onOpenChange={(o) => !o && setAdding(null)}
        title="Add to dictionary"
        description="Whisper gets this spelling right from now on."
        width="w-[400px]"
        footer={
          <>
            <Button onClick={() => setAdding(null)}>Cancel</Button>
            <Button variant="primary" disabled={!adding?.word.trim()} onClick={saveWord}>Add</Button>
          </>
        }
      >
        {adding && (
          <form className="flex flex-col gap-2" onSubmit={(e) => { e.preventDefault(); saveWord(); }}>
            {addError && <Notice tone="error">{addError}</Notice>}
            <TextInput label="Correct spelling" autoFocus maxLength={60} value={adding.word} onChange={(e) => setAdding({ ...adding, word: e.target.value })} placeholder="e.g. Aneed" />
            <TextInput label="Whisper wrote (optional)" maxLength={60} value={adding.heard} onChange={(e) => setAdding({ ...adding, heard: e.target.value })} placeholder="e.g. a need" />
            <button type="submit" hidden />
          </form>
        )}
      </Modal>
      <Modal
        open={!!confirmDelete}
        onOpenChange={(o) => !o && setConfirmDelete(null)}
        title="Delete this dictation?"
        description="It is removed from your history on this Mac."
        width="w-[400px]"
        footer={
          <>
            <Button autoFocus onClick={() => setConfirmDelete(null)}>Keep</Button>
            <Button variant="danger" onClick={() => confirmDelete && remove(confirmDelete)}>Delete</Button>
          </>
        }
      />
    </>
  );
}
