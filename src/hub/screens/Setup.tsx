// First-run setup: permissions, microphone test, shortcut, a place to try it, then done.
// Polls permissions while open, since macOS gives no callback when access changes.
import { useEffect, useState, type ReactNode } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { api, pushToTalk, settingsUrl, shortcutText, type Permissions, type Settings } from "../../lib/api";
import { Button, Notice, PageHeader, PermissionRow, TextArea } from "../../ui";
import { MicTest } from "../parts/MicTest";
import { ShortcutEditor } from "../parts/ShortcutEditor";
import { SpeechEngine } from "../parts/SpeechEngine";
import type { Nav } from "../Hub";

function Step({ n, title, children }: { n: number; title: string; children: ReactNode }) {
  return (
    <section aria-labelledby={`step-${n}`} className="mb-8">
      <h2 id={`step-${n}`} className="mb-3 flex items-center gap-2 text-lg font-semibold">
        <span className="grid size-6 place-items-center rounded-pill bg-accent-soft text-sm text-accent" aria-hidden>{n}</span>
        {title}
      </h2>
      {children}
    </section>
  );
}

export function Setup({ nav }: { nav: Nav }) {
  const [p, setP] = useState<Permissions | null>(null);
  const [s, setS] = useState<Settings | null>(null);
  const [hotkeyError, setHotkeyError] = useState<string | null>(null);
  const [finishError, setFinishError] = useState<string | null>(null);

  useEffect(() => {
    const tick = () => api.permissions().then(setP).catch(() => {});
    tick();
    api.settings().then(setS).catch(() => {});
    const id = window.setInterval(tick, 1500);
    return () => window.clearInterval(id);
  }, []);

  const startHotkey = async () => {
    try { await api.hotkeyStart(); setHotkeyError(null); setP(await api.permissions()); }
    catch (e) { setHotkeyError(String(e)); }
  };

  const finish = async () => {
    try { setS(await api.updateSettings({ onboarding_done: true })); nav("home"); }
    catch (e) { setFinishError(String(e)); }
  };

  const keys = shortcutText(pushToTalk(s));
  const mic = p?.microphone;

  return (
    <>
      <PageHeader title="Set up dictation" description="Five short steps, about two minutes." />

      <Step n={1} title="Choose how speech becomes text">
        {s && <SpeechEngine settings={s} onSaved={setS} />}
      </Step>

      <Step n={2} title="Allow access">
        <div className="flex flex-col gap-3">
          <PermissionRow
            title="Listen for your shortcut"
            body="Input Monitoring lets Quietkey notice when you hold the dictation keys, in any app."
            granted={!!p?.input_monitoring}
            onAllow={async () => { if (!(await api.requestInputMonitoring())) await openUrl(settingsUrl.inputMonitoring); }}
          />
          <PermissionRow
            title="Type into other apps"
            body="Accessibility lets Quietkey paste your words where your cursor is."
            granted={!!p?.accessibility}
            onAllow={() => openUrl(settingsUrl.accessibility)}
          />
          <PermissionRow
            title="Use the microphone"
            body={mic === "undetermined" ? "macOS asks when you test the microphone below." : "Quietkey only records while you hold the shortcut."}
            granted={mic === "granted"}
            onAllow={() => openUrl(settingsUrl.microphone)}
          />
        </div>
        <div className="mt-3 flex flex-col gap-3">
          {p && !p.hotkey_running && (
            <Notice tone="warning" action={<Button size="sm" variant="primary" onClick={startHotkey}>Start listening</Button>}>
              The shortcut listener is off. After allowing the first two, start it here. Some Macs need the app restarted.
            </Notice>
          )}
          {p?.hotkey_running && <Notice tone="success">Ready. Hold {keys} in any text field and talk.</Notice>}
          {hotkeyError && <Notice tone="error">Could not start: {hotkeyError}</Notice>}
          {p?.fn_key_conflict && pushToTalk(s).includes("Fn") && (
            <Notice tone="warning" action={<Button size="sm" onClick={() => openUrl(settingsUrl.keyboard)}>Open Keyboard settings</Button>}>
              macOS also reacts to fn (dictation, emoji or input switching) and will type over your text. Set "Press fn key to" to "Do Nothing", or pick other keys in step 4.
            </Notice>
          )}
        </div>
      </Step>

      <Step n={3} title="Check your microphone">
        <MicTest />
      </Step>

      <Step n={4} title="Choose your shortcut">
        <div className="flex items-start justify-between gap-6 rounded-lg bg-surface p-4">
          <p className="text-sm text-muted">Hold it to talk, let go to insert. Press it twice quickly to keep talking without holding.</p>
          {s && <ShortcutEditor settings={s} onSaved={setS} />}
        </div>
      </Step>

      <Step n={5} title="Try it">
        <TextArea label={`Click here, hold ${keys} and say a sentence`} rows={3} placeholder="Your words appear here" />
      </Step>

      <div className="flex items-center gap-3">
        <Button variant="primary" onClick={finish}>{s?.onboarding_done ? "Back to Home" : "Finish setup"}</Button>
        {finishError && <span className="text-sm text-danger" role="alert">{finishError}</span>}
      </div>
    </>
  );
}
