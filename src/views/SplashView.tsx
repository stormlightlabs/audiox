import { useNavigate } from "@solidjs/router";
import { createEffect, For, onCleanup, Show } from "solid-js";
import { Motion } from "solid-motionone";
import { Accordion } from "../components/Accordion";
import {
  type CheckDisplayStatus,
  PREFLIGHT_CHECK_ORDER,
  type PreflightCheck,
  useAppContext,
} from "../state/AppContext";

type CheckMeta = { key: PreflightCheck; title: string };

const checkMeta: CheckMeta[] = [
  { key: "whisper_cli", title: "Whisper sidecar" },
  { key: "ffmpeg", title: "FFmpeg sidecar" },
  { key: "yt_dlp", title: "yt-dlp sidecar (optional)" },
  { key: "whisper_model", title: "Whisper model file" },
  { key: "ollama_server", title: "Ollama server" },
  { key: "ollama_models", title: "Ollama model set" },
  { key: "database", title: "SQLite database" },
];

function statusClass(status: CheckDisplayStatus, running: boolean): string {
  if (running) {
    return "text-accent";
  }

  switch (status) {
    case "pass": {
      return "text-accent";
    }
    case "warn": {
      return "text-subtext";
    }
    case "fail": {
      return "text-text";
    }
    default: {
      return "text-subtext";
    }
  }
}

function statusLabel(status: CheckDisplayStatus, running: boolean): string {
  if (running) {
    return "running";
  }

  switch (status) {
    case "pass": {
      return "pass";
    }
    case "warn": {
      return "warn";
    }
    case "fail": {
      return "fail";
    }
    default: {
      return "pending";
    }
  }
}

function StatusGlyph(props: { status: CheckDisplayStatus; running: boolean }) {
  if (props.running) {
    return (
      <span class="inline-block size-4 rounded-full border-2 border-accent/40 border-t-accent align-middle animate-spin" />
    );
  }

  switch (props.status) {
    case "pass": {
      return <span class="text-accent">✓</span>;
    }
    case "warn": {
      return <span class="text-subtext">!</span>;
    }
    case "fail": {
      return <span class="text-text">✕</span>;
    }
    default: {
      return <span class="text-subtext">•</span>;
    }
  }
}

function GuidancePanel(props: { messages: string[] }) {
  return (
    <section class="rounded-xl border border-overlay bg-surface/50 p-4">
      <p class="text-xs font-semibold tracking-[0.16em] text-subtext uppercase">Guidance</p>
      <ul class="mt-2 grid gap-2">
        <For each={props.messages}>{(message) => <li class="text-sm text-subtext">{message}</li>}</For>
      </ul>
    </section>
  );
}

export function SplashView() {
  const navigate = useNavigate();
  const { state, runPreflight, completeStartupFlow } = useAppContext();

  const currentRunningIndex = () => {
    if (state.preflightPhase !== "running") {
      return -1;
    }
    return PREFLIGHT_CHECK_ORDER.findIndex((check) => state.checklist[check].status === "pending");
  };

  createEffect(() => {
    const preflight = state.preflightResult;
    if (!preflight) {
      return;
    }

    let timeoutId: ReturnType<typeof setTimeout> | undefined;
    if (!state.startupFlowActive) {
      return;
    }

    if (preflight.should_open_setup) {
      timeoutId = setTimeout(() => {
        void navigate("/setup");
      }, 900);
    } else if (preflight.all_required_passed) {
      timeoutId = setTimeout(() => {
        completeStartupFlow();
        void navigate("/library");
      }, 700);
    }

    onCleanup(() => {
      if (timeoutId) {
        clearTimeout(timeoutId);
      }
    });
  });

  const guidance = () => {
    if (!state.preflightResult) {
      return [];
    }
    return state.preflightResult.details.filter((detail) => detail.status === "fail" || detail.status === "warn");
  };

  return (
    <section class="w-full max-w-3xl rounded-4xl border border-overlay bg-elevation/85 p-6 shadow-2xl shadow-surface/70 md:p-10">
      <div class="grid gap-6">
        <header class="space-y-4 text-center">
          <div class="mx-auto grid size-16 place-content-center rounded-2xl border border-accent/40 bg-accent/10 text-xl font-bold text-accent">
            AX
          </div>
          <div class="space-y-2">
            <p class="text-xs tracking-[0.24em] text-subtext uppercase">Startup Preflight</p>
            <h1 class="font-display text-4xl text-text">Audio X</h1>
            <p class="text-sm text-subtext">
              Checking sidecars, models, Ollama, and local database before opening the app.
            </p>
          </div>
        </header>

        <Accordion
          id="preflight-checklist"
          title="Preflight checklist"
          summary={`${state.completedChecks}/${PREFLIGHT_CHECK_ORDER.length} checks complete`}
          defaultOpen
          class="rounded-2xl border border-overlay bg-surface/45"
          headerClass="px-4 py-3 md:px-5"
          contentClass="space-y-3 px-4 pb-4 md:px-5 md:pb-5">
          <For each={checkMeta}>
            {(item, index) => {
              const checkState = () => state.checklist[item.key];
              const running = () => currentRunningIndex() === index();

              return (
                <Motion.div
                  initial={{ opacity: 0, y: 8 }}
                  animate={{ opacity: 1, y: 0 }}
                  transition={{ duration: 0.2, delay: index() * 0.05 }}
                  class="rounded-xl border border-overlay/75 bg-elevation/75 px-4 py-3">
                  <div class="flex items-center justify-between gap-3">
                    <div class="flex items-center gap-3">
                      <StatusGlyph status={checkState().status} running={running()} />
                      <p class="text-sm font-semibold text-text">{item.title}</p>
                    </div>
                    <span
                      class={`text-xs font-semibold tracking-[0.16em] uppercase ${
                        statusClass(checkState().status, running())
                      }`}>
                      {statusLabel(checkState().status, running())}
                    </span>
                  </div>
                  <Show when={checkState().message}>
                    <p class="mt-2 text-xs leading-relaxed text-subtext">{checkState().message}</p>
                  </Show>
                </Motion.div>
              );
            }}
          </For>
        </Accordion>

        <Show when={state.preflightError}>
          {(preflightError) => (
            <p role="alert" class="rounded-xl border border-accent/50 bg-accent/10 p-3 text-sm text-text">
              {preflightError()}
            </p>
          )}
        </Show>

        <Show when={guidance().length > 0 && state.preflightPhase === "failed"}>
          <GuidancePanel messages={guidance().map((item) => item.message)} />
        </Show>

        <div class="flex flex-wrap items-center justify-between gap-3">
          <p class="text-xs text-subtext">
            <Show when={state.preflightPhase === "running"} fallback={<span>Preflight finished.</span>}>
              Running startup checks...
            </Show>
          </p>
          <button
            type="button"
            class="rounded-xl bg-accent px-4 py-2 text-sm font-semibold text-surface transition hover:brightness-110 disabled:cursor-not-allowed disabled:opacity-60"
            onClick={() => {
              void runPreflight();
            }}
            disabled={state.preflightPhase === "running"}>
            Retry checks
          </button>
        </div>
      </div>
    </section>
  );
}
