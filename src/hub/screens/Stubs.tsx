// Screens that are routed but not built yet. Their backend commands already exist (see lib/api.ts).
import { BarChart3, BookA, Palette, Settings as SettingsIcon, TextQuote } from "lucide-react";
import type { ReactNode } from "react";
import { EmptyState, PageHeader } from "../../ui";

function Stub({ title, description, icon }: { title: string; description: string; icon: ReactNode }) {
  return (
    <>
      <PageHeader title={title} description={description} />
      <EmptyState icon={icon} title="Coming soon" body="This screen is not built yet." />
    </>
  );
}

export const Dictionary = () => (
  <Stub title="Dictionary" description="Names and words you want spelled your way." icon={<BookA className="size-8" />} />
);
export const Snippets = () => (
  <Stub title="Snippets" description="Say a short cue, get a longer text." icon={<TextQuote className="size-8" />} />
);
export const Style = () => (
  <Stub title="Style" description="How polished your text should sound, per kind of app." icon={<Palette className="size-8" />} />
);
export const Insights = () => (
  <Stub title="Insights" description="How much you dictate and how fast." icon={<BarChart3 className="size-8" />} />
);
export const Settings = () => (
  <Stub title="Settings" description="Shortcuts, microphone, languages, privacy." icon={<SettingsIcon className="size-8" />} />
);
