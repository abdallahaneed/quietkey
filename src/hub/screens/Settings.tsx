// Settings as grouped cards: shortcuts, languages, sound, speech, cleanup, app and privacy. Each row: label, (i), control,
// and a reset arrow where the setting has a default.
import { useEffect, useState } from "react";
import { RefreshCw } from "lucide-react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { ShortcutEditor } from "../parts/ShortcutEditor";
import { api, applyTheme, pushToTalk, settingsUrl, type DeleteCategory, type Mic, type Permissions, type Settings as S } from "../../lib/api";
import { Button, Checkbox, Group, IconButton, KeyCaps, Modal, Notice, PageHeader, Row, SelectMenu, SkeletonRows, TextInput, Toggle } from "../../ui";
import type { Nav } from "../Hub";
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

export function Settings({ nav }: { nav: Nav }) {
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

  const fnKeys = pushToTalk(s);
  return (
    <>
      <PageHeader title="Settings" description="Changes are saved as you make them." />
      {error && <div className="mb-6"><Notice tone="error" action={<Button size="sm" variant="ghost" onClick={() => setError(null)}>Dismiss</Button>}>{error}</Notice></div>}

      <Group id="shortcuts" title="Shortcuts">
        <Row label="Hold to dictate" info="Hold these keys to talk, let go to insert. Press them twice quickly, or add Space, to keep talking hands-free. Modifier keys only, so nothing gets typed while you hold them."
          onReset={() => update({ shortcuts: { ...s.shortcuts, push_to_talk: [["Fn"]], hands_free: [["Fn", "Space"]] } })} isDefault={fnKeys.join() === "Fn"}>
          <ShortcutEditor settings={s} onSaved={setS} />
        </Row>
        {([["Hands-free on / off", [...fnKeys, "Space"], "Starts dictation without holding. Press again to insert."],
           ["Cancel", ["Escape"], "Throws the recording away. Undo brings it back for a few seconds."],
           ["Paste last dictation again", ["Ctrl", "Cmd", "V"], "Inserts your last dictation where the cursor is."],
           ["Copy last dictation", ["Ctrl", "Cmd", "C"], "Puts your last dictation on the clipboard."]] as [string, string[], string][]).map(([label, keys, info]) => (
          <Row key={label} label={label} info={info}><KeyCaps keys={keys} /></Row>
        ))}
      </Group>
      {perm?.fn_key_conflict && fnKeys.includes("Fn") && (
        <div className="-mt-4 mb-6"><Notice tone="warning" action={<Button size="sm" onClick={() => openUrl(settingsUrl.keyboard)}>Open Keyboard settings</Button>}>
          macOS also uses fn for dictation, emoji or switching input. Set "Press fn key to" to "Do Nothing", or pick other keys.
        </Notice></div>
      )}
      {perm && perm.other_dictation_apps.length > 0 && (
        <div className="-mt-4 mb-6"><Notice tone="warning">
          {perm.other_dictation_apps.join(" and ")} {perm.other_dictation_apps.length === 1 ? "is" : "are"} running. If {perm.other_dictation_apps.length === 1 ? "it listens" : "they listen"} for the same keys, both apps will record and paste. Change the keys in one of them, or quit it.
        </Notice></div>
      )}

      <Group id="languages" title="Languages" description={multiLang ? "Quietkey detects which of these you are speaking." : "Quietkey listens for this language only, which is the most accurate."}>
        <Row label="Detect the language automatically" info="Works for any language, slightly less accurate than picking one.">
          <Toggle label="Detect the language automatically" checked={s.auto_detect_language}
            onChange={(v) => (!v && s.languages.length === 0 ? setError("Pick a language first, then turn detection off.") : update({ auto_detect_language: v }))} />
        </Row>
        <Row label="Languages you speak" info="Models are graded for these on the Models page." below={
          <fieldset className="grid grid-cols-3 gap-x-4 gap-y-2">
            <legend className="sr-only">Languages</legend>
            {LANGUAGES.map(([code, name]) => (
              <Checkbox key={code} label={name} checked={s.languages.includes(code)} onChange={(on) => toggleLanguage(code, on)} />
            ))}
          </fieldset>
        } />
      </Group>

      <Group id="sound" title="Sound">
        <Row label="Microphone" info="Quietkey falls back to the system default if this one is unplugged." onReset={() => update({ mic_id: null })} isDefault={s.mic_id === null}>
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
        </Row>
        <Row label="Start and stop sounds" info="A short sound when recording starts and when it stops, so you know Quietkey heard the shortcut.">
          <Toggle label="Start and stop sounds" checked={s.sounds} onChange={(v) => update({ sounds: v })} />
        </Row>
      </Group>

      <Group id="model" title="Speech" action={<Button size="sm" variant="ghost" onClick={() => nav("models")}>Choose model…</Button>}>
        <div className="p-4"><SpeechEngine settings={s} onSaved={setS} picker={false} /></div>
      </Group>

      <Group id="live" title="Live text" description={s.stt_engine === "groq"
        ? "Live text needs a speech model on this Mac. With Groq the text appears when you let go."
        : "Your words appear in the text field while you talk. When you let go, the cleaned-up text replaces them."}>
        <Row label="Show words while you talk" info="On: Quietkey types what it hears into the text field as you speak and corrects words as it hears more. Off: nothing appears until you let go, then the whole text is pasted at once. Esc removes live text, and it only goes into the app you started in.">
          <Toggle label="Show words while you talk" checked={s.live_text} disabled={s.stt_engine === "groq"} onChange={(v) => update({ live_text: v })} />
        </Row>
        <Row label="Stop live text after 30 seconds" info="For long dictations. Nothing you say is lost either way: Quietkey keeps recording, only the live words stop updating."
          below={<div className="mt-1 space-y-1 text-sm text-muted">
            <p>Off: the words keep updating for the whole dictation. Each update reads everything you said so far, so a long dictation keeps your Mac busier and the final text can take a few seconds longer after you let go. You notice it most with Whisper Small and Large v3 Turbo, hardly with Parakeet.</p>
            <p>On: after 30 seconds the live words stop, the rest appears when you let go. Long dictations finish sooner and use less battery.</p>
          </div>}>
          <Toggle label="Stop live text after 30 seconds" checked={s.live_text_limit} disabled={!s.live_text || s.stt_engine === "groq"} onChange={(v) => update({ live_text_limit: v })} />
        </Row>
      </Group>

      <Group id="cleanup" title="Cleanup" description="What happens to the text between speaking and pasting.">
        <Row label="Cleanup level" info="Off pastes exactly what was heard. Light removes “um” and repeated words and keeps only the corrected part when you correct yourself. Polish also smooths the wording, which needs AI cleanup; without it Polish works like Light."
          onReset={() => update({ cleanup_level: "light" })} isDefault={s.cleanup_level === "light"}>
          <SelectMenu label="Cleanup level" value={s.cleanup_level} onChange={(v) => update({ cleanup_level: v })}
            options={[{ value: "none", label: "Off" }, { value: "light", label: "Light" }, { value: "medium", label: "Polish" }]} />
        </Row>
        <Row label="AI cleanup" info="Optional and free. Fixes grammar and follows the tone you set for each kind of app. Only text is sent, never audio. Groq needs a free API key and receives the text; Ollama runs a model on this Mac."
          onReset={() => update({ llm_provider: "none" })} isDefault={s.llm_provider === "none"}
          below={<>
            {s.llm_provider === "groq" && (
              <div className="flex flex-col gap-3 rounded-md bg-bg p-3">
                {keySaved ? (
                  <div className="flex items-center justify-between gap-4">
                    <div><p className="text-sm font-medium">Groq API key</p><p className="text-sm text-muted">Saved in your macOS Keychain.</p></div>
                    <Button variant="ghost" onClick={() => api.groqKeyDelete().then(() => setKeySaved(false))}>Remove key</Button>
                  </div>
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
              <div className="flex flex-col gap-3 rounded-md bg-bg p-3">
                <TextInput label="Model" defaultValue={s.ollama_model} onBlur={(e) => e.target.value !== s.ollama_model && update({ ollama_model: e.target.value })} />
                <TextInput label="Address" defaultValue={s.ollama_url} onBlur={(e) => e.target.value !== s.ollama_url && update({ ollama_url: e.target.value })} />
              </div>
            )}
          </>}>
          <SelectMenu label="AI cleanup provider" value={s.llm_provider} onChange={(v) => update({ llm_provider: v })}
            options={[{ value: "none", label: "Off" }, { value: "groq", label: "Groq" }, { value: "ollama", label: "Ollama" }]} />
        </Row>
        <Row label="Try it" info="Runs a sample sentence through your current cleanup settings." below={
          (test.result || test.error) && <div aria-live="polite">
            {test.result && <p className="text-sm">{test.result}</p>}
            {test.error && <p className="text-sm text-danger">{test.error}</p>}
          </div>
        }>
          <Button size="sm" loading={test.running} onClick={runTest}>Try cleanup on a sample</Button>
        </Row>
      </Group>

      <Group id="system" title="App">
        <Row label="Theme" onReset={() => update({ theme: "system" })} isDefault={s.theme === "system"}>
          <SelectMenu label="Theme" value={s.theme} onChange={(v) => update({ theme: v })}
            options={[{ value: "system", label: "Match macOS" }, { value: "light", label: "Light" }, { value: "dark", label: "Dark" }]} />
        </Row>
        <Row label="Start Quietkey when you log in">
          <Toggle label="Start Quietkey when you log in" checked={s.launch_at_login} onChange={(v) => update({ launch_at_login: v })} />
        </Row>
        <Row label="Show the dictation bar" info="When it is hidden, Quietkey still listens for your shortcut.">
          <Toggle label="Show the dictation bar" checked={s.show_bar} onChange={(v) => update({ show_bar: v })} />
        </Row>
      </Group>

      <Group id="privacy" title="Privacy" description={s.stt_engine === "groq"
        ? "Your recordings are sent to Groq with your own key to be turned into text, and nowhere else. Quietkey never takes screenshots."
        : s.llm_provider === "groq"
        ? "Your voice is transcribed on this Mac and never uploaded. With Groq on, the transcribed text of each dictation is sent to Groq for cleanup. Quietkey never takes screenshots."
        : "Your voice and text stay on this Mac. Quietkey makes no network calls for dictation and never takes screenshots."}>
        <Row label="Dictation history" info="Keep: dictations and their recordings stay on this Mac until you delete them. Delete after 24 hours: each one is removed a day later, starred ones stay. Don't keep: nothing is saved, ⌃⌘V still works until you quit."
          onReset={() => chooseStorage("store")} isDefault={s.storage_mode === "store"}>
          <SelectMenu label="Dictation history" value={s.storage_mode} onChange={chooseStorage}
            options={[{ value: "store", label: "Keep history" }, { value: "auto_delete_24h", label: "Delete after 24 hours" }, { value: "never", label: "Don't keep history" }]} />
        </Row>
        <Row label="Delete data" below={<>
          <fieldset className="grid grid-cols-2 gap-2">
            <legend className="sr-only">Data to delete</legend>
            {DELETE_OPTIONS.map(([key, label]) => (
              <Checkbox key={key} label={label} checked={toDelete.has(key)}
                onChange={(on) => setToDelete((d) => { const n = new Set(d); on ? n.add(key) : n.delete(key); return n; })} />
            ))}
          </fieldset>
          {deleteResult && <p className="mt-2 text-sm text-muted" role="status">{deleteResult}</p>}
        </>}>
          <Button size="sm" variant="danger" disabled={toDelete.size === 0} onClick={() => { setConfirmText(""); setDeleteOpen(true); }}>Delete selected…</Button>
        </Row>
      </Group>

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
