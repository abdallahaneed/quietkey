// Snippets: say a short cue, get the full text pasted (an address, a link, a signature).
import { useEffect, useMemo, useState } from "react";
import { Pencil, Plus, TextQuote, Trash2 } from "lucide-react";
import { api, type Snippet } from "../../lib/api";
import { Button, EmptyState, IconButton, Modal, Notice, PageHeader, SkeletonRows, TextArea, TextInput } from "../../ui";

type Editing = { id: string | null; trigger: string; expansion: string };
const MAX_EXPANSION = 4000;

export function Snippets() {
  const [rows, setRows] = useState<Snippet[] | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [editing, setEditing] = useState<Editing | null>(null);
  const [formError, setFormError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [confirm, setConfirm] = useState<Snippet | null>(null);

  const load = () => api.snippets().then((r) => { setRows(r); setLoadError(null); }).catch((e) => setLoadError(String(e)));
  useEffect(() => { load(); }, []);

  const shown = useMemo(() => {
    const q = query.trim().toLowerCase();
    return (rows ?? []).filter((r) => !q || r.trigger.toLowerCase().includes(q) || r.expansion.toLowerCase().includes(q));
  }, [rows, query]);

  const startAdd = () => { setFormError(null); setEditing({ id: null, trigger: "", expansion: "" }); };
  const tooLong = (editing?.expansion.length ?? 0) > MAX_EXPANSION;

  const save = async () => {
    if (!editing || tooLong) return;
    setSaving(true);
    setFormError(null);
    try {
      if (editing.id) await api.snippetUpdate(editing.id, editing.trigger, editing.expansion);
      else await api.snippetAdd(editing.trigger, editing.expansion);
      setEditing(null);
      await load();
    } catch (e) {
      setFormError(String(e));
    } finally {
      setSaving(false);
    }
  };

  const remove = async (s: Snippet) => {
    try {
      await api.snippetDelete([s.id]);
      await load();
    } catch (e) {
      setLoadError(`Could not remove: ${e}`);
    } finally {
      setConfirm(null);
    }
  };

  return (
    <>
      <PageHeader
        title="Snippets"
        description="Say a short cue while dictating and Quietkey types the full text instead."
        actions={<Button variant="primary" icon={<Plus className="size-4" />} onClick={startAdd}>Add snippet</Button>}
      />

      {loadError && <Notice tone="error" action={<Button size="sm" onClick={load}>Try again</Button>}>Could not load snippets: {loadError}</Notice>}
      {!rows && !loadError && <SkeletonRows />}

      {rows && rows.length === 0 && (
        <EmptyState
          icon={<TextQuote className="size-8" />}
          title="No snippets yet"
          body="Save text you type often, like your email address or a calendar link, under a short cue such as “my email”."
          action={<Button variant="primary" onClick={startAdd}>Create a snippet</Button>}
        />
      )}

      {rows && rows.length > 0 && (
        <>
          <TextInput label="Search snippets" hideLabel placeholder="Search…" value={query} onChange={(e) => setQuery(e.target.value)} className="mb-3 w-72" />
          {shown.length === 0 ? (
            <p className="py-8 text-center text-sm text-muted">No snippets match “{query}”.</p>
          ) : (
            <ul className="divide-y divide-border border-y border-border">
              {shown.map((s) => (
                <li key={s.id} className="flex items-start gap-4 py-3">
                  <div className="min-w-0 flex-1">
                    <p className="truncate font-medium" title={s.trigger}>“{s.trigger}”</p>
                    <p className="mt-0.5 line-clamp-2 whitespace-pre-line text-sm text-muted">{s.expansion}</p>
                  </div>
                  <div className="flex gap-1">
                    <IconButton label={`Edit ${s.trigger}`} onClick={() => { setFormError(null); setEditing({ id: s.id, trigger: s.trigger, expansion: s.expansion }); }}><Pencil className="size-4" /></IconButton>
                    <IconButton label={`Delete ${s.trigger}`} onClick={() => setConfirm(s)}><Trash2 className="size-4" /></IconButton>
                  </div>
                </li>
              ))}
            </ul>
          )}
        </>
      )}

      <Modal
        open={!!editing}
        onOpenChange={(o) => !o && setEditing(null)}
        title={editing?.id ? "Edit snippet" : "New snippet"}
        width="w-[520px]"
        footer={
          <>
            <Button onClick={() => setEditing(null)}>Cancel</Button>
            <Button variant="primary" loading={saving} disabled={!editing?.trigger.trim() || !editing?.expansion.trim() || tooLong} onClick={save}>
              {editing?.id ? "Save" : "Create"}
            </Button>
          </>
        }
      >
        {editing && (
          <div className="flex flex-col gap-3">
            {formError && <Notice tone="error">{formError}</Notice>}
            <TextInput label="What you say" autoFocus max={60} maxLength={60} value={editing.trigger}
              onChange={(e) => setEditing({ ...editing, trigger: e.target.value })} placeholder="e.g. my email" />
            <TextArea label="Text it becomes" max={MAX_EXPANSION} value={editing.expansion}
              error={tooLong ? `Keep it under ${MAX_EXPANSION.toLocaleString()} characters` : undefined}
              onChange={(e) => setEditing({ ...editing, expansion: e.target.value })} placeholder="e.g. name@example.com" />
          </div>
        )}
      </Modal>

      <Modal
        open={!!confirm}
        onOpenChange={(o) => !o && setConfirm(null)}
        title="Delete this snippet?"
        description={confirm ? `Saying “${confirm.trigger}” will be typed as is again.` : undefined}
        width="w-[400px]"
        footer={
          <>
            <Button autoFocus onClick={() => setConfirm(null)}>Keep</Button>
            <Button variant="danger" onClick={() => confirm && remove(confirm)}>Delete</Button>
          </>
        }
      />
    </>
  );
}
