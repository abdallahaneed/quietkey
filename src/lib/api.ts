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
export type DeleteCategory = "history" | "dictionary" | "snippets" | "personalization" | "usage" | "everything";

export type Permissions = { accessibility: boolean; input_monitoring: boolean; hotkey_running: boolean; fn_key_conflict: boolean };

export const api = {
  transcriptsList: (before?: number, limit = 50) => invoke<Transcript[]>("transcripts_list", { before, limit }),
  transcriptCopy: (id: string) => invoke<void>("transcript_copy", { id }),
  copyLast: () => invoke<boolean>("copy_last"),
  dictationCancel: () => invoke<void>("dictation_cancel"),
  dictationToggle: () => invoke<void>("dictation_toggle"),
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

  dictionary: () => invoke<DictEntry[]>("dictionary_list"),
  dictionaryAdd: (word: string, misspelling: string | null) => invoke<string>("dictionary_add", { word, misspelling }),
  dictionaryUpdate: (id: string, word: string, misspelling: string | null) => invoke<void>("dictionary_update", { id, word, misspelling }),
  dictionaryDelete: (ids: string[]) => invoke<number>("dictionary_delete", { ids }),
  dictionaryImport: (content: string) => invoke<{ added: number; skipped: number }>("dictionary_import", { content }),

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

export const events = {
  onState: (cb: (s: { state: DictationState; message: string | null }) => void): Promise<UnlistenFn> =>
    listen<{ state: DictationState; message: string | null }>("dictation://state", (e) => cb(e.payload)),
  onLevel: (cb: (rms: number) => void): Promise<UnlistenFn> => listen<number>("dictation://level", (e) => cb(e.payload)),
  onTranscript: (cb: (t: Transcript) => void): Promise<UnlistenFn> =>
    listen<Transcript>("transcript://created", (e) => cb(e.payload)),
};

export const settingsUrl = {
  accessibility: "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility",
  inputMonitoring: "x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent",
  microphone: "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone",
  keyboard: "x-apple.systempreferences:com.apple.Keyboard-Settings.extension",
};
