// Dictionary: words Whisper should spell your way, plus "heard as -> write as" corrections.
import { useEffect, useMemo, useRef, useState } from "react";
import { BookA, Pencil, Plus, Trash2, Upload } from "lucide-react";
import { api, type DictEntry } from "../../lib/api";
import { Button, Checkbox, EmptyState, IconButton, Modal, Notice, PageHeader, SkeletonRows, Switch, TextInput } from "../../ui";

type Editing = { id: string | null; word: string; fix: boolean; heard: string };

export function Dictionary() {
  const [rows, setRows] = useState<DictEntry[] | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [editing, setEditing] = useState<Editing | null>(null);
  const [formError, setFormError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [confirmIds, setConfirmIds] = useState<string[] | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const fileInput = useRef<HTMLInputElement>(null);

  const load = () => api.dictionary().then((r) => { setRows(r); setLoadError(null); }).catch((e) => setLoadError(String(e)));
  useEffect(() => { load(); }, []);

  const shown = useMemo(() => {
    const q = query.trim().toLowerCase();
    return (rows ?? []).filter((r) => !q || r.word.toLowerCase().includes(q) || (r.misspelling ?? "").toLowerCase().includes(q));
  }, [rows, query]);

  const save = async () => {
    if (!editing) return;
    setSaving(true);
    setFormError(null);
    const heard = editing.fix && editing.heard.trim() ? editing.heard : null;
    try {
      if (editing.id) await api.dictionaryUpdate(editing.id, editing.word, heard);
      else await api.dictionaryAdd(editing.word, heard);
      setEditing(null);
      await load();
    } catch (e) {
      setFormError(String(e));
    } finally {
      setSaving(false);
    }
  };

  const remove = async (ids: string[]) => {
    try {
      await api.dictionaryDelete(ids);
      setSelected(new Set());
      setNotice(ids.length === 1 ? "Word removed." : `${ids.length} words removed.`);
      await load();
    } catch (e) {
      setNotice(`Could not remove: ${e}`);
    } finally {
      setConfirmIds(null);
    }
  };

  const importFile = async (file: File) => {
    try {
      const res = await api.dictionaryImport(await file.text());
      setNotice(`Imported ${res.added} ${res.added === 1 ? "word" : "words"}${res.skipped ? `, skipped ${res.skipped} (duplicates or invalid)` : ""}.`);
      await load();
    } catch (e) {
      setNotice(`Import failed: ${e}`);
    }
  };

  const toggle = (id: string, on: boolean) => setSelected((s) => { const n = new Set(s); on ? n.add(id) : n.delete(id); return n; });
  const allShownSelected = shown.length > 0 && shown.every((r) => selected.has(r.id));
  const startAdd = () => { setFormError(null); setEditing({ id: null, word: "", fix: false, heard: "" }); };

  return (
    <>
      <PageHeader
        title="Dictionary"
        description="Names and terms Quietkey should spell your way. Add a correction when it keeps hearing a word wrong: with Parakeet only corrections apply."
        actions={
          <>
            <input ref={fileInput} type="file" accept=".txt,.csv,text/plain" className="sr-only" aria-label="Import file" tabIndex={-1}
              onChange={(e) => { const f = e.target.files?.[0]; if (f) importFile(f); e.target.value = ""; }} />
            <Button icon={<Upload className="size-4" />} onClick={() => fileInput.current?.click()}>Import</Button>
            <Button variant="primary" icon={<Plus className="size-4" />} onClick={startAdd}>Add word</Button>
          </>
        }
      />

      {notice && <div className="mb-4"><Notice action={<Button size="sm" variant="ghost" onClick={() => setNotice(null)}>Dismiss</Button>}>{notice}</Notice></div>}
      {loadError && <Notice tone="error" action={<Button size="sm" onClick={load}>Try again</Button>}>Could not load the dictionary: {loadError}</Notice>}
      {!rows && !loadError && <SkeletonRows />}

      {rows && rows.length === 0 && (
        <EmptyState
          icon={<BookA className="size-8" />}
          title="No words yet"
          body="Add names, product terms or anything Quietkey misspells. A text file with one word per line can be imported too."
          action={<Button variant="primary" onClick={startAdd}>Add your first word</Button>}
        />
      )}

      {rows && rows.length > 0 && (
        <>
          <div className="mb-3 flex items-center justify-between gap-4">
            <TextInput label="Search words" hideLabel placeholder="Search…" value={query} onChange={(e) => setQuery(e.target.value)} className="w-72" />
            {selected.size > 0 && (
              <div className="flex items-center gap-3 text-sm">
                <span>{selected.size} selected</span>
                <Button size="sm" variant="danger" onClick={() => setConfirmIds([...selected])}>Delete selected</Button>
              </div>
            )}
          </div>
          {shown.length === 0 ? (
            <p className="py-8 text-center text-sm text-muted">No words match “{query}”.</p>
          ) : (
            <table className="w-full table-fixed border-collapse text-left">
              <caption className="sr-only">Dictionary words</caption>
              <thead>
                <tr className="border-b border-border text-xs text-muted">
                  <th className="w-8 py-2"><Checkbox label="Select all" hideLabel checked={allShownSelected} onChange={(on) => setSelected(on ? new Set(shown.map((r) => r.id)) : new Set())} /></th>
                  <th className="py-2 font-medium">Word</th>
                  <th className="py-2 font-medium">Correction</th>
                  <th className="w-20 py-2"><span className="sr-only">Actions</span></th>
                </tr>
              </thead>
              <tbody>
                {shown.map((r) => (
                  <tr key={r.id} className={selected.has(r.id) ? "border-b border-border bg-accent-soft" : "border-b border-border"}>
                    <td className="py-2.5"><Checkbox label={`Select ${r.word}`} hideLabel checked={selected.has(r.id)} onChange={(on) => toggle(r.id, on)} /></td>
                    <td className="truncate py-2.5 pr-3 font-medium" title={r.word}>{r.word}</td>
                    <td className="truncate py-2.5 pr-3 text-sm text-muted" title={r.misspelling ?? ""}>{r.misspelling ? `Heard as “${r.misspelling}”` : "—"}</td>
                    <td className="py-2.5">
                      <div className="flex justify-end gap-1">
                        <IconButton label={`Edit ${r.word}`} onClick={() => { setFormError(null); setEditing({ id: r.id, word: r.word, fix: !!r.misspelling, heard: r.misspelling ?? "" }); }}><Pencil className="size-4" /></IconButton>
                        <IconButton label={`Delete ${r.word}`} onClick={() => setConfirmIds([r.id])}><Trash2 className="size-4" /></IconButton>
                      </div>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </>
      )}

      <Modal
        open={!!editing}
        onOpenChange={(o) => !o && setEditing(null)}
        title={editing?.id ? "Edit word" : "Add a word"}
        footer={
          <>
            <Button onClick={() => setEditing(null)}>Cancel</Button>
            <Button variant="primary" loading={saving} disabled={!editing?.word.trim()} onClick={save}>{editing?.id ? "Save" : "Add"}</Button>
          </>
        }
      >
        {editing && (
          <form className="flex flex-col gap-2" onSubmit={(e) => { e.preventDefault(); save(); }}>
            {formError && <Notice tone="error">{formError}</Notice>}
            <TextInput label="Word" autoFocus max={60} maxLength={60} value={editing.word} onChange={(e) => setEditing({ ...editing, word: e.target.value })} placeholder="e.g. Aneed" />
            <Switch label="Fix a word Quietkey hears wrong" description="Replace what Quietkey writes with the word above." checked={editing.fix} onChange={(v) => setEditing({ ...editing, fix: v })} />
            {editing.fix && (
              <TextInput label="Quietkey writes" max={60} maxLength={60} value={editing.heard} onChange={(e) => setEditing({ ...editing, heard: e.target.value })} placeholder="e.g. a need" />
            )}
            <button type="submit" hidden />
          </form>
        )}
      </Modal>

      <Modal
        open={!!confirmIds}
        onOpenChange={(o) => !o && setConfirmIds(null)}
        title={confirmIds && confirmIds.length > 1 ? `Remove ${confirmIds.length} words?` : "Remove this word?"}
        description="Quietkey will stop using it for spelling."
        width="w-[400px]"
        footer={
          <>
            <Button autoFocus onClick={() => setConfirmIds(null)}>Keep</Button>
            <Button variant="danger" onClick={() => confirmIds && remove(confirmIds)}>Remove</Button>
          </>
        }
      />
    </>
  );
}
