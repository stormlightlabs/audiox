import { invoke } from "@tauri-apps/api/core";
import { createContext, onMount, type ParentProps, useContext } from "solid-js";
import { createStore } from "solid-js/store";

export type BootPhase = "idle" | "loading" | "ready" | "error";

export interface AppBootstrapResult {
  appDataDir: string;
  databasePath: string;
  createdDirectories: string[];
  schemaVersion: number;
}

interface AppStore {
  bootPhase: BootPhase;
  bootError: string | null;
  bootstrap: AppBootstrapResult | null;
}

interface AppContextValue {
  state: AppStore;
  initialize: () => Promise<void>;
}

const AppContext = createContext<AppContextValue>();

function normalizeError(error: unknown): string {
  if (error instanceof Error) {
    return error.message;
  }
  return String(error);
}

export function AppProvider(props: ParentProps) {
  const [state, setState] = createStore<AppStore>({ bootPhase: "idle", bootError: null, bootstrap: null });

  const initialize = async () => {
    setState({ bootPhase: "loading", bootError: null });

    try {
      const bootstrap = await invoke<AppBootstrapResult>("initialize_app");
      setState({ bootstrap, bootPhase: "ready", bootError: null });
    } catch (error) {
      setState({ bootPhase: "error", bootError: normalizeError(error) });
    }
  };

  onMount(() => {
    void initialize();
  });

  return <AppContext.Provider value={{ state, initialize }}>{props.children}</AppContext.Provider>;
}

export function useAppContext() {
  const context = useContext(AppContext);
  if (!context) {
    throw new Error("useAppContext must be used within an AppProvider");
  }
  return context;
}
