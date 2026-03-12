import { A, Navigate, Route, Router, useLocation, useNavigate } from "@solidjs/router";
import { invoke } from "@tauri-apps/api/core";
import * as logger from "@tauri-apps/plugin-log";
import { openUrl } from "@tauri-apps/plugin-opener";
import { createEffect, createSignal, For, onCleanup, onMount, type ParentProps, Show } from "solid-js";
import { Motion, Presence } from "solid-motionone";
import { Accordion } from "./components/Accordion";
import { StatusBar } from "./components/StatusBar";
import { normalizeError } from "./errors";
import { AppProvider, PREFLIGHT_CHECK_ORDER, useAppContext } from "./state/AppContext";
import { DocumentView } from "./views/DocumentView";
import { ImportView } from "./views/ImportView";
import { LibraryView } from "./views/LibraryView";
import { RecordView } from "./views/RecordView";
import { SettingsView } from "./views/SettingsView";
import { SetupView } from "./views/SetupView";
import { SplashView } from "./views/SplashView";

type NavItem = { href: string; label: string; tagline: string };
type FundingLink = { name: string; url: string; description: string };

const navItems: NavItem[] = [
  { href: "/library", label: "Library", tagline: "Browse your transcripts" },
  { href: "/import", label: "Import", tagline: "Ingest files" },
  { href: "/record", label: "Record", tagline: "Capture audio" },
  { href: "/settings", label: "Settings", tagline: "Devices and preferences" },
];

const fundingLinks: FundingLink[] = [
  {
    name: "GitHub Sponsors",
    url: "https://github.com/sponsors/desertthunder",
    description: "Sponsor ongoing open-source development",
  },
  { name: "Ko-fi", url: "https://ko-fi.com/desertthunder", description: "Buy me a coffee" },
  { name: "Source Code", url: "https://github.com/stormlightlabs/audiox", description: "View the source code" },
];

async function openFundingLink(url: string) {
  try {
    await openUrl(url);
  } catch (error) {
    logger.warn(`Failed to open ${url}`, { keyValues: { error: normalizeError(error) } });
  }
}

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
    return "Audio X - Import";
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

function isTauriRuntime(): boolean {
  return Boolean((globalThis as typeof globalThis & { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__);
}

function SideNavigation() {
  return (
    <Motion.aside
      initial={{ opacity: 0, x: -18, scale: 0.985 }}
      animate={{ opacity: 1, x: 0, scale: 1 }}
      exit={{ opacity: 0, x: -18, scale: 0.985 }}
      transition={{ duration: 0.22 }}
      class="flex h-full min-h-0 w-full flex-col gap-6 overflow-y-auto border-r border-overlay bg-elevation/90 p-5 pb-6 shadow-2xl shadow-surface/50 backdrop-blur md:w-72 md:self-stretch">
      <header class="space-y-2">
        <p class="font-display text-2xl tracking-wide text-text">Audio X</p>
        <p class="text-xs tracking-[0.2em] text-subtext uppercase">Navigation</p>
      </header>
      <nav class="grid gap-2">
        <For each={navItems}>
          {(item) => (
            <A
              href={item.href}
              aria-label={item.label}
              class="group rounded-xl border border-overlay/60 bg-surface/25 px-3 py-2 transition hover:border-accent/35 hover:bg-raised/90"
              activeClass="!border-accent/60 !bg-accent/15">
              <p class="text-sm font-semibold text-text">{item.label}</p>
              <p class="text-xs text-subtext">{item.tagline}</p>
            </A>
          )}
        </For>
      </nav>
      <div class="space-y-3 border-t border-overlay/70 pt-4">
        <div>
          <p class="text-[11px] font-semibold tracking-[0.16em] text-subtext uppercase">Support this project</p>
          <p class="mt-1 text-xs text-subtext">Audio X is free to use and funded by community support.</p>
        </div>

        <Accordion
          id="support-why-matters"
          title="Why support matters"
          summary="How support keeps Audio X sustainable."
          class="rounded-lg border border-overlay/70 bg-surface/20"
          headerClass="px-3 py-2"
          contentClass="px-3 pb-3">
          <p class="text-xs leading-relaxed text-subtext">
            Support keeps Audio X independent, privacy-first, open source, and steadily improving.
          </p>
          <div class="mt-2 grid grid-cols-2 gap-2 text-[11px] text-subtext">
            <span class="rounded-md border border-overlay/60 bg-surface/20 px-2 py-1">Independent</span>
            <span class="rounded-md border border-overlay/60 bg-surface/20 px-2 py-1">Privacy-first</span>
            <span class="rounded-md border border-overlay/60 bg-surface/20 px-2 py-1">Open source</span>
            <span class="rounded-md border border-overlay/60 bg-surface/20 px-2 py-1">Continuous updates</span>
          </div>
        </Accordion>

        <Accordion
          id="support-ways-to-help"
          title="Ways to help"
          summary="Non-financial ways to contribute."
          class="rounded-lg border border-overlay/70 bg-surface/20"
          headerClass="px-3 py-2"
          contentClass="px-3 pb-3">
          <ul class="list-disc space-y-1 pl-4 text-xs text-subtext">
            <li>Star and share Audio X</li>
            <li>Report bugs and request features</li>
            <li>Contribute code or docs</li>
          </ul>
        </Accordion>

        <div class="rounded-lg border border-overlay/70 bg-surface/20 p-3">
          <p class="text-xs font-semibold text-text">Links</p>
          <div class="mt-2 space-y-2">
            <For each={fundingLinks}>
              {(link) => (
                <button
                  type="button"
                  class="w-full rounded-md border border-overlay/60 bg-surface/20 px-2.5 py-2 text-left transition hover:border-accent/40 hover:bg-accent/10"
                  onClick={() => {
                    void openFundingLink(link.url);
                  }}>
                  <p class="text-xs font-semibold text-text">{link.name}</p>
                  <p class="mt-0.5 text-[11px] text-subtext">{link.description}</p>
                </button>
              )}
            </For>
          </div>
        </div>
      </div>
    </Motion.aside>
  );
}

function ShellLayout(props: ParentProps) {
  const { state } = useAppContext();
  const location = useLocation();
  const navigate = useNavigate();
  const [sidebarOpen, setSidebarOpen] = createSignal(true);
  const [appVersion, setAppVersion] = createSignal("loading...");
  const isSplashRoute = () => location.pathname === "/splash";

  createEffect(() => {
    const title = windowTitleForPath(location.pathname);
    if (typeof document !== "undefined") {
      document.title = title;
    }

    if (!isTauriRuntime()) {
      return;
    }

    void invoke("set_window_title", { title }).catch((error) => {
      logger.warn("Failed to update native window title", { keyValues: { error: normalizeError(error), title } });
    });
  });

  onMount(() => {
    if (isTauriRuntime()) {
      void invoke<string>("get_app_version").then((value) => {
        const normalized = value?.trim();
        if (!normalized) {
          setAppVersion("unknown");
          return;
        }
        setAppVersion(normalized);
      }).catch((error) => {
        logger.warn("Failed to resolve app version", { keyValues: { error: normalizeError(error) } });
        setAppVersion("unknown");
      });
    } else {
      setAppVersion("web");
    }

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
    <div class="relative h-screen overflow-hidden bg-surface text-text">
      <div class="absolute inset-0 -z-10 bg-[radial-gradient(circle_at_14%_12%,rgba(40,90,140,0.35),transparent_42%),radial-gradient(circle_at_88%_8%,rgba(17,31,56,0.5),transparent_45%),linear-gradient(160deg,#03050a,#070d17_55%,#05070d)]" />
      <Show
        when={isSplashRoute()}
        fallback={
          <div class="flex h-[calc(100vh-3rem)] w-full min-h-0 flex-col md:flex-row">
            <Presence>
              <Show when={sidebarOpen()}>
                <SideNavigation />
              </Show>
            </Presence>
            <main class="min-h-0 flex-1 overflow-y-auto bg-elevation/60 p-4 pb-20 shadow-2xl shadow-surface/50 md:h-full md:p-8 md:pb-20">
              <Motion.div
                initial={{ opacity: 0, y: 10 }}
                animate={{ opacity: 1, y: 0 }}
                transition={{ duration: 0.25 }}>
                {props.children}
              </Motion.div>
            </main>
          </div>
        }>
        <main class="mx-auto flex h-[calc(100vh-3rem)] w-full max-w-5xl items-start justify-center overflow-y-auto pt-6 pb-20">
          <Motion.div initial={{ opacity: 0, y: 10 }} animate={{ opacity: 1, y: 0 }} transition={{ duration: 0.25 }}>
            {props.children}
          </Motion.div>
        </main>
      </Show>
      <StatusBar
        sidebarOpen={sidebarOpen()}
        preflightPhase={state.preflightPhase}
        completedChecks={state.completedChecks}
        totalChecks={PREFLIGHT_CHECK_ORDER.length}
        appVersion={appVersion()}
        onToggleSidebar={() => {
          setSidebarOpen((open) => !open);
        }} />
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
      {/* TODO: make these /documents & /documents/:id */}
      {/* TODO: remove empty DocumentView */}
      <Route path="/library" component={LibraryView} />
      <Route path="/document" component={DocumentView} />
      <Route path="/document/:id" component={DocumentView} />
      <Route path="/settings" component={SettingsView} />
    </Router>
  );
}

export default App;
