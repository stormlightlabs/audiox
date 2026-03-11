import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { createContext, onCleanup, onMount, type ParentProps, useContext } from "solid-js";
import { createStore } from "solid-js/store";

export const PREFLIGHT_EVENT = "preflight://check";
export const PREFLIGHT_CHECK_ORDER = [
  "whisper_cli",
  "ffmpeg",
  "yt_dlp",
  "whisper_model",
  "ollama_server",
  "ollama_models",
  "database",
] as const;

export type PreflightCheck = (typeof PREFLIGHT_CHECK_ORDER)[number];
export type CheckStatus = "pass" | "fail" | "warn";
export type PreflightPhase = "idle" | "running" | "ready" | "failed";
export type CheckDisplayStatus = CheckStatus | "pending";

export type PreflightCheckDetail = { check: PreflightCheck; status: CheckStatus; message: string };

export type PreflightResult = {
  whisper_cli: CheckStatus;
  ffmpeg: CheckStatus;
  yt_dlp: CheckStatus;
  whisper_model: CheckStatus;
  ollama_server: CheckStatus;
  ollama_models: CheckStatus;
  database: CheckStatus;
  should_open_setup: boolean;
  all_required_passed: boolean;
  details: PreflightCheckDetail[];
};

export type SetupStatus = {
  whisper_model_ready: boolean;
  ollama_server_ready: boolean;
  missing_ollama_models: string[];
  setup_completed: boolean;
  all_required_ready: boolean;
  guidance: string[];
};

export type CheckUiState = { status: CheckDisplayStatus; message: string };

export type ChecklistState = Record<PreflightCheck, CheckUiState>;

type AppStore = {
  preflightPhase: PreflightPhase;
  preflightError: string | null;
  preflightResult: PreflightResult | null;
  checklist: ChecklistState;
  completedChecks: number;
};

type AppContextValue = { state: AppStore; runPreflight: () => Promise<PreflightResult | null> };

const AppContext = createContext<AppContextValue>();

function normalizeError(error: unknown): string {
  if (error instanceof Error) {
    return error.message;
  }
  return String(error);
}

function createInitialChecklist(): ChecklistState {
  const checklist = {} as ChecklistState;
  for (const check of PREFLIGHT_CHECK_ORDER) {
    checklist[check] = { status: "pending", message: "" };
  }
  return checklist;
}

function countCompletedChecks(checklist: ChecklistState): number {
  return Object.values(checklist).filter((item) => item.status !== "pending").length;
}

function mergePreflightDetails(checklist: ChecklistState, details: PreflightCheckDetail[]): ChecklistState {
  const merged = { ...checklist };
  for (const detail of details) {
    merged[detail.check] = { status: detail.status, message: detail.message };
  }
  return merged;
}

export function AppProvider(props: ParentProps) {
  const [state, setState] = createStore<AppStore>({
    preflightPhase: "idle",
    preflightError: null,
    preflightResult: null,
    checklist: createInitialChecklist(),
    completedChecks: 0,
  });

  const runPreflight = async () => {
    setState({
      preflightPhase: "running",
      preflightError: null,
      preflightResult: null,
      checklist: createInitialChecklist(),
      completedChecks: 0,
    });

    try {
      const result = await invoke<PreflightResult>("preflight");
      const checklist = mergePreflightDetails(state.checklist, result.details);
      setState({
        preflightResult: result,
        preflightPhase: result.all_required_passed ? "ready" : "failed",
        preflightError: null,
        checklist,
        completedChecks: countCompletedChecks(checklist),
      });
      return result;
    } catch (error) {
      setState({ preflightPhase: "failed", preflightError: normalizeError(error) });
      return null;
    }
  };

  onMount(() => {
    let unlisten: UnlistenFn | undefined;
    let disposed = false;

    void (async () => {
      try {
        unlisten = await listen<PreflightCheckDetail>(PREFLIGHT_EVENT, (event) => {
          const detail = event.payload;
          setState("checklist", detail.check, { status: detail.status, message: detail.message });
          setState("completedChecks", countCompletedChecks(state.checklist));
        });
        if (disposed && unlisten) {
          await unlisten();
          unlisten = undefined;
        }
      } catch {
        // Event channel may be unavailable in plain browser contexts.
      }

      if (!disposed) {
        await runPreflight();
      }
    })();

    onCleanup(() => {
      disposed = true;
      if (unlisten) {
        void unlisten();
      }
    });
  });

  return <AppContext.Provider value={{ state, runPreflight }}>{props.children}</AppContext.Provider>;
}

export function useAppContext() {
  const context = useContext(AppContext);
  if (!context) {
    throw new Error("useAppContext must be used within an AppProvider");
  }
  return context;
}
