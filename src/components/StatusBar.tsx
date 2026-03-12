import { A } from "@solidjs/router";
import { Show } from "solid-js";

export type StatusBarProps = {
  sidebarOpen: boolean;
  onToggleSidebar: () => void;
  preflightPhase: string;
  completedChecks: number;
  totalChecks: number;
  appVersion: string;
};

function SidebarToggle(props: { sidebarOpen: boolean; onToggleSidebar: () => void }) {
  return (
    <button
      type="button"
      class="rounded-lg border border-overlay bg-surface/35 px-3 py-1.5 text-xs font-semibold text-subtext transition hover:border-accent/40 hover:text-text"
      onClick={() => props.onToggleSidebar()}>
      <Show
        when={props.sidebarOpen}
        fallback={
          <span class="flex items-center">
            <span class="sr-only">Hide navigation</span>
            <i class="h-4 w-4 i-bi-chevron-bar-right" />
          </span>
        }>
        <span class="flex items-center">
          <span class="sr-only">Show navigation</span>
          <i class="h-4 w-4 i-bi-chevron-bar-left" />
        </span>
      </Show>
    </button>
  );
}

export function StatusBar(props: StatusBarProps) {
  return (
    <footer class="fixed inset-x-0 bottom-0 z-40 h-12 border-t border-overlay bg-black/95 backdrop-blur">
      <div class="mx-auto flex h-full w-full flex-wrap items-center justify-between gap-4 px-2 md:px-4">
        <div class="flex flex-wrap items-center gap-2">
          <SidebarToggle sidebarOpen={props.sidebarOpen} onToggleSidebar={props.onToggleSidebar} />
          <A
            href="/library"
            class="rounded-lg border border-overlay/70 bg-surface/30 px-3 py-1.5 text-xs font-semibold text-subtext transition hover:border-accent/40 hover:text-text"
            activeClass="!border-accent/60 !bg-accent/15 !text-text">
            Home
          </A>
          <A
            href="/splash"
            class="rounded-lg border border-overlay/70 bg-surface/30 px-3 py-1.5 text-xs font-semibold text-subtext transition hover:border-accent/40 hover:text-text"
            activeClass="!border-accent/60 !bg-accent/15 !text-text">
            Splash
          </A>
          <A
            href="/setup"
            class="rounded-lg border border-overlay/70 bg-surface/30 px-3 py-1.5 text-xs font-semibold text-subtext transition hover:border-accent/40 hover:text-text"
            activeClass="!border-accent/60 !bg-accent/15 !text-text">
            Setup
          </A>
        </div>
        <p class="flex gap-1 text-right text-[11px] font-semibold tracking-[0.14em] text-subtext uppercase md:text-xs">
          PREFLIGHT <span class="text-text">{props.preflightPhase}</span>
          <span>{props.completedChecks}/{props.totalChecks} checks complete</span>
          <span class="hidden md:inline">|</span>
          <span class="text-accent normal-case tracking-normal">{props.appVersion}</span>
        </p>
      </div>
    </footer>
  );
}
