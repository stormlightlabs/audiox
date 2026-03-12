import { A, Navigate, Route, Router, useLocation, useNavigate } from "@solidjs/router";
import { createEffect, createSignal, For, onCleanup, onMount, type ParentProps, Show } from "solid-js";
import { Motion } from "solid-motionone";
import { AppProvider, useAppContext } from "./state/AppContext";
import { DocumentView } from "./views/DocumentView";
import { ImportView } from "./views/ImportView";
import { LibraryView } from "./views/LibraryView";
import { RecordView } from "./views/RecordView";
import { SettingsView } from "./views/SettingsView";
import { SetupView } from "./views/SetupView";
import { SplashView } from "./views/SplashView";

type NavItem = { href: string; label: string; tagline: string };

const navItems: NavItem[] = [
  { href: "/splash", label: "Splash", tagline: "runtime checks" },
  { href: "/setup", label: "Setup", tagline: "first run" },
  { href: "/record", label: "Record", tagline: "microphone" },
  { href: "/import", label: "Import", tagline: "files and urls" },
  { href: "/library", label: "Library", tagline: "document index" },
  { href: "/document", label: "Document", tagline: "reader" },
  { href: "/settings", label: "Settings", tagline: "preferences" },
];

function windowTitleForPath(pathname: string): string {
  if (pathname === "/splash") {
    return "Audio X - Preflight";
  }
  if (pathname === "/setup") {
    return "Audio X - Setup Wizard";
  }
  if (pathname === "/record") {
    return "Audio X - Microphone Recording";
  }
  if (pathname === "/import") {
    return "Audio X - Audio Import";
  }
  if (pathname === "/library") {
    return "Audio X - Document Library";
  }
  if (pathname.startsWith("/document")) {
    return "Audio X - Document";
  }
  if (pathname === "/settings") {
    return "Audio X - Settings";
  }
  return "Audio X";
}

function BootStatus() {
  const { state } = useAppContext();
  return (
    <div class="rounded-2xl border border-overlay bg-raised/80 p-3">
      <p class="text-xs font-semibold tracking-[0.2em] text-subtext uppercase">Preflight</p>
      <p class="mt-2 text-sm text-text">{state.preflightPhase}</p>
      <p class="mt-1 text-xs text-subtext">{state.completedChecks}/7 checks complete</p>
    </div>
  );
}

function SideNavigation() {
  return (
    <aside class="flex w-full flex-col gap-6 rounded-3xl border border-overlay bg-elevation/90 p-5 shadow-2xl shadow-surface/50 backdrop-blur md:w-72 md:self-stretch">
      <header class="space-y-2">
        <p class="font-display text-2xl tracking-wide text-text">Audio X</p>
        <p class="text-xs tracking-[0.2em] text-subtext uppercase">Desktop shell</p>
      </header>
      <nav class="grid gap-2">
        <For each={navItems}>
          {(item) => (
            <A
              href={item.href}
              aria-label={item.label}
              class="group rounded-2xl border border-overlay/60 bg-surface/25 px-3 py-2 transition hover:border-accent/35 hover:bg-raised/90"
              activeClass="!border-accent/60 !bg-accent/15">
              <p class="text-sm font-semibold text-text">{item.label}</p>
              <p class="text-xs text-subtext">{item.tagline}</p>
            </A>
          )}
        </For>
      </nav>
      <BootStatus />
    </aside>
  );
}

function ShellLayout(props: ParentProps) {
  const location = useLocation();
  const navigate = useNavigate();
  const [sidebarOpen, setSidebarOpen] = createSignal(true);
  const isSplashRoute = () => location.pathname === "/splash";

  createEffect(() => {
    if (typeof document === "undefined") {
      return;
    }
    document.title = windowTitleForPath(location.pathname);
  });

  onMount(() => {
    const keydownHandler = (event: KeyboardEvent) => {
      const commandKey = event.metaKey || event.ctrlKey;
      if (!commandKey || event.defaultPrevented || event.altKey || event.shiftKey) {
        return;
      }

      const key = event.key.toLowerCase();
      if (key === "n") {
        event.preventDefault();
        void navigate("/record");
        return;
      }

      if (key === "f") {
        event.preventDefault();
        void navigate("/library");
        globalThis.dispatchEvent(new CustomEvent("audiox:focus-library-search"));
        return;
      }

      if (key === "i") {
        event.preventDefault();
        void navigate("/import");
      }
    };

    globalThis.addEventListener("keydown", keydownHandler);
    onCleanup(() => {
      globalThis.removeEventListener("keydown", keydownHandler);
    });
  });

  return (
    <div class="relative min-h-screen bg-surface px-4 py-5 text-text md:px-8 md:py-8">
      <div class="absolute inset-0 -z-10 bg-[radial-gradient(circle_at_14%_12%,rgba(40,90,140,0.35),transparent_42%),radial-gradient(circle_at_88%_8%,rgba(17,31,56,0.5),transparent_45%),linear-gradient(160deg,#03050a,#070d17_55%,#05070d)]" />
      <Show
        when={isSplashRoute()}
        fallback={
          <div class="mx-auto flex w-full max-w-7xl flex-col gap-4 md:min-h-[calc(100vh-4rem)]">
            <div class="flex items-center justify-between rounded-2xl border border-overlay bg-elevation/70 px-3 py-2">
              <p class="text-xs font-semibold tracking-[0.16em] text-subtext uppercase">
                {sidebarOpen() ? "Navigation expanded" : "Navigation hidden"}
              </p>
              <button
                type="button"
                class="rounded-lg border border-overlay bg-surface/35 px-3 py-1.5 text-xs font-semibold text-subtext transition hover:border-accent/40 hover:text-text"
                onClick={() => {
                  setSidebarOpen((open) => !open);
                }}>
                {sidebarOpen() ? "Hide sidebar" : "Show sidebar"}
              </button>
            </div>

            <div class="flex w-full flex-col gap-6 md:flex-row">
              <Show when={sidebarOpen()}>
                <SideNavigation />
              </Show>
              <main class="flex-1 rounded-3xl border border-overlay bg-elevation/60 p-6 shadow-2xl shadow-surface/50 md:p-8">
                <Motion.div
                  initial={{ opacity: 0, y: 10 }}
                  animate={{ opacity: 1, y: 0 }}
                  transition={{ duration: 0.25 }}>
                  {props.children}
                </Motion.div>
              </main>
            </div>
          </div>
        }>
        <main class="mx-auto flex w-full max-w-5xl items-center justify-center py-6 md:min-h-[calc(100vh-4rem)]">
          <Motion.div initial={{ opacity: 0, y: 10 }} animate={{ opacity: 1, y: 0 }} transition={{ duration: 0.25 }}>
            {props.children}
          </Motion.div>
        </main>
      </Show>
    </div>
  );
}

function RootRoute(props: ParentProps) {
  return (
    <AppProvider>
      <ShellLayout>{props.children}</ShellLayout>
    </AppProvider>
  );
}

function RedirectToSplash() {
  return <Navigate href="/splash" />;
}

function App() {
  return (
    <Router root={RootRoute}>
      <Route path="/" component={RedirectToSplash} />
      <Route path="/splash" component={SplashView} />
      <Route path="/setup" component={SetupView} />
      <Route path="/record" component={RecordView} />
      <Route path="/import" component={ImportView} />
      <Route path="/library" component={LibraryView} />
      <Route path="/document" component={DocumentView} />
      <Route path="/document/:id" component={DocumentView} />
      <Route path="/settings" component={SettingsView} />
    </Router>
  );
}

export default App;
