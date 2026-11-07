// Settings: microphone, languages, theme, cleanup and free AI providers, shortcuts, system and privacy.
import { useEffect, useState, type ReactNode } from "react";
import { RefreshCw } from "lucide-react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { ShortcutEditor } from "../parts/ShortcutEditor";
import { api, applyTheme, pushToTalk, settingsUrl, type DeleteCategory, type Mic, type Permissions, type Settings as S } from "../../lib/api";
import { Button, Checkbox, IconButton, KeyCaps, Modal, Notice, PageHeader, RadioCards, SelectMenu, SkeletonRows, Switch, TextInput } from "../../ui";
import { SpeechEngine } from "../parts/SpeechEngine";

const LANGUAGES: [string, string][] = [
  ["en", "English"], ["de", "German"], ["ar", "Arabic"], ["fr", "French"], ["es", "Spanish"], ["tr", "Turkish"],
  ["it", "Italian"], ["pt", "Portuguese"], ["nl", "Dutch"], ["pl", "Polish"], ["ru", "Russian"], ["uk", "Ukrainian"],
  ["zh", "Chinese"], ["ja", "Japanese"], ["ko", "Korean"], ["hi", "Hindi"], ["ur", "Urdu"], ["fa", "Persian"],
];
const DEFAULT_MIC = "__default__";
const DELETE_OPTIONS: [DeleteCategory, string][] = [
  ["history", "Dictation history"], ["dictionary", "Dictionary"], ["snippets", "Snippets"],
  ["personalization", "Style choices"], ["usage", "Usage statistics"],
];

function Section({ id, title, description, children }: { id: string; title: string; description?: string; children: ReactNode }) {
  return (
    <section aria-labelledby={id} className="mb-10">
      <h2 id={id} className="text-lg font-semibold">{title}</h2>
      {description && <p className="mt-0.5 text-sm text-muted">{description}</p>}
      <div className="mt-4 flex flex-col gap-4">{children}</div>
    </section>
  );
}

function Row({ label, hint, children }: { label: string; hint?: string; children: ReactNode }) {
  return (
    <div className="flex items-start justify-between gap-6">
      <div>
        <p className="text-base font-medium">{label}</p>
        {hint && <p className="text-sm text-muted">{hint}</p>}
      </div>
      <div className="shrink-0">{children}</div>
    </div>
  );
}

export function Settings() {
  const [s, setS] = useState<S | null>(null);
  const [mics, setMics] = useState<Mic[]>([]);
  const [keySaved, setKeySaved] = useState(false);
  const [perm, setPerm] = useState<Permissions | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [groqKey, setGroqKey] = useState("");
  const [keyError, setKeyError] = useState<string | null>(null);
  const [test, setTest] = useState<{ running: boolean; result?: string; error?: string }>({ running: false });
  const [pendingStorage, setPendingStorage] = useState<S["storage_mode"] | null>(null);
  const [toDelete, setToDelete] = useState<Set<DeleteCategory>>(new Set());
  const [deleteOpen, setDeleteOpen] = useState(false);
  const [confirmText, setConfirmText] = useState("");
  const [deleteResult, setDeleteResult] = useState<string | null>(null);

  const load = async () => {
    try {
      const [settings, micList, key] = await Promise.all([api.settings(), api.mics(), api.groqKeyStatus()]);
      setS(settings); setMics(micList); setKeySaved(key.saved); setLoadError(null);
    } catch (e) {
      setLoadError(String(e));
    }
  };
  useEffect(() => { load(); api.permissions().then(setPerm).catch(() => {}); }, []);

  /** Optimistic update; the backend validates and the old value comes back if it refuses. */
  const update = async (patch: Partial<S>) => {
    if (!s) return;
    const before = s;
    setS({ ...s, ...patch });
    setError(null);
    try {
      const saved = await api.updateSettings(patch);
      setS(saved);
      if (patch.theme) applyTheme(saved.theme);
    } catch (e) {
      setS(before);
      setError(String(e));
    }
  };

  if (loadError) return (<><PageHeader title="Settings" /><Notice tone="error" action={<Button size="sm" onClick={load}>Try again</Button>}>Could not load settings: {loadError}</Notice></>);
  if (!s) return (<><PageHeader title="Settings" /><SkeletonRows n={5} /></>);

  const toggleLanguage = (code: string, on: boolean) => {
    const next = on ? [...s.languages, code] : s.languages.filter((l) => l !== code);
    if (next.length === 0 && !s.auto_detect_language) {
      setError("Keep at least one language, or turn on automatic detection.");
      return;
    }
    update({ languages: next });
  };

  const saveKey = async () => {
    setKeyError(null);
    try {
      await api.groqKeySet(groqKey);
      setGroqKey(""); setKeySaved(true);
    } catch (e) {
      setKeyError(String(e));
    }
  };

  const runTest = async () => {
    setTest({ running: true });
    try { setTest({ running: false, result: await api.cleanupTest() }); }
    catch (e) { setTest({ running: false, error: String(e) }); }
  };

  const chooseStorage = (mode: S["storage_mode"]) => {
    if (mode === s.storage_mode) return;
    if (mode === "store") update({ storage_mode: mode });
    else setPendingStorage(mode); // both other modes delete saved dictations, so ask first
  };

  const runDelete = async () => {
    try {
      const counts = await api.dataDelete([...toDelete], confirmText);
      const total = Object.values(counts).reduce((a, b) => a + b, 0);
      setDeleteResult(`Deleted ${total} ${total === 1 ? "item" : "items"}.`);
      setDeleteOpen(false); setConfirmText(""); setToDelete(new Set());
    } catch (e) {
      setError(String(e));
      setDeleteOpen(false);
    }
  };

  const multiLang = s.languages.length > 1 || s.auto_detect_language;

  return (
    <>
      <PageHeader title="Settings" description="Changes are saved as you make them." />
      {error && <div className="mb-6"><Notice tone="error" action={<Button size="sm" variant="ghost" onClick={() => setError(null)}>Dismiss</Button>}>{error}</Notice></div>}

      <Section id="general" title="General">
        <Row label="Microphone" hint="Quietkey falls back to the system default if this one is unplugged.">
          <div className="flex items-center gap-1">
            <SelectMenu
              label="Microphone"
              value={s.mic_id ?? DEFAULT_MIC}
              onChange={(v) => update({ mic_id: v === DEFAULT_MIC ? null : v })}
              options={[
                { value: DEFAULT_MIC, label: "System default" },
                ...mics.map((m) => ({ value: m.name, label: m.is_default ? `${m.name} (default)` : m.name })),
                ...(s.mic_id && !mics.some((m) => m.name === s.mic_id) ? [{ value: s.mic_id, label: `${s.mic_id} (not connected)` }] : []),
              ]}
            />
            <IconButton label="Refresh microphones" onClick={() => api.mics().then(setMics).catch((e) => setError(String(e)))}><RefreshCw className="size-4" /></IconButton>
          </div>
        </Row>
        <Row label="Theme">
          <SelectMenu label="Theme" value={s.theme} onChange={(v) => update({ theme: v })}
            options={[{ value: "system", label: "Match macOS" }, { value: "light", label: "Light" }, { value: "dark", label: "Dark" }]} />
        </Row>
        <div>
          <p className="text-base font-medium">Languages you speak</p>
          <p className="text-sm text-muted">
            {multiLang ? "Quietkey detects which of these you are speaking." : "Quietkey listens for this language only, which is the most accurate."}
          </p>
          <div className="mt-2 rounded-lg border border-border px-4">
            <Switch label="Detect the language automatically" description="Works for any language, slightly less accurate than picking one."
              checked={s.auto_detect_language}
              onChange={(v) => (!v && s.languages.length === 0 ? setError("Pick a language first, then turn detection off.") : update({ auto_detect_language: v }))} />
          </div>
          <fieldset className="mt-3 grid grid-cols-3 gap-x-4 gap-y-2">
            <legend className="sr-only">Languages</legend>
            {LANGUAGES.map(([code, name]) => (
              <Checkbox key={code} label={name} checked={s.languages.includes(code)} onChange={(on) => toggleLanguage(code, on)} />
            ))}
          </fieldset>
        </div>
      </Section>

      <Section id="model" title="Speech" description="Bigger local models hear names and accents better but use more memory.">
        <SpeechEngine settings={s} onSaved={setS} />
      </Section>

      <Section id="cleanup" title="Cleanup" description="What happens to the text between speaking and pasting.">
        <RadioCards label="Cleanup level" value={s.cleanup_level} onChange={(v) => update({ cleanup_level: v })} options={[
          { value: "none", title: "Off", body: "Paste exactly what Whisper heard." },
          { value: "light", title: "Light", body: "Remove “um” and repeated words, keep only the corrected part when you correct yourself." },
          { value: "medium", title: "Polish", body: "Also smooth the wording. Needs AI cleanup below; without it this works like Light." },
        ]} />
        <div>
          <p className="text-base font-medium">AI cleanup</p>
          <p className="text-sm text-muted">Optional and free. Fixes grammar and follows the tone you set for each kind of app. Only text is sent, never audio.</p>
          <div className="mt-3">
            <RadioCards label="AI cleanup provider" value={s.llm_provider} onChange={(v) => update({ llm_provider: v })} options={[
              { value: "none", title: "Off", body: "Only the built-in rules. Nothing leaves your Mac." },
              { value: "groq", title: "Groq", body: "Free API key from Groq, fast. The text is sent to Groq." },
              { value: "ollama", title: "Ollama", body: "A model running on this Mac. Free and private, needs Ollama installed." },
            ]} />
          </div>
        </div>
        {s.llm_provider === "groq" && (
          <div className="flex flex-col gap-3 rounded-lg bg-surface p-4">
            {keySaved ? (
              <Row label="Groq API key" hint="Saved in your macOS Keychain.">
                <Button variant="ghost" onClick={() => api.groqKeyDelete().then(() => setKeySaved(false))}>Remove key</Button>
              </Row>
            ) : (
              <form className="flex items-end gap-2" onSubmit={(e) => { e.preventDefault(); saveKey(); }}>
                <TextInput label="Groq API key" type="password" autoComplete="off" value={groqKey} onChange={(e) => setGroqKey(e.target.value)}
                  placeholder="gsk_…" error={keyError ?? undefined} className="flex-1" />
                <Button type="submit" variant="primary" disabled={!groqKey.trim()}>Save key</Button>
              </form>
            )}
            {!keySaved && (
              <p className="text-sm text-muted">
                No key yet?{" "}
                <button type="button" className="text-accent underline" onClick={() => openUrl("https://console.groq.com/keys")}>Create one free at console.groq.com</button>
              </p>
            )}
            <TextInput label="Model" defaultValue={s.groq_model} onBlur={(e) => e.target.value !== s.groq_model && update({ groq_model: e.target.value })} />
          </div>
        )}
        {s.llm_provider === "ollama" && (
          <div className="flex flex-col gap-3 rounded-lg bg-surface p-4">
            <TextInput label="Model" defaultValue={s.ollama_model} onBlur={(e) => e.target.value !== s.ollama_model && update({ ollama_model: e.target.value })} />
            <TextInput label="Address" defaultValue={s.ollama_url} onBlur={(e) => e.target.value !== s.ollama_url && update({ ollama_url: e.target.value })} />
          </div>
        )}
        <div className="flex items-start gap-3">
          <Button loading={test.running} onClick={runTest}>Try cleanup on a sample</Button>
          <div className="min-w-0 flex-1" aria-live="polite">
            {test.result && <p className="text-sm">{test.result}</p>}
            {test.error && <p className="text-sm text-danger">{test.error}</p>}
          </div>
        </div>
      </Section>

      <Section id="shortcuts" title="Shortcuts" description="Hold the dictation keys to talk. Press them twice quickly, or add Space, to keep talking hands-free.">
        <Row label="Hold to dictate" hint="Modifier keys only, so nothing gets typed while you hold them.">
          <ShortcutEditor settings={s} onSaved={setS} />
        </Row>
        {perm?.fn_key_conflict && pushToTalk(s).includes("Fn") && (
          <Notice tone="warning" action={<Button size="sm" onClick={() => openUrl(settingsUrl.keyboard)}>Open Keyboard settings</Button>}>
            macOS also uses fn for dictation, emoji or switching input. Set "Press fn key to" to "Do Nothing", or pick other keys.
          </Notice>
        )}
        {perm && perm.other_dictation_apps.length > 0 && (
          <Notice tone="warning">
            {perm.other_dictation_apps.join(" and ")} {perm.other_dictation_apps.length === 1 ? "is" : "are"} running. If {perm.other_dictation_apps.length === 1 ? "it listens" : "they listen"} for the same keys, both apps will record and paste. Change the keys in one of them, or quit it.
          </Notice>
        )}
        {([["Hands-free on / off", [...pushToTalk(s), "Space"]], ["Cancel", ["Escape"]], ["Paste last dictation again", ["Ctrl", "Cmd", "V"]], ["Copy last dictation", ["Ctrl", "Cmd", "C"]]] as [string, string[]][]).map(([label, keys]) => (
          <Row key={label} label={label}><KeyCaps keys={keys} /></Row>
        ))}
      </Section>

      <Section id="system" title="System">
        <div className="rounded-lg border border-border px-4">
          <Switch label="Start Quietkey when you log in" checked={s.launch_at_login} onChange={(v) => update({ launch_at_login: v })} />
          <Switch label="Show the dictation bar" description="When it is hidden, Quietkey still listens for your shortcut." checked={s.show_bar} onChange={(v) => update({ show_bar: v })} />
        </div>
      </Section>

      <Section id="privacy" title="Privacy" description={s.stt_engine === "groq"
        ? "Your recordings are sent to Groq with your own key to be turned into text, and nowhere else. Quietkey never takes screenshots."
        : s.llm_provider === "groq"
        ? "Your voice is transcribed on this Mac and never uploaded. With Groq on, the transcribed text of each dictation is sent to Groq for cleanup. Quietkey never takes screenshots."
        : "Your voice and text stay on this Mac. Quietkey makes no network calls for dictation and never takes screenshots."}>
        <RadioCards label="Dictation history" value={s.storage_mode} onChange={chooseStorage} options={[
          { value: "store", title: "Keep history", body: "Your dictations stay on this Mac until you delete them." },
          { value: "auto_delete_24h", title: "Delete after 24 hours", body: "Each dictation is removed a day after you make it." },
          { value: "never", title: "Don't keep history", body: "Nothing is saved. ⌃⌘V still works until you quit." },
        ]} />
        <div className="rounded-lg border border-border p-4">
          <p className="text-base font-medium">Delete data</p>
          <fieldset className="mt-2 grid grid-cols-2 gap-2">
            <legend className="sr-only">Data to delete</legend>
            {DELETE_OPTIONS.map(([key, label]) => (
              <Checkbox key={key} label={label} checked={toDelete.has(key)}
                onChange={(on) => setToDelete((d) => { const n = new Set(d); on ? n.add(key) : n.delete(key); return n; })} />
            ))}
          </fieldset>
          <div className="mt-3 flex items-center gap-3">
            <Button variant="danger" disabled={toDelete.size === 0} onClick={() => { setConfirmText(""); setDeleteOpen(true); }}>Delete selected…</Button>
            {deleteResult && <span className="text-sm text-muted" role="status">{deleteResult}</span>}
          </div>
        </div>
      </Section>

      <Modal
        open={!!pendingStorage}
        onOpenChange={(o) => !o && setPendingStorage(null)}
        title={pendingStorage === "never" ? "Stop keeping history?" : "Delete history after 24 hours?"}
        description={pendingStorage === "never" ? "All saved dictations are deleted now. This cannot be undone." : "Dictations older than 24 hours are deleted now. This cannot be undone."}
        width="w-[420px]"
        footer={
          <>
            <Button autoFocus onClick={() => setPendingStorage(null)}>Cancel</Button>
            <Button variant="danger" onClick={() => { const m = pendingStorage; setPendingStorage(null); if (m) update({ storage_mode: m }); }}>Delete and switch</Button>
          </>
        }
      />

      <Modal
        open={deleteOpen}
        onOpenChange={setDeleteOpen}
        title="Delete this data?"
        description={`${[...toDelete].map((k) => DELETE_OPTIONS.find(([key]) => key === k)?.[1]).join(", ")} will be deleted for good.`}
        width="w-[420px]"
        footer={
          <>
            <Button autoFocus onClick={() => setDeleteOpen(false)}>Cancel</Button>
            <Button variant="danger" disabled={confirmText !== "DELETE"} onClick={runDelete}>Delete</Button>
          </>
        }
      >
        <TextInput label="Type DELETE to confirm" value={confirmText} onChange={(e) => setConfirmText(e.target.value)} autoComplete="off" />
      </Modal>
    </>
  );
}
