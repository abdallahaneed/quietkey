// Style: how dictated text should sound, per kind of app. Quietkey picks the kind from the app you dictate into.
import { useEffect, useState } from "react";
import { api, type Settings, type StyleCategory, type StyleRow, type Tone } from "../../lib/api";
import { Button, Notice, PageHeader, RadioCards, SkeletonRows } from "../../ui";
import type { Nav } from "../Hub";

type Option = { value: Tone; title: string; body: string };

// Each kind of app: example apps, and the tones it offers with a sample in that tone.
const KINDS: { id: StyleCategory; title: string; apps: string; options: Option[] }[] = [
  {
    id: "personal", title: "Personal messages", apps: "Messages, WhatsApp, Telegram, Signal",
    options: [
      { value: "formal", title: "Tidy", body: "Running late. I'll be there at 7." },
      { value: "casual", title: "Relaxed", body: "Running late, I'll be there at 7" },
      { value: "very_casual", title: "Texting", body: "running late, there at 7" },
    ],
  },
  {
    id: "work", title: "Work chat", apps: "Slack, Teams, Discord, Notion, Linear",
    options: [
      { value: "formal", title: "Tidy", body: "The fix is merged. Could you check it before the release?" },
      { value: "casual", title: "Relaxed", body: "The fix is merged, could you check it before the release" },
      { value: "excited", title: "Upbeat", body: "The fix is merged! Could you check it before the release?" },
    ],
  },
  {
    id: "email", title: "Email", apps: "Mail, Outlook, Spark, Superhuman",
    options: [
      { value: "formal", title: "Tidy", body: "Thanks for the draft. I will send my comments by Friday." },
      { value: "casual", title: "Relaxed", body: "Thanks for the draft, I'll send my comments by Friday" },
      { value: "excited", title: "Upbeat", body: "Thanks for the draft! I'll send my comments by Friday." },
    ],
  },
  {
    id: "other", title: "Everything else", apps: "Any app not listed above",
    options: [
      { value: "formal", title: "Tidy", body: "Buy milk and call the landlord about the heating." },
      { value: "casual", title: "Relaxed", body: "Buy milk and call the landlord about the heating" },
      { value: "excited", title: "Upbeat", body: "Buy milk and call the landlord about the heating!" },
    ],
  },
];

export function Style({ nav }: { nav: Nav }) {
  const [rows, setRows] = useState<StyleRow[] | null>(null);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = () =>
    Promise.all([api.styles(), api.settings()])
      .then(([r, s]) => { setRows(r); setSettings(s); setLoadError(null); })
      .catch((e) => setLoadError(String(e)));
  useEffect(() => { load(); }, []);

  /** Optimistic; the old tone comes back if the backend refuses. */
  const choose = async (category: StyleCategory, tone: Tone) => {
    if (!rows) return;
    const before = rows;
    setRows(rows.map((r) => (r.category === category ? { ...r, tone } : r)));
    setError(null);
    try {
      await api.styleSet(category, tone);
    } catch (e) {
      setRows(before);
      setError(String(e));
    }
  };

  const header = <PageHeader title="Style" description="How your text should sound, depending on the app you dictate into." />;
  if (loadError) return (<>{header}<Notice tone="error" action={<Button size="sm" onClick={load}>Try again</Button>}>Could not load styles: {loadError}</Notice></>);
  if (!rows || !settings) return (<>{header}<SkeletonRows n={4} /></>);

  const aiOn = settings.llm_provider !== "none" && settings.cleanup_level !== "none";

  return (
    <>
      {header}
      {!aiOn && (
        <div className="mb-6">
          <Notice tone="info" action={<Button size="sm" onClick={() => nav("settings")}>Open Settings</Button>}>
            {settings.cleanup_level === "none"
              ? "Cleanup is off, so text is pasted exactly as heard and these choices have no effect."
              : "Without AI cleanup, a tone only changes the final period and, for Texting, the first capital letter. Turn on AI cleanup for the full effect."}
          </Notice>
        </div>
      )}
      {error && <div className="mb-6"><Notice tone="error" action={<Button size="sm" variant="ghost" onClick={() => setError(null)}>Dismiss</Button>}>{error}</Notice></div>}
      {KINDS.map((k) => {
        const row = rows.find((r) => r.category === k.id);
        if (!row) return null;
        return (
          <section key={k.id} aria-labelledby={`style-${k.id}`} className="mb-8">
            <h2 id={`style-${k.id}`} className="text-lg font-semibold">{k.title}</h2>
            <p className="mt-0.5 text-sm text-muted">{k.apps}</p>
            <RadioCards label={`Tone for ${k.title}`} value={row.tone} onChange={(t) => choose(k.id, t)} options={k.options}
              className="mt-3 sm:grid-cols-3" />
          </section>
        );
      })}
    </>
  );
}
