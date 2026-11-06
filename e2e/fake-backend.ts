// Stands in for the Rust side so the screens can be tested in a plain browser.
// Mirrors window.__TAURI_INTERNALS__ as used by @tauri-apps/api 2.x (invoke, transformCallback, events).
import type { Page } from "@playwright/test";

export type FakeState = {
  transcripts?: unknown[];
  permissions?: { accessibility: boolean; input_monitoring: boolean; hotkey_running: boolean; fn_key_conflict: boolean };
  failList?: boolean;
};

export async function installFakeBackend(page: Page, state: FakeState = {}) {
  await page.addInitScript((s: FakeState) => {
    const callbacks = new Map<number, (payload: unknown) => void>();
    const listeners = new Map<string, number[]>();
    let nextId = 1;
    const calls: { cmd: string; args: unknown }[] = [];
    const permissions = s.permissions ?? { accessibility: true, input_monitoring: true, hotkey_running: true, fn_key_conflict: false };
    const handlers: Record<string, (args: any) => unknown> = {
      transcripts_list: () => {
        if (s.failList) throw "database is locked";
        return s.transcripts ?? [];
      },
      permissions_status: () => permissions,
      transcript_copy: () => null,
      hotkey_start: () => null,
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
