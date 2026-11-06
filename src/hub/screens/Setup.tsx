// Permission setup. Polls while the window is open, since macOS gives no callback when access changes.
import { useEffect, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { api, settingsUrl, type Permissions } from "../../lib/api";
import { Button, Notice, PageHeader, PermissionRow } from "../../ui";

export function Setup() {
  const [p, setP] = useState<Permissions | null>(null);
  const [hotkeyError, setHotkeyError] = useState<string | null>(null);

  useEffect(() => {
    const tick = () => api.permissions().then(setP).catch(() => {});
    tick();
    const id = window.setInterval(tick, 1500);
    return () => window.clearInterval(id);
  }, []);

  const startHotkey = async () => {
    try { await api.hotkeyStart(); setHotkeyError(null); setP(await api.permissions()); }
    catch (e) { setHotkeyError(String(e)); }
  };

  return (
    <>
      <PageHeader title="Set up dictation" description="macOS asks for these once. Nothing you say leaves this Mac for transcription." />
      <div className="flex flex-col gap-3">
        <PermissionRow
          title="Listen for the fn key"
          body="Input Monitoring lets the app notice when you hold fn, in any app."
          granted={!!p?.input_monitoring}
          onAllow={async () => { if (!(await api.requestInputMonitoring())) await openUrl(settingsUrl.inputMonitoring); }}
        />
        <PermissionRow
          title="Type into other apps"
          body="Accessibility lets the app paste your words where your cursor is."
          granted={!!p?.accessibility}
          onAllow={() => openUrl(settingsUrl.accessibility)}
        />
        {/* TODO: show real mic status via AVCaptureDevice.authorizationStatus */}
        <div className="flex items-center justify-between gap-4 rounded-lg bg-surface p-4">
          <div>
            <p className="text-base font-semibold">Use the microphone</p>
            <p className="text-sm text-muted">macOS asks the first time you hold fn and speak. If you said no, turn it on here.</p>
          </div>
          <Button variant="ghost" onClick={() => openUrl(settingsUrl.microphone)}>Open settings</Button>
        </div>
      </div>
      <div className="mt-6 flex flex-col gap-3">
        {p && !p.hotkey_running && (
          <Notice tone="warning" action={<Button size="sm" variant="primary" onClick={startHotkey}>Start listening</Button>}>
            The fn listener is off. After allowing both permissions, start it here. Some Macs need the app restarted.
          </Notice>
        )}
        {p?.hotkey_running && <Notice tone="success">Ready. Hold fn in any text field and talk.</Notice>}
        {hotkeyError && <Notice tone="error">Could not start: {hotkeyError}</Notice>}
        {p?.fn_key_conflict && (
          <Notice tone="warning" action={<Button size="sm" onClick={() => openUrl(settingsUrl.keyboard)}>Open Keyboard settings</Button>}>
            macOS also reacts to fn (dictation, emoji or input switching) and will type over your text. Set "Press fn key to" to "Do Nothing".
          </Notice>
        )}
      </div>
    </>
  );
}
