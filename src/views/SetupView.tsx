import { useNavigate } from "@solidjs/router";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { createSignal, For, onCleanup, onMount, Show } from "solid-js";
import { createStore } from "solid-js/store";
import { type SetupStatus, useAppContext } from "../state/AppContext";
import { ViewScaffold } from "./ViewScaffold";

const WHISPER_PROGRESS_EVENT = "setup://whisper-progress";
const OLLAMA_PROGRESS_EVENT = "setup://ollama-progress";

type StepKey = "whisper_model" | "ollama_server" | "nomic_embed_text" | "gemma";
type StepStatus = "pending" | "running" | "pass" | "fail" | "blocked";
type SetupPhase = "checking" | "idle" | "running" | "failed" | "completed";

type SetupStep = {
  key: StepKey;
  title: string;
  description: string;
  status: StepStatus;
  message: string;
  progress: number;
};

type WhisperProgressEvent = {
  modelName: string;
  status: "running" | "completed" | "error";
  message: string;
  downloadedBytes: number;
  totalBytes: number | null;
  percent: number;
};

type OllamaProgressEvent = {
  modelName: string;
  status: "running" | "completed" | "error";
  message: string;
  completed: number;
  total: number;
  percent: number;
};

const STEP_ORDER: StepKey[] = ["whisper_model", "ollama_server", "nomic_embed_text", "gemma"];

function normalizeError(error: unknown): string {
  if (error instanceof Error) {
    return error.message;
  }
  return String(error);
}

function hasModel(status: SetupStatus, model: string): boolean {
  return !status.missing_ollama_models.includes(model);
}

function modelToStep(modelName: string): StepKey | null {
  if (modelName.startsWith("nomic-embed-text")) {
    return "nomic_embed_text";
  }
  if (modelName.startsWith("gemma3:4b")) {
    return "gemma";
  }
  return null;
}

function progressEventStatus(status: "running" | "completed" | "error"): StepStatus {
  if (status === "error") {
    return "fail";
  }
  if (status === "completed") {
    return "pass";
  }
  return "running";
}

function buildStepMap(status: SetupStatus): Record<StepKey, SetupStep> {
  const ollamaServerStatus: StepStatus = status.ollama_server_ready ? "pass" : "fail";
  const modelStepStatus = status.ollama_server_ready ? "pending" : "blocked";

  return {
    whisper_model: {
      key: "whisper_model",
      title: "Whisper model",
      description: "Download ggml-base.en.bin into appdata/models.",
      status: status.whisper_model_ready ? "pass" : "pending",
      message: status.whisper_model_ready
        ? "Whisper model is available."
        : "Model not found. Setup will download ggml-base.en.bin.",
      progress: status.whisper_model_ready ? 100 : 0,
    },
    ollama_server: {
      key: "ollama_server",
      title: "Ollama server",
      description: "Reachable at http://localhost:11434.",
      status: ollamaServerStatus,
      message: status.ollama_server_ready
        ? "Ollama server is reachable."
        : "Ollama is not reachable. Install Ollama and start it with `ollama serve`.",
      progress: status.ollama_server_ready ? 100 : 0,
    },
    nomic_embed_text: {
      key: "nomic_embed_text",
      title: "nomic-embed-text",
      description: "Embedding model required for semantic features.",
      status: status.ollama_server_ready
        ? (hasModel(status, "nomic-embed-text") ? "pass" : modelStepStatus)
        : "blocked",
      message: status.ollama_server_ready
        ? (hasModel(status, "nomic-embed-text") ? "Model is installed." : "Model is missing and will be pulled.")
        : "Waiting for Ollama server.",
      progress: status.ollama_server_ready && hasModel(status, "nomic-embed-text") ? 100 : 0,
    },
    gemma: {
      key: "gemma",
      title: "gemma3:4b",
      description: "Generation model required for title/summary/tags.",
      status: status.ollama_server_ready ? (hasModel(status, "gemma3:4b") ? "pass" : modelStepStatus) : "blocked",
      message: status.ollama_server_ready
        ? (hasModel(status, "gemma3:4b") ? "Model is installed." : "Model is missing and will be pulled.")
        : "Waiting for Ollama server.",
      progress: status.ollama_server_ready && hasModel(status, "gemma3:4b") ? 100 : 0,
    },
  };
}

function statusLabel(status: StepStatus): string {
  switch (status) {
    case "pass": {
      return "ready";
    }
    case "running": {
      return "running";
    }
    case "fail": {
      return "failed";
    }
    case "blocked": {
      return "blocked";
    }
    default: {
      return "pending";
    }
  }
}

function statusClass(status: StepStatus): string {
  switch (status) {
    case "pass": {
      return "text-accent";
    }
    case "running": {
      return "text-accent";
    }
    case "fail": {
      return "text-text";
    }
    case "blocked": {
      return "text-subtext";
    }
    default: {
      return "text-subtext";
    }
  }
}

function StatusGlyph(props: { status: StepStatus }) {
  switch (props.status) {
    case "pass": {
      return <span class="text-accent">✓</span>;
    }
    case "running": {
      return (
        <span class="inline-block size-4 rounded-full border-2 border-accent/40 border-t-accent align-middle animate-spin" />
      );
    }
    case "fail": {
      return <span class="text-text">✕</span>;
    }
    case "blocked": {
      return <span class="text-subtext">⏸</span>;
    }
    default: {
      return <span class="text-subtext">•</span>;
    }
  }
}

function StepCard(props: { step: SetupStep }) {
  return (
    <article class="rounded-2xl border border-overlay bg-elevation/70 p-4 sm:p-5">
      <div class="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
        <div class="space-y-1">
          <p class="text-base font-semibold text-text">{props.step.title}</p>
          <p class="text-xs leading-relaxed text-subtext">{props.step.description}</p>
        </div>
        <div class="flex items-center gap-2 sm:shrink-0">
          <StatusGlyph status={props.step.status} />
          <span class={`text-xs font-semibold tracking-[0.16em] uppercase ${statusClass(props.step.status)}`}>
            {statusLabel(props.step.status)}
          </span>
        </div>
      </div>
      <p class="mt-3 min-h-8 text-xs leading-relaxed text-subtext">{props.step.message}</p>
      <div class="mt-3 h-2 overflow-hidden rounded-full border border-overlay bg-surface/50">
        <div
          class="h-full rounded-full bg-accent/75 transition-[width] duration-300"
          style={{ width: `${Math.max(0, Math.min(props.step.progress, 100))}%` }} />
      </div>
    </article>
  );
}

function GuidancePanel(props: { guidance: string[] }) {
  return (
    <section class="rounded-2xl border border-overlay bg-surface/45 p-4">
      <p class="text-xs font-semibold tracking-[0.2em] text-subtext uppercase">Guidance</p>
      <ul class="mt-2 grid gap-2">
        <For each={props.guidance}>{(line) => <li class="text-sm text-subtext">{line}</li>}</For>
      </ul>
    </section>
  );
}

export function SetupView() {
  const navigate = useNavigate();
  const { runPreflight } = useAppContext();
  const [phase, setPhase] = createSignal<SetupPhase>("checking");
  const [error, setError] = createSignal<string | null>(null);
  const [setupStatus, setSetupStatus] = createSignal<SetupStatus | null>(null);
  const [isRefreshing, setIsRefreshing] = createSignal(false);
  const [steps, setSteps] = createStore<Record<StepKey, SetupStep>>({
    whisper_model: {
      key: "whisper_model",
      title: "Whisper model",
      description: "Download ggml-base.en.bin into appdata/models.",
      status: "pending",
      message: "Waiting for setup check...",
      progress: 0,
    },
    ollama_server: {
      key: "ollama_server",
      title: "Ollama server",
      description: "Reachable at http://localhost:11434.",
      status: "pending",
      message: "Waiting for setup check...",
      progress: 0,
    },
    nomic_embed_text: {
      key: "nomic_embed_text",
      title: "nomic-embed-text",
      description: "Embedding model required for semantic features.",
      status: "pending",
      message: "Waiting for setup check...",
      progress: 0,
    },
    gemma: {
      key: "gemma",
      title: "gemma3:4b",
      description: "Generation model required for title/summary/tags.",
      status: "pending",
      message: "Waiting for setup check...",
      progress: 0,
    },
  });

  const refreshSetup = async (indicateProgress = false) => {
    if (indicateProgress) {
      setIsRefreshing(true);
      if (phase() !== "running") {
        setPhase("checking");
      }
    }

    try {
      const status = await invoke<SetupStatus>("check_setup");
      setSetupStatus(status);
      setSteps(buildStepMap(status));
      setError(null);
      if (phase() !== "running") {
        setPhase("idle");
      }
      return status;
    } catch (refreshError) {
      setError(normalizeError(refreshError));
      setPhase("failed");
      return null;
    } finally {
      if (indicateProgress) {
        setIsRefreshing(false);
      }
    }
  };

  const completeSetupFlow = async () => {
    setPhase("completed");
    const result = await runPreflight();
    if (result?.all_required_passed) {
      await navigate("/library", { replace: true });
      return;
    }
    setError("Setup completed, but preflight still has required failures. Review Splash guidance and retry.");
    setPhase("failed");
  };

  const runSetupWizard = async () => {
    setError(null);
    setPhase("running");
    const initial = await refreshSetup();
    if (!initial) {
      setPhase("failed");
      return;
    }

    try {
      if (!initial.whisper_model_ready) {
        setSteps("whisper_model", "status", "running");
        setSteps("whisper_model", "message", "Downloading ggml-base.en.bin...");
        await invoke("download_whisper_model", { model: "base.en" });
      }

      const afterWhisper = await refreshSetup();
      if (!afterWhisper) {
        setPhase("failed");
        return;
      }

      if (!afterWhisper.ollama_server_ready) {
        setPhase("failed");
        return;
      }

      for (const model of afterWhisper.missing_ollama_models) {
        const stepKey = modelToStep(model);
        if (stepKey) {
          setSteps(stepKey, "status", "running");
          setSteps(stepKey, "message", `Pulling ${model} from Ollama...`);
        }
        await invoke("pull_ollama_model", { model });
      }

      const finalStatus = await refreshSetup();
      if (finalStatus?.all_required_ready) {
        await completeSetupFlow();
        return;
      }

      setPhase("failed");
      setError("Some required dependencies are still missing. Review guidance and retry setup.");
    } catch (setupError) {
      setError(normalizeError(setupError));
      setPhase("failed");
    }
  };

  const setupPhaseLabel = () => {
    if (phase() === "running") {
      return "Installing dependencies...";
    }
    if (isRefreshing()) {
      return "Re-checking setup status...";
    }
    return `Setup phase: ${phase()}`;
  };

  onMount(() => {
    let unlistenWhisper: UnlistenFn | undefined;
    let unlistenOllama: UnlistenFn | undefined;

    void (async () => {
      try {
        unlistenWhisper = await listen<WhisperProgressEvent>(WHISPER_PROGRESS_EVENT, (event) => {
          const status = progressEventStatus(event.payload.status);
          setSteps("whisper_model", "status", status);
          setSteps("whisper_model", "message", event.payload.message);
          setSteps("whisper_model", "progress", event.payload.percent);
        });
        unlistenOllama = await listen<OllamaProgressEvent>(OLLAMA_PROGRESS_EVENT, (event) => {
          const stepKey = modelToStep(event.payload.modelName);
          if (!stepKey) {
            return;
          }
          const status = progressEventStatus(event.payload.status);
          setSteps(stepKey, "status", status);
          setSteps(stepKey, "message", event.payload.message);
          setSteps(stepKey, "progress", event.payload.percent);
        });
      } catch {
        // Events are unavailable in plain browser test contexts.
      }

      const status = await refreshSetup();
      if (status?.all_required_ready) {
        await completeSetupFlow();
      }
    })();

    onCleanup(() => {
      if (unlistenWhisper) {
        void unlistenWhisper();
      }
      if (unlistenOllama) {
        void unlistenOllama();
      }
    });
  });

  return (
    <ViewScaffold
      eyebrow="Setup"
      title="First-run setup wizard"
      description="Install the first-run model dependencies so transcription and document processing are ready.">
      <section class="space-y-4 rounded-3xl border border-overlay bg-elevation/85 p-4 sm:p-6">
        <div class="grid grid-cols-1 gap-3 xl:grid-cols-2">
          <For each={STEP_ORDER}>{(stepKey) => <StepCard step={steps[stepKey]} />}</For>
        </div>

        <Show when={(setupStatus()?.guidance.length ?? 0) > 0}>
          <GuidancePanel guidance={setupStatus()?.guidance ?? []} />
        </Show>

        <Show when={error()}>
          {(message) => (
            <p role="alert" class="rounded-xl border border-accent/50 bg-accent/10 p-3 text-sm text-text">
              {message()}
            </p>
          )}
        </Show>

        <div class="flex flex-col gap-3 border-t border-overlay pt-2 sm:flex-row sm:items-center sm:justify-between">
          <p class="text-xs text-subtext">{setupPhaseLabel()}</p>
          <div class="flex w-full flex-col gap-2 sm:w-auto sm:flex-row">
            <button
              type="button"
              class="inline-flex items-center justify-center gap-2 rounded-xl border border-overlay px-4 py-2 text-sm font-semibold text-subtext transition hover:border-accent/35 hover:text-text disabled:cursor-not-allowed disabled:opacity-60"
              onClick={() => {
                void refreshSetup(true);
              }}
              disabled={phase() === "running" || isRefreshing()}>
              {isRefreshing() ? "Checking setup..." : "Re-check setup"}
            </button>
            <button
              type="button"
              class="rounded-xl bg-accent px-4 py-2 text-sm font-semibold text-surface transition hover:brightness-110 disabled:cursor-not-allowed disabled:opacity-60"
              onClick={() => {
                if (setupStatus()?.all_required_ready) {
                  void completeSetupFlow();
                  return;
                }
                void runSetupWizard();
              }}
              disabled={phase() === "running" || isRefreshing()}>
              {setupStatus()?.all_required_ready ? "Continue to library" : "Start setup"}
            </button>
          </div>
        </div>
      </section>
    </ViewScaffold>
  );
}
