// The design-system check page: every primitive in light and dark. Screenshot this after token changes.
import { useState } from "react";
import { Mic, Trash2 } from "lucide-react";
import {
  Button, EmptyState, IconButton, KeyCaps, Menu, Modal, Notice, PageHeader, PermissionRow,
  RadioCards, SelectMenu, SkeletonRows, StatTile, Switch, TextInput,
} from "../../ui";

type Theme = "system" | "light" | "dark";

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section className="mb-8">
      <h2 className="mb-3 text-xs font-semibold uppercase tracking-wide text-muted">{title}</h2>
      <div className="flex flex-col gap-3">{children}</div>
    </section>
  );
}

export function Design() {
  const [theme, setTheme] = useState<Theme>(() => (document.documentElement.dataset.theme as Theme) ?? "system");
  const [word, setWord] = useState("Aneed");
  const [on, setOn] = useState(true);
  const [tone, setTone] = useState("formal");
  const [level, setLevel] = useState<"none" | "light" | "medium">("light");
  const [modal, setModal] = useState(false);

  const applyTheme = (t: Theme) => {
    setTheme(t);
    if (t === "system") delete document.documentElement.dataset.theme;
    else document.documentElement.dataset.theme = t;
  };

  return (
    <>
      <PageHeader
        title="Design system"
        description="Every UI primitive in light and dark, for checking token changes."
        actions={<SelectMenu label="Theme" value={theme} onChange={applyTheme} options={[{ value: "system", label: "Match macOS" }, { value: "light", label: "Light" }, { value: "dark", label: "Dark" }]} />}
      />
      <Section title="Buttons">
        <div className="flex flex-wrap gap-2">
          <Button variant="primary">Primary</Button>
          <Button>Secondary</Button>
          <Button variant="ghost">Ghost</Button>
          <Button variant="danger" icon={<Trash2 className="size-4" />}>Delete</Button>
          <Button variant="primary" loading>Saving</Button>
          <Button disabled>Disabled</Button>
          <Button size="sm">Small</Button>
          <Button size="lg" variant="primary">Large</Button>
          <IconButton label="Microphone"><Mic className="size-4" /></IconButton>
        </div>
      </Section>
      <Section title="Inputs">
        <TextInput label="Word" value={word} onChange={(e) => setWord(e.target.value)} max={60} placeholder="A name or term" />
        <TextInput label="With error" value="aneed" onChange={() => {}} error="That word is already in your dictionary" />
        <TextInput label="Search" hideLabel placeholder="Search…" value="" onChange={() => {}} />
      </Section>
      <Section title="Switch, radio cards, select">
        <div className="rounded-lg border border-border px-4">
          <Switch label="Play sounds" description="A short tone when listening starts and stops." checked={on} onChange={setOn} />
          <Switch label="Share data" description="Example of a setting someone else controls." checked={false} onChange={() => {}} lockedReason="Locked by policy" />
        </div>
        <RadioCards label="Tone" value={tone} onChange={setTone} options={[
          { value: "formal", title: "Polished", body: "Full sentences, capitals and final periods." },
          { value: "casual", title: "Relaxed", body: "Lighter punctuation, no final period in chats." },
          { value: "very_casual", title: "Texting", body: "Lowercase, minimal punctuation.", disabled: true },
        ]} />
        <SelectMenu label="Cleanup" value={level} onChange={setLevel} options={[{ value: "none", label: "Off" }, { value: "light", label: "Light" }, { value: "medium", label: "Medium" }]} />
      </Section>
      <Section title="Feedback">
        <Notice>Plain information.</Notice>
        <Notice tone="success">Saved.</Notice>
        <Notice tone="warning" action={<Button size="sm">Fix</Button>}>Something needs attention.</Notice>
        <Notice tone="error">Something failed.</Notice>
        <PermissionRow title="Type into other apps" body="Example permission, not granted." granted={false} onAllow={() => {}} />
        <PermissionRow title="Listen for the fn key" body="Example permission, granted." granted onAllow={() => {}} />
      </Section>
      <Section title="Data">
        <div className="grid grid-cols-3 gap-3">
          <StatTile label="Words this week" value="4,812" sub="+12% on last week" />
          <StatTile label="Speaking speed" value="148" sub="words per minute" />
          <StatTile label="Sessions" value="96" />
        </div>
        <SkeletonRows n={2} />
        <EmptyState icon={<Mic className="size-8" />} title="Empty state" body="One line on what goes here and how to add the first item." action={<Button variant="primary">Add first item</Button>} />
      </Section>
      <Section title="Keys, menu, dialog">
        <div className="flex items-center gap-4">
          <KeyCaps keys={["Fn"]} /> <KeyCaps keys={["Fn", "Space"]} /> <KeyCaps keys={["Ctrl", "Cmd", "V"]} />
          <Menu trigger={<Button>Menu</Button>} items={[{ label: "Copy", onSelect: () => {} }, { label: "Delete", onSelect: () => {}, danger: true }]} />
          <Button onClick={() => setModal(true)}>Open dialog</Button>
        </div>
        <Modal open={modal} onOpenChange={setModal} title="Remove this word?" description="It will no longer guide spelling."
          footer={<><Button onClick={() => setModal(false)} autoFocus>Keep it</Button><Button variant="danger" onClick={() => setModal(false)}>Remove</Button></>} />
      </Section>
      <Section title="Dictation bar">
        <div className="flex flex-wrap items-center gap-4 rounded-lg bg-surface-2 p-6">
          <span className="h-2 w-11 rounded-pill bg-bar-bg opacity-60" title="idle" />
          <span className="flex h-9 items-center gap-2 rounded-pill bg-bar-bg px-3 shadow-bar">
            <span className="size-2 rounded-pill bg-bar-rec" />
            {[6, 14, 20, 11, 5].map((h, i) => <span key={i} className="w-[3px] rounded-pill bg-bar-level" style={{ height: h }} />)}
          </span>
          <span className="flex h-9 items-center gap-1.5 rounded-pill bg-bar-bg px-3 text-xs text-bar-text shadow-bar">No microphone found</span>
        </div>
      </Section>
    </>
  );
}
