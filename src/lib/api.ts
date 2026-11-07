// Typed wrappers over the Rust commands and events. Screens import from here only, so the data layer can change
// without touching screen code.
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type Transcript = {
  id: string;
  created_at: number;
  raw_text: string;
  final_text: string;
  app_bundle_id: string | null;
  app_name: string | null;
  language: string | null;
  duration_ms: number;
  word_count: number;
  cleanup_level: "none" | "light" | "medium";
  status: "pasted" | "paste_failed" | "cancelled" | "error";
  error: string | null;
};

export type DictationState =
  | "idle" | "recording" | "hands_free" | "processing" | "success" | "error" | "warning" | "cancelled";

export type Settings = {
  shortcuts: Record<string, string[][]>;
  mic_id: string | null;
  languages: string[];
  auto_detect_language: boolean;
  theme: "light" | "dark" | "system";
  launch_at_login: boolean;
  show_bar: boolean;
  storage_mode: "store" | "auto_delete_24h" | "never";
  cleanup_level: "none" | "light" | "medium";
  llm_provider: "none" | "groq" | "ollama";
  groq_model: string;
  ollama_model: string;
  ollama_url: string;
  sounds: boolean;
  onboarding_done: boolean;
  [key: string]: unknown;
};
export type Mic = { name: string; is_default: boolean };
export type DictEntry = { id: string; word: string; misspelling: string | null; source: "manual" | "auto" | "import"; created_at: number };
export type Snippet = { id: string; trigger: string; expansion: string; updated_at: number };
export type StyleCategory = "personal" | "work" | "email" | "other";
export type Tone = "formal" | "casual" | "very_casual" | "excited";
export type StyleRow = { category: StyleCategory; tone: Tone };
export type DeleteCategory = "history" | "dictionary" | "snippets" | "personalization" | "usage" | "everything";

export type Permissions = {
  accessibility: boolean; input_monitoring: boolean; hotkey_running: boolean; fn_key_conflict: boolean;
  microphone: "granted" | "denied" | "undetermined" | "unknown";
  /** Other running dictation apps that may react to the same keys. */
  other_dictation_apps: string[];
};

/** The keys of the hold-to-dictate shortcut, e.g. ["Fn"] or ["RightCtrl", "RightOpt"]. */
export const pushToTalk = (s: Settings | null): string[] => s?.shortcuts?.push_to_talk?.[0] ?? ["Fn"];
const KEY_WORD: Record<string, string> = {
  Fn: "fn", LeftCtrl: "left ⌃", RightCtrl: "right ⌃", LeftOpt: "left ⌥", RightOpt: "right ⌥",
  LeftCmd: "left ⌘", RightCmd: "right ⌘", LeftShift: "left ⇧", RightShift: "right ⇧",
};
/** "fn" or "right ⌃ + right ⌥", for sentences like "Hold fn to talk". */
export const shortcutText = (keys: string[]) => keys.map((k) => KEY_WORD[k] ?? k).join(" + ");

export const api = {
  transcriptsList: (before?: number, limit = 50) => invoke<Transcript[]>("transcripts_list", { before, limit }),
  /** `raw`: copy what Whisper heard, before cleanup. */
  transcriptCopy: (id: string, raw = false) => invoke<void>("transcript_copy", { id, raw }),
  copyLast: () => invoke<boolean>("copy_last"),
  dictationCancel: () => invoke<void>("dictation_cancel"),
  dictationToggle: () => invoke<void>("dictation_toggle"),
  dictationUndo: () => invoke<void>("dictation_undo"),
  hotkeyPause: (on: boolean) => invoke<void>("hotkey_pause", { on }),
  micTestStart: () => invoke<void>("mic_test_start"),
  micTestStop: () => invoke<void>("mic_test_stop"),
  permissions: () => invoke<Permissions>("permissions_status"),
  requestInputMonitoring: () => invoke<boolean>("request_input_monitoring"),
  hotkeyStart: () => invoke<void>("hotkey_start"),
  transcriptDelete: (id: string) => invoke<number>("transcript_delete", { id }),

  settings: () => invoke<Settings>("settings_get"),
  updateSettings: (patch: Partial<Settings>) => invoke<Settings>("settings_update", { patch }),
  mics: () => invoke<Mic[]>("mics_list"),
  dataDelete: (categories: DeleteCategory[], confirm: string) => invoke<Record<string, number>>("data_delete", { categories, confirm }),
  groqKeyStatus: () => invoke<{ saved: boolean }>("llm_key_status"),
  groqKeySet: (key: string) => invoke<void>("llm_key_set", { key }),
  groqKeyDelete: () => invoke<void>("llm_key_delete"),
  cleanupTest: () => invoke<string>("llm_test"),

  models: () => invoke<WhisperModel[]>("models_list"),
  modelSelect: (id: string) => invoke<void>("model_select", { id }),
  dictionary: () => invoke<DictEntry[]>("dictionary_list"),
  dictionaryAdd: (word: string, misspelling: string | null) => invoke<string>("dictionary_add", { word, misspelling }),
  dictionaryUpdate: (id: string, word: string, misspelling: string | null) => invoke<void>("dictionary_update", { id, word, misspelling }),
  dictionaryDelete: (ids: string[]) => invoke<number>("dictionary_delete", { ids }),
  dictionaryImport: (content: string) => invoke<{ added: number; skipped: number }>("dictionary_import", { content }),

  styles: () => invoke<StyleRow[]>("styles_get"),
  styleSet: (category: StyleCategory, tone: Tone) => invoke<void>("style_set", { category, tone }),

  snippets: () => invoke<Snippet[]>("snippets_list"),
  snippetAdd: (trigger: string, expansion: string) => invoke<string>("snippets_add", { trigger, expansion }),
  snippetUpdate: (id: string, trigger: string, expansion: string) => invoke<void>("snippets_update", { id, trigger, expansion }),
  snippetDelete: (ids: string[]) => invoke<number>("snippets_delete", { ids }),
};

/** Puts the saved theme on <html> so the CSS tokens follow it. "system" follows macOS. */
export function applyTheme(theme: Settings["theme"]) {
  if (theme === "system") delete document.documentElement.dataset.theme;
  else document.documentElement.dataset.theme = theme;
}

export type WhisperModel = {
  model: { id: string; label: string; size_mb: number; ram_mb: number; note: string };
  downloaded: boolean;
  active: boolean;
};
export type ModelEvent =
  | { kind: "progress"; id: string; done: number; total: number }
  | { kind: "ready"; id: string }
  | { kind: "error"; id: string; message: string };

export const events = {
  onModel: async (cb: (e: ModelEvent) => void): Promise<UnlistenFn> => {
    const un = await Promise.all((["progress", "ready", "error"] as const).map((kind) =>
      listen<any>(`model://${kind}`, (e) => cb({ kind, ...e.payload }))));
    return () => un.forEach((f) => f());
  },
  onState: (cb: (s: { state: DictationState; message: string | null }) => void): Promise<UnlistenFn> =>
    listen<{ state: DictationState; message: string | null }>("dictation://state", (e) => cb(e.payload)),
  onLevel: (cb: (rms: number) => void): Promise<UnlistenFn> => listen<number>("dictation://level", (e) => cb(e.payload)),
  onMicTestLevel: (cb: (rms: number) => void): Promise<UnlistenFn> => listen<number>("mic_test://level", (e) => cb(e.payload)),
  onSettings: (cb: (s: Settings) => void): Promise<UnlistenFn> => listen<Settings>("settings://changed", (e) => cb(e.payload)),
  onTranscript: (cb: (t: Transcript) => void): Promise<UnlistenFn> =>
    listen<Transcript>("transcript://created", (e) => cb(e.payload)),
};

export const settingsUrl = {
  accessibility: "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility",
  inputMonitoring: "x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent",
  microphone: "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone",
  keyboard: "x-apple.systempreferences:com.apple.Keyboard-Settings.extension",
};
