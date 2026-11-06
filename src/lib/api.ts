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
};

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
