// Records for a few seconds and shows the level, so the user sees the right microphone hears them. Nothing is kept.
import { useEffect, useRef, useState } from "react";
import { api, events } from "../../lib/api";
import { Button, Notice } from "../../ui";

const SECONDS = 6;
const HEARD = 0.02; // RMS above room noise; normal speech is 0.05 and up

export function MicTest() {
  const [running, setRunning] = useState(false);
  const [level, setLevel] = useState(0);
  const [result, setResult] = useState<"heard" | "silent" | null>(null);
  const [error, setError] = useState<string | null>(null);
  const peak = useRef(0);
  const timer = useRef<number | undefined>(undefined);

  const finish = () => {
    window.clearTimeout(timer.current);
    api.micTestStop().catch(() => {});
    setRunning(false);
    setLevel(0);
    setResult(peak.current >= HEARD ? "heard" : "silent");
  };

  const start = async () => {
    setError(null); setResult(null); peak.current = 0;
    try {
      await api.micTestStart();
      setRunning(true);
      timer.current = window.setTimeout(finish, SECONDS * 1000);
    } catch (e) {
      setError(String(e));
    }
  };

  useEffect(() => {
    const un = events.onMicTestLevel((rms) => {
      peak.current = Math.max(peak.current, rms);
      setLevel(Math.min(1, Math.sqrt(rms) * 2.2));
    });
    return () => { un.then((f) => f()); window.clearTimeout(timer.current); api.micTestStop().catch(() => {}); };
  }, []);

  return (
    <div className="flex flex-col gap-3">
      <div className="flex items-center gap-3">
        <Button onClick={running ? finish : start}>{running ? "Stop" : "Test microphone"}</Button>
        <div
          role="meter" aria-label="Microphone level" aria-valuemin={0} aria-valuemax={100} aria-valuenow={Math.round(level * 100)}
          className="h-2 flex-1 overflow-hidden rounded-pill bg-surface-2"
        >
          <div className="h-full rounded-pill bg-accent transition-[width] duration-75" style={{ width: `${level * 100}%` }} />
        </div>
      </div>
      {running && <p className="text-sm text-muted" role="status">Say a sentence…</p>}
      {result === "heard" && <Notice tone="success">The microphone hears you.</Notice>}
      {result === "silent" && <Notice tone="warning">Nothing came through. Pick another microphone in Settings, or check that Quietkey may use it.</Notice>}
      {error && <Notice tone="error">{error}</Notice>}
    </div>
  );
}
