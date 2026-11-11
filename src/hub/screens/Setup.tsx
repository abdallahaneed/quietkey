// First-run onboarding, one step per screen: welcome, speech engine, permissions, microphone, shortcut, try it, done.
// Opens by itself until it is finished (Hub.tsx) and again from Setup in the sidebar. The model download started in
// the speech step keeps running while the user goes on; its progress shows in the bottom bar.
// Polls permissions while open, since macOS gives no callback when access changes.
import { useEffect, useRef, useState, type ReactNode } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { CheckCircle2, Cpu, Gift, Lock, MousePointerClick, X } from "lucide-react";
import { api, events, pushToTalk, settingsUrl, shortcutText, type Permissions, type Settings, type WhisperModel } from "../../lib/api";
import { Button, IconButton, KeyCaps, Notice, PermissionRow, TextArea, cx } from "../../ui";
import { MicTest } from "../parts/MicTest";
import { ShortcutEditor } from "../parts/ShortcutEditor";
import { SpeechEngine } from "../parts/SpeechEngine";
import logo from "../../../assets/icon.svg";
import type { Nav } from "../Hub";

const STEPS = ["welcome", "speech", "access", "mic", "shortcut", "try", "done"] as const;
type Step = (typeof STEPS)[number];

/** Model download progress for the bottom bar: the name and percent, or null when nothing downloads. */
function useDownload() {
  const [models, setModels] = useState<WhisperModel[]>([]);
  const [busy, setBusy] = useState<{ id: string; pct: number | null } | null>(null);
  const [failed, setFailed] = useState(false);
  useEffect(() => {
    const load = () => api.models().then(setModels).catch(() => {});
    load();
    const un = events.onModel((e) => {
      if (e.kind === "progress") { setFailed(false); setBusy({ id: e.id, pct: e.total ? Math.floor((e.done / e.total) * 100) : null }); }
      else { setBusy(null); setFailed(e.kind === "error"); load(); }
    });
    return () => { un.then((f) => f()); };
  }, []);
  const name = (id: string) => models.find((m) => m.model.id === id)?.model.label ?? id;
  const ready = models.find((m) => m.active && m.downloaded)?.model.label ?? null;
  return { busy: busy && { name: name(busy.id), pct: busy.pct }, failed, ready, models };
}

function Title({ title, sub }: { title: string; sub?: ReactNode }) {
  return (
    <div className="mb-6 text-center">
      <h1 className="text-2xl font-semibold">{title}</h1>
      {sub && <p className="mt-1.5 text-base text-muted">{sub}</p>}
    </div>
  );
}

/** A small app window where the words of a sentence appear one by one while the shortcut is "held". */
function HowItWorks({ keys }: { keys: string[] }) {
  const words = "Running ten minutes late, start without me.".split(" ");
  const still = typeof window !== "undefined" && window.matchMedia?.("(prefers-reduced-motion: reduce)").matches;
  const [n, setN] = useState(still ? words.length : 0);
  useEffect(() => {
    if (still) return;
    // 0..words: speaking, then a pause with the full text, then again
    const id = window.setInterval(() => setN((x) => (x >= words.length + 4 ? 0 : x + 1)), 380);
    return () => window.clearInterval(id);
  }, [still, words.length]);
  const holding = n > 0 && n <= words.length;
  return (
    <figure aria-label={`Illustration: hold ${keys.join(" ")} in a text field, talk, and the words are typed`} className="mx-auto w-full max-w-[460px]">
      <div className="rounded-lg border border-border bg-surface shadow-card">
        <div className="flex items-center gap-1.5 border-b border-border px-3 py-2" aria-hidden>
          <span className="size-2.5 rounded-pill bg-border" /><span className="size-2.5 rounded-pill bg-border" /><span className="size-2.5 rounded-pill bg-border" />
          <span className="ml-2 text-xs text-muted">Message to Sam</span>
        </div>
        <div className="p-4" aria-hidden>
          <div className="flex min-h-[44px] items-center rounded-md border border-accent bg-bg px-3 text-base">
            <span>{words.slice(0, Math.min(n, words.length)).join(" ")}</span>
            <span className="ml-0.5 inline-block h-5 w-px animate-pulse bg-accent" />
          </div>
        </div>
      </div>
      <div className="mt-4 flex items-center justify-center gap-3 text-sm text-muted" aria-hidden>
        <span className={cx("rounded-md p-1 transition-colors", holding && "bg-accent-soft")}><KeyCaps keys={keys} /></span>
        <span>{holding ? "held while you talk" : n > words.length ? "let go: done" : "click a text field"}</span>
      </div>
    </figure>
  );
}

export function Setup({ nav }: { nav: Nav }) {
  const [step, setStep] = useState<Step>("welcome");
  const [p, setP] = useState<Permissions | null>(null);
  const [s, setS] = useState<Settings | null>(null);
  const [hotkeyError, setHotkeyError] = useState<string | null>(null);
  const [finishError, setFinishError] = useState<string | null>(null);
  const [tried, setTried] = useState(false);
  const dl = useDownload();

  useEffect(() => {
    const tick = () => api.permissions().then(setP).catch(() => {});
    tick();
    api.settings().then(setS).catch(() => {});
    const id = window.setInterval(tick, 1500);
    return () => window.clearInterval(id);
  }, []);

  // A fresh install has no model: start the default download as soon as the user reaches the speech step, so it
  // runs while the other steps are done.
  const started = useRef(false);
  useEffect(() => {
    if (step === "welcome" || s?.stt_engine !== "local" || started.current) return;
    started.current = true;
    api.models().then((ms) => { if (!ms.some((m) => m.downloaded)) return api.modelSelect("parakeet-v3"); }).catch(() => {});
  }, [step, s?.stt_engine]);

  // The try step: a dictation saved while it is open means it worked.
  useEffect(() => {
    if (step !== "try") return;
    const un = events.onTranscript(() => setTried(true));
    return () => { un.then((f) => f()); };
  }, [step]);

  const startHotkey = async () => {
    try { await api.hotkeyStart(); setHotkeyError(null); setP(await api.permissions()); }
    catch (e) { setHotkeyError(String(e)); }
  };

  const finish = async () => {
    try { setS(await api.updateSettings({ onboarding_done: true })); nav("home"); }
    catch (e) { setFinishError(String(e)); }
  };

  const keyList = pushToTalk(s);
  const keys = shortcutText(keyList);
  const mic = p?.microphone;
  const allowed = !!p?.input_monitoring && !!p?.accessibility && mic === "granted";
  const i = STEPS.indexOf(step);
  const go = (d: 1 | -1) => setStep(STEPS[Math.min(STEPS.length - 1, Math.max(0, i + d))]);

  const next: { label: string; onClick: () => void } =
    step === "welcome" ? { label: "Get started", onClick: () => go(1) }
    : step === "access" && !allowed ? { label: "Skip for now", onClick: () => go(1) }
    : step === "try" && !tried ? { label: "Skip", onClick: () => go(1) }
    : step === "done" ? { label: s?.onboarding_done ? "Back to Home" : "Start dictating", onClick: finish }
    : { label: "Continue", onClick: () => go(1) };
  const primary = !(step === "access" && !allowed) && !(step === "try" && !tried);

  return (
    <div className="flex h-full flex-col">
      {/* title bar space for the traffic lights, draggable */}
      <div data-tauri-drag-region className="flex h-[44px] shrink-0 items-center justify-end px-3">
        {s?.onboarding_done && <IconButton label="Close setup" onClick={() => nav("home")}><X className="size-4" /></IconButton>}
      </div>
      <ol aria-label={`Step ${i + 1} of ${STEPS.length}`} className="mx-auto flex w-full max-w-[600px] gap-1.5 px-6">
        {STEPS.map((x, k) => (
          <li key={x} aria-current={k === i ? "step" : undefined} className={cx("h-1 flex-1 rounded-pill transition-colors", k < i ? "bg-accent/50" : k === i ? "bg-accent" : "bg-border")} />
        ))}
      </ol>

      <main className="min-h-0 flex-1 overflow-y-auto">
        <div className="mx-auto w-full max-w-[600px] px-6 py-8">
          {step === "welcome" && (
            <div className="flex flex-col items-center text-center">
              <img src={logo} alt="" className="mb-5 size-20" />
              <Title title="Welcome to Quietkey" sub="Hold a key, talk, and your words are typed into any app. Setup takes about two minutes." />
              <ul className="flex w-full flex-col gap-3 text-left">
                {[
                  [MousePointerClick, "Works where you type", "Mail, Slack, Notes, your browser: wherever the cursor is."],
                  [Lock, "Private", "With a model on this Mac, your voice never leaves it."],
                  [Gift, "Free", "No account, no subscription, no word limit."],
                ].map(([Icon, t, b]) => {
                  const I = Icon as typeof Lock;
                  return (
                    <li key={t as string} className="flex items-start gap-3 rounded-lg border border-border bg-surface p-4">
                      <I className="mt-0.5 size-5 shrink-0 text-accent" aria-hidden />
                      <div><p className="font-medium">{t as string}</p><p className="text-sm text-muted">{b as string}</p></div>
                    </li>
                  );
                })}
              </ul>
            </div>
          )}

          {step === "speech" && (
            <>
              <Title title="How should speech become text?" sub="Pick where your voice is turned into text. You can change this later." />
              {s && <SpeechEngine settings={s} onSaved={setS} />}
              {s?.stt_engine === "local" && <p className="mt-3 text-sm text-muted">The model downloads in the background. Carry on meanwhile.</p>}
            </>
          )}

          {step === "access" && (
            <>
              <Title title="Allow three things" sub="macOS asks for each one. Quietkey only listens while you hold your shortcut." />
              <div className="flex flex-col gap-3">
                <PermissionRow
                  title="Listen for your shortcut"
                  body="Input Monitoring lets Quietkey notice when you hold the dictation keys, in any app."
                  granted={!!p?.input_monitoring}
                  onAllow={async () => { if (!(await api.requestInputMonitoring())) await openUrl(settingsUrl.inputMonitoring); }}
                />
                <PermissionRow
                  title="Type into other apps"
                  body="Accessibility lets Quietkey type your words where your cursor is."
                  granted={!!p?.accessibility}
                  onAllow={async () => { if (!(await api.requestAccessibility())) await openUrl(settingsUrl.accessibility); }}
                />
                <PermissionRow
                  title="Use the microphone"
                  body={mic === "undetermined" ? "macOS asks you once. Quietkey only records while you hold the shortcut." : "Quietkey only records while you hold the shortcut."}
                  granted={mic === "granted"}
                  onAllow={async () => { if ((await api.requestMicrophone()) !== "granted") await openUrl(settingsUrl.microphone); }}
                />
              </div>
              <div className="mt-3 flex flex-col gap-3">
                {p && !p.hotkey_running && (
                  <Notice tone="warning" action={<Button size="sm" variant="primary" onClick={startHotkey}>Start listening</Button>}>
                    The shortcut listener is off. After allowing the first two, start it here. Some Macs need the app restarted.
                  </Notice>
                )}
                {p?.hotkey_running && allowed && <Notice tone="success">All set. Quietkey can hear your shortcut and type for you.</Notice>}
                {hotkeyError && <Notice tone="error">Could not start: {hotkeyError}</Notice>}
                {p?.fn_key_conflict && keyList.includes("Fn") && (
                  <Notice tone="warning" action={<Button size="sm" onClick={() => openUrl(settingsUrl.keyboard)}>Open Keyboard settings</Button>}>
                    macOS also reacts to fn (dictation, emoji or input switching) and will type over your text. Set "Press fn key to" to "Do Nothing", or pick other keys in the shortcut step.
                  </Notice>
                )}
              </div>
            </>
          )}

          {step === "mic" && (
            <>
              <Title title="Check your microphone" sub="Say a few words. The bar should move while you talk. Nothing is recorded." />
              <MicTest />
            </>
          )}

          {step === "shortcut" && (
            <>
              <Title title="One shortcut, every app" sub={<>Click where you would type, hold <strong className="font-semibold text-fg">{keys}</strong> and talk. Let go, and your words are there.</>} />
              <HowItWorks keys={keyList} />
              <div className="mt-6 flex items-center justify-between gap-6 rounded-lg border border-border bg-surface p-4">
                <p className="text-sm text-muted">Your shortcut. Press it twice quickly to keep talking without holding.</p>
                {s && <ShortcutEditor settings={s} onSaved={setS} />}
              </div>
            </>
          )}

          {step === "try" && (
            <>
              <Title title="Give it a try" sub={<>Click the field, hold <strong className="font-semibold text-fg">{keys}</strong> and say a sentence.</>} />
              {!dl.ready && s?.stt_engine === "local" && (
                <div className="mb-3"><Notice tone="info">{dl.busy ? `The speech model is still downloading (${dl.busy.pct ?? 0}%). You can try once it is done, or skip and try later.` : "No speech model yet. Go back to pick one, or skip and set it up later."}</Notice></div>
              )}
              <TextArea label="Practice field" rows={5} placeholder="Your words appear here" autoFocus />
              {tried && <div className="mt-3"><Notice tone="success">That worked. Quietkey typed what you said.</Notice></div>}
            </>
          )}

          {step === "done" && (
            <div className="flex flex-col items-center text-center">
              <CheckCircle2 className="mb-4 size-14 text-accent" aria-hidden />
              <Title title="You're ready" sub="Quietkey runs in the menu bar. Close this window any time; dictation keeps working." />
              <ul className="flex w-full flex-col divide-y divide-border rounded-lg border border-border bg-surface text-left text-sm">
                <li className="flex items-center justify-between gap-4 p-3"><span>Dictate</span><span className="flex items-center gap-2 text-muted">hold <KeyCaps keys={keyList} /></span></li>
                <li className="flex items-center justify-between gap-4 p-3"><span>Cancel while talking</span><KeyCaps keys={["Esc"]} /></li>
                <li className="flex items-center justify-between gap-4 p-3"><span>Paste the last dictation again</span><KeyCaps keys={["⌃", "⌘", "V"]} /></li>
                <li className="flex items-center justify-between gap-4 p-3"><span>Live text, cleanup, languages</span><span className="text-muted">Settings</span></li>
              </ul>
              {finishError && <p className="mt-3 text-sm text-danger" role="alert">{finishError}</p>}
            </div>
          )}
        </div>
      </main>

      <footer className="flex h-16 shrink-0 items-center justify-between gap-4 border-t border-border px-6">
        <div className="w-24">{i > 0 && <Button variant="ghost" onClick={() => go(-1)}>Back</Button>}</div>
        <div className="flex min-w-0 flex-1 items-center justify-end gap-4">
          {s?.stt_engine === "local" && (dl.busy || dl.failed) && (
            <span className="flex min-w-0 items-center gap-2 truncate text-sm text-muted" role="status">
              <Cpu className="size-4 shrink-0" aria-hidden />
              {dl.failed ? "Download failed. Pick the model again on the Models page." : `${dl.busy!.name}: ${dl.busy!.pct === null || dl.busy!.pct >= 100 ? "getting ready…" : `downloading ${dl.busy!.pct}%`}`}
            </span>
          )}
          {/* key: a new button per step, so the colour never animates between primary and secondary */}
          <Button key={step} variant={primary ? "primary" : "secondary"} onClick={next.onClick}>{next.label}</Button>
        </div>
      </footer>
    </div>
  );
}
