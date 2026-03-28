import { StatusGlyph } from "$/components/StatusGlyph";
import { normalizeError } from "$/errors";
import { type SetupStatus, useAppContext } from "$/state/AppContext";
import type { ProgressStatus } from "$/types";
import {
  EMBEDDING_PROGRESS_EVENT,
  GEMMA_REQUIREMENT,
  OLLAMA_PROGRESS_EVENT,
  STEP_ORDER,
  WHISPER_PROGRESS_EVENT,
} from "$/views/SetupView/lib";
import type {
  EmbeddingProgressEvent,
  OllamaProgressEvent,
  SetupPhase,
  SetupStep,
  StepKey,
  StepStatus,
  WhisperProgressEvent,
} from "$/views/SetupView/lib";
import { useNavigate } from "@solidjs/router";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import * as logger from "@tauri-apps/plugin-log";
import { createSignal, For, onCleanup, onMount, Show } from "solid-js";
import { createStore } from "solid-js/store";
import { ViewScaffold } from "./ViewScaffold";

function modelRequirementMatches(candidate: string, required: string): boolean {
  const left = candidate.trim().toLowerCase();
  const right = required.trim().toLowerCase();

  if (left === right) {
    return true;
  }

  const [leftFamily, leftTag] = left.split(":");
  const [rightFamily, rightTag] = right.split(":");
  if (leftFamily !== rightFamily) {
    return false;
  }

  return !leftTag || !rightTag || leftTag === rightTag;
}

function hasModel(status: SetupStatus, model: string): boolean {
  return !status.missing_ollama_models.some((missing) => modelRequirementMatches(missing, model));
}

function modelToStep(modelName: string): StepKey | null {
  if (modelName.startsWith("gemma3")) {
    return "metadata_backend";
  }
  return null;
}

function progressEventStatus(status: ProgressStatus): StepStatus {
  if (status === "error") {
    return "fail";
  }
  if (status === "completed") {
    return "pass";
  }
  return "running";
}

function getMetadataStatus(status: SetupStatus): StepStatus {
  if (status.resolved_metadata_backend === "apple_intelligence") {
    return "pass";
  }

  if (status.ollama_reachable) {
    return hasModel(status, GEMMA_REQUIREMENT) ? "pass" : "pending";
  }

  const isMacFlow = status.metadata_backend_mode !== "ollama" || status.apple_intelligence_available
    || status.apple_intelligence_reason !== null;
  return isMacFlow ? "blocked" : "fail";
}

function getMetadataMessage(status: SetupStatus): string {
  if (status.resolved_metadata_backend === "apple_intelligence") {
    return "Apple Intelligence is available for metadata generation.";
  }

  if (status.ollama_reachable && hasModel(status, GEMMA_REQUIREMENT)) {
    return "Ollama metadata model is installed.";
  } else if (status.ollama_reachable) {
    return "Ollama is reachable and the metadata model will be pulled.";
  }

  const isMacFlow = status.metadata_backend_mode !== "ollama" || status.apple_intelligence_available
    || status.apple_intelligence_reason !== null;

  if (isMacFlow) {
    if (status.apple_intelligence_available) {
      return "Apple Intelligence is available. Ollama remains optional as a fallback.";
    }
    if (status.apple_intelligence_reason) {
      return `Apple Intelligence is unavailable: ${status.apple_intelligence_reason}.`;
    }
    return "Apple Intelligence is unavailable.";
  }

  return "Ollama is not reachable yet.";
}

function buildStepMap(status: SetupStatus): Record<StepKey, SetupStep> {
  const isMacFlow = status.metadata_backend_mode !== "ollama" || status.apple_intelligence_available
    || status.apple_intelligence_reason !== null;
  const ollamaServerStatus: StepStatus = status.ollama_reachable ? "pass" : (isMacFlow ? "blocked" : "fail");
  const metadataStatus: StepStatus = getMetadataStatus(status);
  const metadataMessage = getMetadataMessage(status);

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
    embedding_model: {
      key: "embedding_model",
      title: "Local embedding model",
      description: "Download NomicEmbedTextV15 into appdata/models/embed.",
      status: status.embedding_model_ready ? "pass" : "pending",
      message: status.embedding_model_ready
        ? "Local embedding model is available."
        : "Model not found. Setup will download it.",
      progress: status.embedding_model_ready ? 100 : 0,
    },
    metadata_backend: {
      key: "metadata_backend",
      title: isMacFlow ? "Metadata backend" : "Ollama metadata model",
      description: isMacFlow
        ? "Use Apple Intelligence when available and Ollama as the fallback backend."
        : "Use the gemma3 family through Ollama for title, summary, and tag generation.",
      status: metadataStatus,
      message: metadataMessage,
      progress: metadataStatus === "pass" ? 100 : 0,
    },
    ollama_fallback: {
      key: "ollama_fallback",
      title: isMacFlow ? "Ollama fallback" : "Ollama server",
      description: isMacFlow
        ? "Optional on Mac when Apple Intelligence is available; used as the fallback backend otherwise."
        : "Required for metadata generation at http://localhost:11434.",
      status: ollamaServerStatus,
      message: status.ollama_reachable
        ? "Ollama server is reachable."
        : (isMacFlow
          ? "Ollama fallback is optional until Apple Intelligence is unavailable or you switch backends."
          : "Ollama is not reachable. Search still works, but metadata generation requires Ollama."),
      progress: status.ollama_reachable ? 100 : 0,
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
  const { runPreflight, state, completeStartupFlow } = useAppContext();
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
    embedding_model: {
      key: "embedding_model",
      title: "Local embedding model",
      description: "Download NomicEmbedTextV15 into appdata/models/embed.",
      status: "pending",
      message: "Waiting for setup check...",
      progress: 0,
    },
    metadata_backend: {
      key: "metadata_backend",
      title: "Metadata backend",
      description: "Preparing the preferred metadata backend.",
      status: "pending",
      message: "Waiting for setup check...",
      progress: 0,
    },
    ollama_fallback: {
      key: "ollama_fallback",
      title: "Ollama fallback",
      description: "Preparing Ollama fallback availability.",
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
      completeStartupFlow();
      await navigate("/library");
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

      if (!afterWhisper.embedding_model_ready) {
        setSteps("embedding_model", "status", "running");
        setSteps("embedding_model", "message", "Downloading local embedding model...");
        await invoke("download_embedding_model");
      }

      const afterEmbedding = await refreshSetup();
      if (!afterEmbedding) {
        setPhase("failed");
        return;
      }

      if (afterEmbedding.resolved_metadata_backend !== "apple_intelligence" && afterEmbedding.ollama_reachable) {
        for (const model of afterEmbedding.missing_ollama_models) {
          const stepKey = modelToStep(model);
          if (stepKey) {
            setSteps(stepKey, "status", "running");
            setSteps(stepKey, "message", `Pulling ${model} from Ollama...`);
          }
          await invoke("pull_ollama_model", { model });
        }
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
    let unlistenEmbedding: UnlistenFn | undefined;
    let unlistenOllama: UnlistenFn | undefined;

    void (async () => {
      try {
        unlistenWhisper = await listen<WhisperProgressEvent>(WHISPER_PROGRESS_EVENT, (event) => {
          const status = progressEventStatus(event.payload.status);
          setSteps("whisper_model", "status", status);
          setSteps("whisper_model", "message", event.payload.message);
          setSteps("whisper_model", "progress", event.payload.percent);
        });
        unlistenEmbedding = await listen<EmbeddingProgressEvent>(EMBEDDING_PROGRESS_EVENT, (event) => {
          const status = progressEventStatus(event.payload.status);
          setSteps("embedding_model", "status", status);
          setSteps("embedding_model", "message", event.payload.message);
          setSteps("embedding_model", "progress", event.payload.percent);
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
      } catch (error) {
        logger.warn("Events are unavailable in plain browser contexts.");
        logger.error("error, preflight check failure", { keyValues: { error: normalizeError(error) } });
      }

      const status = await refreshSetup();
      if (status?.all_required_ready && state.startupFlowActive) {
        await completeSetupFlow();
      }
    })();

    onCleanup(() => {
      if (unlistenWhisper) {
        void unlistenWhisper();
      }
      if (unlistenEmbedding) {
        void unlistenEmbedding();
      }
      if (unlistenOllama) {
        void unlistenOllama();
      }
    });
  });

  return (
    <ViewScaffold
      eyebrow="Setup"
      title="Getting things ready"
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
