// Stands in for the Rust side so the screens can be tested in a plain browser.
// Mirrors window.__TAURI_INTERNALS__ as used by @tauri-apps/api 2.x (invoke, transformCallback, events).
import type { Page } from "@playwright/test";

export type FakeState = {
  transcripts?: unknown[];
  permissions?: { accessibility: boolean; input_monitoring: boolean; hotkey_running: boolean; fn_key_conflict: boolean; microphone?: string; other_dictation_apps?: string[] };
  failList?: boolean;
  dictionary?: { id: string; word: string; misspelling: string | null; source: string; created_at: number }[];
  snippets?: { id: string; trigger: string; expansion: string; updated_at: number }[];
  settings?: Record<string, unknown>;
  mics?: { name: string; is_default: boolean }[];
  groqKeySaved?: boolean;
  styles?: { category: string; tone: string }[];
  failStyles?: boolean;
};

export async function installFakeBackend(page: Page, state: FakeState = {}) {
  await page.addInitScript((s: FakeState) => {
    const callbacks = new Map<number, (payload: unknown) => void>();
    const listeners = new Map<string, number[]>();
    let nextId = 1;
    const calls: { cmd: string; args: unknown }[] = [];
    const permissions = { other_dictation_apps: [], ...(s.permissions ?? { accessibility: true, input_monitoring: true, hotkey_running: true, fn_key_conflict: false, microphone: "granted" }) };
    // In-memory data with the same rules as the Rust side (lengths, case-insensitive duplicates).
    const uid = () => Math.random().toString(36).slice(2);
    const clean = (v: string | null | undefined) => (v ?? "").trim();
    const dict = [...(s.dictionary ?? [])];
    const snips = [...(s.snippets ?? [])];
    const transcripts = [...((s.transcripts as any[]) ?? [])];
    const styles = (s.styles ?? ["personal", "work", "email", "other"].map((category) => ({ category, tone: "formal" }))).map((x) => ({ ...x }));
    const settings: Record<string, unknown> = {
      shortcuts: { push_to_talk: [["Fn"]] }, mic_id: null, languages: ["en"], auto_detect_language: false, theme: "system",
      launch_at_login: true, show_bar: true, storage_mode: "store", cleanup_level: "light", llm_provider: "none",
      groq_model: "llama-3.3-70b-versatile", ollama_model: "llama3.2:3b", ollama_url: "http://127.0.0.1:11434",
      sounds: true, onboarding_done: false, ...(s.settings ?? {}),
    };
    let groqKey = s.groqKeySaved ?? false;
    const dupWord = (w: string, m: string | null, skip?: string) =>
      dict.some((d) => d.id !== skip && d.word.toLowerCase() === w.toLowerCase() && (d.misspelling ?? "").toLowerCase() === (m ?? "").toLowerCase());
    const handlers: Record<string, (args: any) => unknown> = {
      dictionary_list: () => [...dict].sort((a, b) => a.word.localeCompare(b.word)),
      dictionary_add: (a: { word: string; misspelling: string | null }) => {
        const w = clean(a.word), m = clean(a.misspelling) || null;
        if (!w) throw "Enter a word";
        if (w.length > 60 || (m && m.length > 60)) throw "That word is too long or empty";
        if (dupWord(w, m)) throw "That word already exists";
        const id = uid(); dict.push({ id, word: w, misspelling: m, source: "manual", created_at: Date.now() }); return id;
      },
      dictionary_update: (a: { id: string; word: string; misspelling: string | null }) => {
        const d = dict.find((x) => x.id === a.id); if (!d) throw "That word no longer exists";
        const w = clean(a.word), m = clean(a.misspelling) || null;
        if (dupWord(w, m, a.id)) throw "That word already exists";
        d.word = w; d.misspelling = m; return null;
      },
      dictionary_delete: (a: { ids: string[] }) => { const n = dict.length; for (const id of a.ids) { const i = dict.findIndex((d) => d.id === id); if (i >= 0) dict.splice(i, 1); } return n - dict.length; },
      dictionary_import: (a: { content: string }) => {
        let added = 0, skipped = 0;
        for (const line of a.content.split("\n").map((l) => l.trim()).filter((l) => l && !l.startsWith("#"))) {
          const [wrong, right] = line.includes("=>") ? line.split("=>") : [null, line];
          const w = clean(right), m = clean(wrong) || null;
          if (!w || !/[\p{L}\p{N}]/u.test(w) || dupWord(w, m)) { skipped++; continue; }
          dict.push({ id: uid(), word: w, misspelling: m, source: "import", created_at: Date.now() }); added++;
        }
        return { added, skipped };
      },
      snippets_list: () => [...snips].sort((a, b) => a.trigger.localeCompare(b.trigger)),
      snippets_add: (a: { trigger: string; expansion: string }) => {
        const t = clean(a.trigger);
        if (!t) throw "Enter what you will say";
        if (!a.expansion.trim()) throw "Enter the text it should become";
        if (snips.some((x) => x.trigger.toLowerCase() === t.toLowerCase())) throw "That snippet already exists";
        const id = uid(); snips.push({ id, trigger: t, expansion: a.expansion, updated_at: Date.now() }); return id;
      },
      snippets_update: (a: { id: string; trigger: string; expansion: string }) => {
        const x = snips.find((y) => y.id === a.id); if (!x) throw "That snippet no longer exists";
        const t = clean(a.trigger);
        if (snips.some((y) => y.id !== a.id && y.trigger.toLowerCase() === t.toLowerCase())) throw "That snippet already exists";
        x.trigger = t; x.expansion = a.expansion; x.updated_at = Date.now(); return null;
      },
      snippets_delete: (a: { ids: string[] }) => { const n = snips.length; for (const id of a.ids) { const i = snips.findIndex((x) => x.id === id); if (i >= 0) snips.splice(i, 1); } return n - snips.length; },
      transcript_delete: (a: { id: string }) => { const i = transcripts.findIndex((t) => t.id === a.id); if (i >= 0) transcripts.splice(i, 1); return i >= 0 ? 1 : 0; },
      settings_get: () => ({ ...settings }),
      settings_update: (a: { patch: Record<string, unknown> }) => {
        for (const [k, v] of Object.entries(a.patch)) {
          if (!(k in settings)) throw `unknown setting ${k}`;
          if (k === "ollama_url" && !String(v).startsWith("http://127.0.0.1") && !String(v).startsWith("http://localhost")) throw "Ollama must run on this Mac (http://localhost or http://127.0.0.1)";
          if (k === "languages" && Array.isArray(v) && v.length === 0 && !settings.auto_detect_language) throw "Choose at least one language or turn on auto-detect";
        }
        Object.assign(settings, a.patch); return { ...settings };
      },
      mics_list: () => s.mics ?? [{ name: "MacBook Air Microphone", is_default: true }, { name: "USB Headset", is_default: false }],
      llm_key_status: () => ({ saved: groqKey }),
      llm_key_set: (a: { key: string }) => { if (!a.key.trim().startsWith("gsk_") || a.key.trim().length <= 20) throw "That does not look like a Groq API key (it starts with gsk_)"; groqKey = true; return null; },
      llm_key_delete: () => { groqKey = false; return null; },
      llm_test: () => (settings.llm_provider === "groq" && !groqKey ? (() => { throw "no Groq API key saved"; })() : "So I think we should meet at 6 tomorrow."),
      data_delete: (a: { categories: string[]; confirm: string }) => { if (a.confirm !== "DELETE") throw "Type DELETE to confirm"; const out: Record<string, number> = {}; if (a.categories.includes("dictionary") || a.categories.includes("everything")) { out.dictionary = dict.length; dict.length = 0; } return out; },
      transcripts_list: () => {
        if (s.failList) throw "database is locked";
        return [...transcripts];
      },
      permissions_status: () => permissions,
      styles_get: () => { if (s.failStyles) throw "database is locked"; return styles.map((x) => ({ ...x })); },
      style_set: (a: { category: string; tone: string }) => {
        // same rules as the schema: very casual is Personal only, excited is not offered for Personal
        if ((a.tone === "very_casual" && a.category !== "personal") || (a.tone === "excited" && a.category === "personal")) throw "That tone for this kind of app is not allowed";
        const row = styles.find((x) => x.category === a.category);
        if (!row) throw "Unknown app category";
        row.tone = a.tone;
        return null;
      },
      transcript_copy: () => null,
      hotkey_start: () => null,
      hotkey_pause: () => null,
      mic_test_start: () => null,
      mic_test_stop: () => null,
      dictation_cancel: () => null,
      dictation_toggle: () => null,
      dictation_undo: () => null,
      "plugin:window|set_ignore_cursor_events": () => null,
      request_input_monitoring: () => true,
      "plugin:event|listen": (a: { event: string; handler: number }) => {
        listeners.set(a.event, [...(listeners.get(a.event) ?? []), a.handler]);
        return a.handler;
      },
      "plugin:event|unlisten": () => null,
      "plugin:opener|open_url": () => null,
    };
    (window as any).__TAURI_INTERNALS__ = {
      invoke: async (cmd: string, args: unknown) => {
        calls.push({ cmd, args });
        const h = handlers[cmd];
        if (!h) throw `fake backend: no handler for ${cmd}`;
        return h(args);
      },
      transformCallback: (cb: (p: unknown) => void) => {
        const id = nextId++;
        callbacks.set(id, cb);
        return id;
      },
      unregisterCallback: (id: number) => callbacks.delete(id),
      convertFileSrc: (p: string) => p,
      metadata: { currentWindow: { label: "main" }, currentWebview: { windowLabel: "main", label: "main" } },
    };
    (window as any).__TAURI_EVENT_PLUGIN_INTERNALS__ = {
      unregisterListener: (event: string, id: number) => listeners.set(event, (listeners.get(event) ?? []).filter((x) => x !== id)),
    };
    // Test helpers: fire a backend event, read the commands the UI sent.
    (window as any).__fake = {
      emit: (event: string, payload: unknown) =>
        (listeners.get(event) ?? []).forEach((id) => callbacks.get(id)?.({ event, id, payload })),
      calls,
    };
  }, state);
}

export const sampleTranscript = (over: Record<string, unknown> = {}) => ({
  id: crypto.randomUUID(),
  created_at: Date.now() - 60_000,
  raw_text: "um so the meeting is at 6",
  final_text: "So the meeting is at 6 p.m.",
  app_bundle_id: "com.apple.mail",
  app_name: "Mail",
  language: "en",
  duration_ms: 3200,
  word_count: 7,
  cleanup_level: "light",
  status: "pasted",
  error: null,
  ...over,
});
