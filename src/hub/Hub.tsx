// The main window: sidebar + one screen at a time.
import { useEffect, useState, type ComponentType } from "react";
import { BarChart3, BookA, Home as HomeIcon, Palette, Settings as SettingsIcon, Sparkles, SwatchBook, TextQuote } from "lucide-react";
import { cx } from "../ui";
import { Home } from "./screens/Home";
import { Setup } from "./screens/Setup";
import { Design } from "./screens/Design";
import { Dictionary, Insights, Settings, Snippets, Style } from "./screens/Stubs";

export type Route = "home" | "dictionary" | "snippets" | "style" | "insights" | "settings" | "setup" | "design";
export type Nav = (r: Route) => void;

const MAIN: { id: Route; label: string; icon: ComponentType<{ className?: string }> }[] = [
  { id: "home", label: "Home", icon: HomeIcon },
  { id: "dictionary", label: "Dictionary", icon: BookA },
  { id: "snippets", label: "Snippets", icon: TextQuote },
  { id: "style", label: "Style", icon: Palette },
  { id: "insights", label: "Insights", icon: BarChart3 },
];
const BOTTOM: typeof MAIN = [
  { id: "setup", label: "Setup", icon: Sparkles },
  { id: "design", label: "Design system", icon: SwatchBook },
  { id: "settings", label: "Settings", icon: SettingsIcon },
];

const SCREENS: Record<Route, ComponentType<{ nav: Nav }>> = {
  home: Home, dictionary: Dictionary, snippets: Snippets, style: Style, insights: Insights,
  settings: Settings, setup: Setup, design: Design,
};

export function Hub() {
  const [route, setRoute] = useState<Route>(() => (location.hash.slice(1) as Route) in SCREENS ? (location.hash.slice(1) as Route) : "home");
  useEffect(() => { history.replaceState(null, "", `#${route}`); }, [route]);
  // Cmd+, opens Settings, like every Mac app.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => { if (e.metaKey && e.key === ",") { e.preventDefault(); setRoute("settings"); } };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);
  const Screen = SCREENS[route];

  const item = (r: (typeof MAIN)[number]) => (
    <a
      key={r.id}
      href={`#${r.id}`}
      aria-current={route === r.id ? "page" : undefined}
      onClick={(e) => { e.preventDefault(); setRoute(r.id); }}
      className={cx(
        "flex h-8 items-center gap-2.5 rounded-md px-2.5 text-base font-medium",
        route === r.id ? "bg-accent-soft text-fg" : "text-muted hover:bg-surface-2 hover:text-fg",
      )}
    >
      <r.icon className={cx("size-[18px]", route === r.id && "text-accent")} />
      {r.label}
    </a>
  );

  return (
    <div className="flex h-full">
      <aside className="flex w-[var(--sidebar)] shrink-0 flex-col border-r border-border bg-surface">
        {/* Space for the inset traffic lights; draggable like a title bar. */}
        <div data-tauri-drag-region className="h-[52px] shrink-0" />
        <nav aria-label="Main" className="flex flex-1 flex-col gap-0.5 px-3">{MAIN.map(item)}</nav>
        <nav aria-label="App" className="flex flex-col gap-0.5 px-3 pb-3">{BOTTOM.map(item)}</nav>
      </aside>
      <main className="flex-1 overflow-y-auto">
        <div data-tauri-drag-region className="sticky top-0 z-10 h-[52px] bg-bg/80 backdrop-blur" />
        <div className="mx-auto max-w-[var(--content-max)] px-8 pb-12">
          <Screen nav={setRoute} />
        </div>
      </main>
    </div>
  );
}
