// The main window: sidebar + one screen at a time.
import { useEffect, useState, type ComponentType } from "react";
import { BarChart3, BookA, Cpu, History, Palette, Settings as SettingsIcon, Sparkles, SwatchBook, TextQuote } from "lucide-react";
import { cx } from "../ui";
import { Home } from "./screens/Home";
import { Setup } from "./screens/Setup";
import { Design } from "./screens/Design";
import { Dictionary } from "./screens/Dictionary";
import { Snippets } from "./screens/Snippets";
import { Settings } from "./screens/Settings";
import { Insights } from "./screens/Stubs";
import { Style } from "./screens/Style";
import { Models } from "./screens/Models";
import { Footer } from "./parts/Footer";
import logo from "../../assets/icon.svg";
import { api, applyTheme, events } from "../lib/api";

export type Route = "home" | "models" | "dictionary" | "snippets" | "style" | "insights" | "settings" | "setup" | "design";
export type Nav = (r: Route) => void;

const MAIN: { id: Route; label: string; icon: ComponentType<{ className?: string }> }[] = [
  { id: "home", label: "History", icon: History },
  { id: "models", label: "Models", icon: Cpu },
  { id: "dictionary", label: "Dictionary", icon: BookA },
  { id: "snippets", label: "Snippets", icon: TextQuote },
  { id: "style", label: "Style", icon: Palette },
  // Placeholder screen, dev builds only until it has content.
  ...(import.meta.env.DEV ? [{ id: "insights" as Route, label: "Insights", icon: BarChart3 }] : []),
];
const BOTTOM: typeof MAIN = [
  { id: "setup", label: "Setup", icon: Sparkles },
  ...(import.meta.env.DEV ? [{ id: "design" as Route, label: "Design system", icon: SwatchBook }] : []), // developer page
  { id: "settings", label: "Settings", icon: SettingsIcon },
];

const SCREENS: Record<Route, ComponentType<{ nav: Nav }>> = {
  home: Home, models: Models, dictionary: Dictionary, snippets: Snippets, style: Style, insights: Insights,
  settings: Settings, setup: Setup, design: Design,
};

export function Hub() {
  const [route, setRoute] = useState<Route>(() => (location.hash.slice(1) as Route) in SCREENS ? (location.hash.slice(1) as Route) : "home");
  useEffect(() => { history.replaceState(null, "", `#${route}`); }, [route]);
  // Onboarding fills the window until it is finished, then Setup in the sidebar opens it again.
  const [onboarded, setOnboarded] = useState<boolean | null>(null);
  // The saved theme; "system" follows macOS through the CSS media query.
  useEffect(() => {
    api.settings().then((st) => { applyTheme(st.theme); setOnboarded(st.onboarding_done); }).catch(() => setOnboarded(true));
    const un = events.onSettings((st) => setOnboarded(st.onboarding_done));
    return () => { un.then((f) => f()); };
  }, []);
  // Cmd+, opens Settings, like every Mac app.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => { if (e.metaKey && e.key === ",") { e.preventDefault(); setRoute("settings"); } };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);
  const Screen = SCREENS[route];
  if (onboarded === null) return <div className="h-full bg-bg" />; // settings not read yet: no flash of Home
  if (!onboarded || route === "setup") {
    return <div className="h-full bg-bg"><Setup nav={(r) => { setOnboarded(true); setRoute(r === "setup" ? "home" : r); }} /></div>;
  }

  const item = (r: (typeof MAIN)[number]) => (
    <a
      key={r.id}
      href={`#${r.id}`}
      aria-current={route === r.id ? "page" : undefined}
      onClick={(e) => { e.preventDefault(); setRoute(r.id); }}
      className={cx(
        "flex h-9 items-center gap-2.5 rounded-md px-2.5 text-base font-medium",
        route === r.id ? "bg-accent text-on-accent" : "text-fg hover:bg-surface-2",
      )}
    >
      <r.icon className="size-[18px]" />
      {r.label}
    </a>
  );

  return (
    <div className="flex h-full flex-col">
      <div className="flex min-h-0 flex-1">
        <aside className="flex w-[var(--sidebar)] shrink-0 flex-col border-r border-border bg-bg">
          {/* Space for the inset traffic lights; draggable like a title bar. */}
          <div data-tauri-drag-region className="h-[44px] shrink-0" />
          <div data-tauri-drag-region className="mx-3 mb-3 flex items-center gap-2 border-b border-border px-1 pb-3">
            <img src={logo} alt="" className="size-8" />
            <span className="text-lg font-semibold">Quietkey</span>
          </div>
          <nav aria-label="Main" className="flex flex-1 flex-col gap-1 px-3">{MAIN.map(item)}</nav>
          <nav aria-label="App" className="flex flex-col gap-1 px-3 pb-3">{BOTTOM.map(item)}</nav>
        </aside>
        <main className="flex-1 overflow-y-auto bg-bg">
          <div data-tauri-drag-region className="sticky top-0 z-10 h-[44px] bg-bg/80 backdrop-blur" />
          <div className="mx-auto max-w-[var(--content-max)] px-6 pb-10">
            <Screen nav={setRoute} />
          </div>
        </main>
      </div>
      <Footer nav={setRoute} />
    </div>
  );
}
