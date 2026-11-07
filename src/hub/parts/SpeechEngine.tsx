// Where speech becomes text: a Whisper model on this Mac (private, offline) or Groq's Whisper with the user's free key
// (nothing to download, works on any Mac). Used in Setup and Settings.
import { useEffect, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { api, type Settings } from "../../lib/api";
import { Button, Notice, RadioCards, TextInput } from "../../ui";
import { ModelPicker } from "./ModelPicker";

export function SpeechEngine({ settings, onSaved }: { settings: Settings; onSaved: (s: Settings) => void }) {
  const [keySaved, setKeySaved] = useState<boolean | null>(null);
  const [key, setKey] = useState("");
  const [error, setError] = useState<string | null>(null);
  useEffect(() => { api.groqKeyStatus().then((k) => setKeySaved(k.saved)).catch(() => setKeySaved(false)); }, []);

  const choose = async (engine: Settings["stt_engine"]) => {
    setError(null);
    try { onSaved(await api.updateSettings({ stt_engine: engine })); }
    catch (e) { setError(String(e)); }
  };
  const saveKey = async () => {
    setError(null);
    try { await api.groqKeySet(key); setKey(""); setKeySaved(true); }
    catch (e) { setError(String(e)); }
  };

  return (
    <div className="flex flex-col gap-3">
      <RadioCards label="Speech engine" value={settings.stt_engine} onChange={choose} options={[
        { value: "local", title: "On this Mac", body: "Download a speech model once. Works offline, and your voice never leaves the Mac." },
        { value: "groq", title: "Groq, with your free key", body: "Nothing to download, works on any Mac. Your recordings are sent to Groq to be turned into text." },
      ]} />
      {error && <Notice tone="error">{error}</Notice>}
      {settings.stt_engine === "local" && (
        <div className="ml-7 flex flex-col gap-2 border-l-2 border-border pl-4">
          <p className="text-sm font-semibold">Pick a model</p>
          <ModelPicker />
        </div>
      )}
      {settings.stt_engine === "groq" && (keySaved
        ? <Notice tone="success">Groq key saved in your Keychain. You are ready to dictate.</Notice>
        : (
          <form className="flex flex-col gap-2" onSubmit={(e) => { e.preventDefault(); saveKey(); }}>
            <div className="flex items-end gap-2">
              <TextInput label="Groq API key" type="password" autoComplete="off" value={key} onChange={(e) => setKey(e.target.value)} placeholder="gsk_…" className="flex-1" />
              <Button type="submit" variant="primary" disabled={!key.trim()}>Save key</Button>
            </div>
            <p className="text-sm text-muted">
              Free, no card:{" "}
              <button type="button" className="underline hover:text-fg" onClick={() => openUrl("https://console.groq.com/keys")}>get a key at console.groq.com</button>, then paste it here.
            </p>
          </form>
        ))}
    </div>
  );
}
