import {
  type AudioInputDevice,
  checkMicrophonePermission,
  getPreferredAudioInputDeviceId,
  listAudioInputDevices,
  requestMicrophonePermission,
  setPreferredAudioInputDeviceId,
  supportsMediaRecording,
} from "$/devices";
import { normalizeError } from "$/errors";
import { invoke } from "@tauri-apps/api/core";
import { createMemo, For, onMount, Show } from "solid-js";
import { createStore } from "solid-js/store";
import type { PreflightResult } from "../state/AppContext";
import { DeviceSelector } from "./SettingsView/DeviceSelector";
import { WhisperModelInventory } from "./SettingsView/lib";
import { RecordingDeviceActions } from "./SettingsView/RecordingDeviceActions";
import {
  InstalledWhisperModels,
  WhisperLanguageSelector,
  WhisperModelSelector,
  WhisperThreadsInput,
} from "./SettingsView/WhisperControls";
import { ViewScaffold } from "./ViewScaffold";

type MetadataBackendMode = "auto" | "apple_intelligence" | "ollama";

type ResolvedMetadataBackend = "apple_intelligence" | "ollama" | "unavailable";

type AppSettings = {
  whisperModel: string;
  whisperLanguage: string;
  whisperThreads: number;
  metadataBackendMode: MetadataBackendMode;
  ollamaEndpoint: string;
};

type SettingsStore = {
  isBusy: boolean;
  isSavingSettings: boolean;
  isCheckingMetadataBackend: boolean;
  isRunningPreflight: boolean;
  isDownloadingModel: boolean;
  deletingModelName: string | null;
  settings: AppSettings;
  modelInventory: WhisperModelInventory | null;
  metadataBackendStatus: MetadataBackendStatus | null;
  preflightResult: PreflightResult | null;
  info: string | null;
  error: string | null;
  selectedDeviceId: string;
  devices: AudioInputDevice[];
  settingsError: string | null;
  settingsInfo: string | null;
  isLoading: boolean;
  hasPermission: boolean;
};

type MetadataBackendStatus = {
  mode: MetadataBackendMode;
  resolvedBackend: ResolvedMetadataBackend;
  appleIntelligenceAvailable: boolean;
  appleIntelligenceReason: string | null;
  ollamaEndpoint: string;
  ollamaReachable: boolean;
  installedOllamaModels: string[];
  missingOllamaModels: string[];
  message: string;
};

const METADATA_BACKEND_OPTIONS: Array<{ value: MetadataBackendMode; label: string }> = [
  { value: "auto", label: "Auto" },
  { value: "apple_intelligence", label: "Apple Intelligence" },
  { value: "ollama", label: "Ollama" },
];

function SettingsActions(
  props: {
    isSavingSettings: boolean;
    isBusy: boolean;
    isDownloadingModel: boolean;
    selectedModelInstalled: boolean;
    deletingModelName: string | null;
    saveSettings: () => void;
    downloadSelectedModel: () => void;
    deleteModel: (modelName: string) => void;
    whisperModel: string;
  },
) {
  const isSavingSettings = () => props.isSavingSettings;
  const isBusy = () => props.isBusy;
  const isDownloadingModel = () => props.isDownloadingModel;
  const selectedModelInstalled = () => props.selectedModelInstalled;
  const deletingModelName = () => props.deletingModelName;
  const whisperModel = () => props.whisperModel;
  return (
    <div class="sm:col-span-2 flex flex-wrap gap-2">
      <button
        type="button"
        class="rounded-xl bg-accent px-4 py-2 text-sm font-semibold text-surface transition hover:brightness-110 disabled:cursor-not-allowed disabled:opacity-60"
        disabled={isSavingSettings() || isBusy()}
        onClick={() => {
          void props.saveSettings();
        }}>
        {isSavingSettings() ? "Saving..." : "Save settings"}
      </button>
      <button
        type="button"
        class="rounded-xl border border-overlay px-4 py-2 text-sm font-semibold text-subtext transition hover:border-accent/35 hover:text-text disabled:opacity-60"
        disabled={isDownloadingModel() || isBusy()}
        onClick={() => {
          void props.downloadSelectedModel();
        }}>
        {isDownloadingModel() ? "Downloading..." : "Download selected model"}
      </button>
      <Show when={selectedModelInstalled()}>
        <button
          type="button"
          class="rounded-xl border border-overlay px-4 py-2 text-sm font-semibold text-subtext transition hover:border-accent/35 hover:text-text disabled:opacity-60"
          disabled={Boolean(deletingModelName())}
          onClick={() => {
            void props.deleteModel(whisperModel());
          }}>
          {deletingModelName() === whisperModel() ? "Deleting..." : "Delete selected model"}
        </button>
      </Show>
    </div>
  );
}

function MetadataBackendSelector(
  props: {
    metadataBackendMode: MetadataBackendMode;
    options: Array<{ value: MetadataBackendMode; label: string }>;
    onMetadataBackendModeChange: (metadataBackendMode: MetadataBackendMode) => void;
  },
) {
  return (
    <label class="grid gap-2">
      <span class="text-xs font-semibold tracking-[0.14em] text-subtext uppercase">Metadata backend</span>
      <select
        class="rounded-xl border border-overlay bg-surface/45 px-3 py-2 text-sm text-text focus:border-accent/60 focus:outline-hidden"
        value={props.metadataBackendMode}
        onInput={(event) => {
          const value = (event.currentTarget as HTMLSelectElement).value as MetadataBackendMode;
          props.onMetadataBackendModeChange(value);
        }}>
        <For each={props.options}>{(option) => <option value={option.value}>{option.label}</option>}</For>
      </select>
    </label>
  );
}

function OllamaEndpointInput(
  props: { ollamaEndpoint: string; onOllamaEndpointChange: (ollamaEndpoint: string) => void; useOllama: boolean },
) {
  const useOllama = () => props.useOllama;
  const ollamaEndpoint = () => props.ollamaEndpoint;
  return (
    <Show when={useOllama()}>
      <label class="grid gap-2">
        <span class="text-xs font-semibold tracking-[0.14em] text-subtext uppercase">Ollama endpoint</span>
        <input
          type="text"
          class="rounded-xl border border-overlay bg-surface/45 px-3 py-2 text-sm text-text focus:border-accent/60 focus:outline-hidden"
          value={ollamaEndpoint()}
          onInput={(event) => {
            const value = (event.currentTarget as HTMLInputElement).value;
            props.onOllamaEndpointChange(value);
          }} />
      </label>
    </Show>
  );
}

function CheckMetadataBackendButton(props: { isCheckingMetadataBackend: boolean; checkMetadataBackend: () => void }) {
  return (
    <div class="flex flex-wrap items-center gap-2">
      <button
        type="button"
        class="rounded-xl border border-overlay px-3 py-1.5 text-xs font-semibold text-subtext transition hover:border-accent/35 hover:text-text disabled:opacity-60"
        disabled={props.isCheckingMetadataBackend}
        onClick={() => {
          void props.checkMetadataBackend();
        }}>
        {props.isCheckingMetadataBackend ? "Checking metadata backends..." : "Re-check metadata backend"}
      </button>
    </div>
  );
}

export function SettingsView() {
  const isSupported = supportsMediaRecording();
  const isMacOS = /mac/i.test(globalThis.navigator?.platform ?? "");
  const [state, setState] = createStore<SettingsStore>({
    isBusy: false,
    isSavingSettings: false,
    isCheckingMetadataBackend: false,
    isRunningPreflight: false,
    isDownloadingModel: false,
    deletingModelName: null,
    settings: {
      whisperModel: "base.en",
      whisperLanguage: "auto",
      whisperThreads: 4,
      metadataBackendMode: isMacOS ? "auto" : "ollama",
      ollamaEndpoint: "http://localhost:11434",
    },
    modelInventory: null,
    metadataBackendStatus: null,
    preflightResult: null,
    settingsError: null,
    settingsInfo: null,
    isLoading: false,
    devices: [] as AudioInputDevice[],
    selectedDeviceId: getPreferredAudioInputDeviceId() ?? "",
    hasPermission: false,
    error: null,
    info: null,
  });

  const selectedModelInstalled = createMemo(() =>
    state.modelInventory?.installedModels.some((model) => model.modelName === state.settings.whisperModel) ?? false
  );
  const metadataBackendOptions = createMemo(() =>
    isMacOS ? METADATA_BACKEND_OPTIONS : METADATA_BACKEND_OPTIONS.filter((option) => option.value === "ollama")
  );
  const metadataBackendMayUseOllama = createMemo(() =>
    !isMacOS || state.settings.metadataBackendMode === "auto" || state.settings.metadataBackendMode === "ollama"
  );
  const preflightSummary = createMemo(() => {
    const result = state.preflightResult;
    if (!result) {
      return null;
    }

    const counts = { pass: 0, warn: 0, fail: 0 };
    for (const detail of result.details) {
      if (detail.status === "pass" || detail.status === "warn" || detail.status === "fail") {
        counts[detail.status] += 1;
      }
    }
    return counts;
  });

  const refreshDevices = async () => {
    if (!isSupported) {
      return;
    }

    setState("isLoading", true);
    setState("error", null);
    try {
      const permission = await checkMicrophonePermission();
      setState("hasPermission", permission.granted);
      if (!permission.granted) {
        setState("devices", []);
        return;
      }

      const inputs = await listAudioInputDevices();
      setState("devices", inputs);
      if (state.selectedDeviceId.length > 0 && !inputs.some((device) => device.id === state.selectedDeviceId)) {
        setState("selectedDeviceId", "");
        setPreferredAudioInputDeviceId(null);
      }
    } catch (refreshError) {
      setState("error", normalizeError(refreshError));
    } finally {
      setState("isLoading", false);
    }
  };

  const requestPermission = async () => {
    setState("info", null);
    setState("error", null);
    try {
      const permission = await requestMicrophonePermission();
      setState("hasPermission", permission.granted);
      if (!permission.granted) {
        setState(
          "error",
          permission.canRequest
            ? "Microphone permission was not granted."
            : "Microphone permission is denied at the system level.",
        );
        return;
      }
      setState("info", "Microphone permission granted.");
      await refreshDevices();
    } catch (permissionError) {
      setState("error", normalizeError(permissionError));
    }
  };

  const refreshSettings = async () => {
    if (!isSupported) {
      return;
    }

    setState("isBusy", true);
    setState("settingsError", null);
    try {
      const nextSettings = await invoke<AppSettings>("get_app_settings");
      setState("settings", nextSettings);
      const inventory = await invoke<WhisperModelInventory>("list_whisper_models");
      setState("modelInventory", inventory);
      const backendStatus = await invoke<MetadataBackendStatus>("get_metadata_backend_status");
      setState("metadataBackendStatus", backendStatus);
    } catch (refreshError) {
      setState("settingsError", normalizeError(refreshError));
    } finally {
      setState("isBusy", false);
    }
  };

  const saveSettings = async () => {
    setState("isSavingSettings", true);
    setState("settingsError", null);
    setState("settingsInfo", null);
    try {
      const saved = await invoke<AppSettings>("save_app_settings", {
        whisperModel: state.settings.whisperModel,
        whisperLanguage: state.settings.whisperLanguage,
        whisperThreads: state.settings.whisperThreads,
        metadataBackendMode: state.settings.metadataBackendMode,
        ollamaEndpoint: state.settings.ollamaEndpoint,
      });
      setState("settings", saved);
      setState("settingsInfo", "Saved settings.");
      const inventory = await invoke<WhisperModelInventory>("list_whisper_models");
      setState("modelInventory", inventory);
      const backendStatus = await invoke<MetadataBackendStatus>("get_metadata_backend_status");
      setState("metadataBackendStatus", backendStatus);
    } catch (saveError) {
      setState("settingsError", normalizeError(saveError));
    } finally {
      setState("isSavingSettings", false);
    }
  };

  const downloadSelectedModel = async () => {
    setState("isDownloadingModel", true);
    setState("settingsError", null);
    setState("settingsInfo", null);
    try {
      await invoke("download_whisper_model", { model: state.settings.whisperModel });
      const inventory = await invoke<WhisperModelInventory>("list_whisper_models");
      setState("modelInventory", inventory);
      setState("settingsInfo", `Downloaded ${state.settings.whisperModel}.`);
    } catch (downloadError) {
      setState("settingsError", normalizeError(downloadError));
    } finally {
      setState("isDownloadingModel", false);
    }
  };

  const deleteModel = async (modelName: string) => {
    setState("deletingModelName", modelName);
    setState("settingsError", null);
    setState("settingsInfo", null);
    try {
      const inventory = await invoke<WhisperModelInventory>("delete_whisper_model", { model: modelName });
      setState("modelInventory", inventory);
      setState("settings", "whisperModel", inventory.selectedModel);
      setState("settingsInfo", `Deleted ${modelName}.`);
    } catch (deleteError) {
      setState("settingsError", normalizeError(deleteError));
    } finally {
      setState("deletingModelName", null);
    }
  };

  const checkMetadataBackend = async () => {
    setState("isCheckingMetadataBackend", true);
    setState("settingsError", null);
    try {
      const status = await invoke<MetadataBackendStatus>("get_metadata_backend_status");
      setState("metadataBackendStatus", status);
    } catch (statusError) {
      setState("settingsError", normalizeError(statusError));
      setState("metadataBackendStatus", null);
    } finally {
      setState("isCheckingMetadataBackend", false);
    }
  };

  const rerunPreflight = async () => {
    setState("isRunningPreflight", true);
    setState("settingsError", null);
    try {
      const result = await invoke<PreflightResult>("preflight");
      setState("preflightResult", result);
    } catch (preflightError) {
      setState("settingsError", normalizeError(preflightError));
      setState("preflightResult", null);
    } finally {
      setState("isRunningPreflight", false);
    }
  };

  const onDeviceChange = (nextDeviceId: string) => {
    setState("selectedDeviceId", nextDeviceId);
    setPreferredAudioInputDeviceId(nextDeviceId || null);
    setState("info", nextDeviceId ? "Saved preferred microphone." : "Using the system default microphone.");
  };

  const selectedDeviceName = () => {
    const selected = state.selectedDeviceId;
    if (!selected) {
      return "System default input";
    }
    const device = state.devices.find((item) => item.id === selected);
    return device?.name || "Saved input device";
  };

  const onWhisperModelChange = (whisperModel: string) => {
    setState("settings", "whisperModel", whisperModel);
  };

  const onWhisperLanguageChange = (whisperLanguage: string) => {
    setState("settings", "whisperLanguage", whisperLanguage);
  };

  const onWhisperThreadsChange = (whisperThreads: number) => {
    setState("settings", "whisperThreads", whisperThreads);
  };

  const onMetadataBackendModeChange = (metadataBackendMode: MetadataBackendMode) => {
    setState("settings", "metadataBackendMode", metadataBackendMode);
  };

  const onOllamaEndpointChange = (ollamaEndpoint: string) => {
    setState("settings", "ollamaEndpoint", ollamaEndpoint);
  };

  onMount(() => {
    void refreshDevices();
    void refreshSettings();
  });

  return (
    <Show
      when={isSupported}
      fallback={
        <ViewScaffold
          eyebrow="Settings"
          title="System configuration"
          description="Configure microphone defaults for in-app recording. Choose which input device Murmur should use before opening the Record view.">
          <section class="space-y-4 rounded-3xl border border-overlay bg-elevation/85 p-6">
            <p role="alert" class="rounded-xl border border-accent/50 bg-accent/10 p-3 text-sm text-text">
              This view requires the native Tauri runtime.
            </p>
          </section>
        </ViewScaffold>
      }>
      <ViewScaffold
        eyebrow="Settings"
        title="System configuration"
        description="Manage local transcription, metadata backends, model downloads, and recording device preferences.">
        <section class="space-y-5 rounded-3xl border border-overlay bg-elevation/85 p-6">
          <div class="grid gap-3 rounded-2xl border border-overlay bg-surface/35 p-4 sm:grid-cols-2">
            <WhisperModelSelector
              whisperModel={state.settings.whisperModel}
              onWhisperModelChange={onWhisperModelChange} />

            <WhisperLanguageSelector
              whisperLanguage={state.settings.whisperLanguage}
              onWhisperLanguageChange={onWhisperLanguageChange} />

            <WhisperThreadsInput
              whisperThreads={state.settings.whisperThreads}
              onWhisperThreadsChange={onWhisperThreadsChange} />

            <MetadataBackendSelector
              metadataBackendMode={state.settings.metadataBackendMode}
              options={metadataBackendOptions()}
              onMetadataBackendModeChange={onMetadataBackendModeChange} />

            <OllamaEndpointInput
              useOllama={metadataBackendMayUseOllama()}
              ollamaEndpoint={state.settings.ollamaEndpoint}
              onOllamaEndpointChange={onOllamaEndpointChange} />

            <SettingsActions
              isSavingSettings={state.isSavingSettings}
              isBusy={state.isBusy}
              isDownloadingModel={state.isDownloadingModel}
              selectedModelInstalled={selectedModelInstalled()}
              deletingModelName={state.deletingModelName}
              saveSettings={saveSettings}
              downloadSelectedModel={downloadSelectedModel}
              deleteModel={deleteModel}
              whisperModel={state.settings.whisperModel} />
          </div>

          <InstalledWhisperModels
            modelInventory={state.modelInventory}
            selectedModelName={state.settings.whisperModel}
            deletingModelName={state.deletingModelName}
            deleteModel={deleteModel}
            selectedModelInstalled={selectedModelInstalled()} />

          <section class="space-y-3 rounded-2xl border border-overlay bg-surface/30 p-4">
            <CheckMetadataBackendButton
              isCheckingMetadataBackend={state.isCheckingMetadataBackend}
              checkMetadataBackend={checkMetadataBackend} />
            <Show when={state.metadataBackendStatus}>
              {(status) => (
                <div class="rounded-xl border border-overlay bg-surface/35 p-3 text-xs text-subtext">
                  <p class="text-sm font-semibold text-text">Resolved backend: {status().resolvedBackend}</p>
                  <p class="mt-1">Selected mode: {status().mode}</p>
                  <p class="mt-1">{status().message}</p>
                  <Show when={isMacOS}>
                    <p class="mt-1">
                      Apple Intelligence: {status().appleIntelligenceAvailable ? "available" : "unavailable"}
                      {status().appleIntelligenceReason ? ` (${status().appleIntelligenceReason})` : ""}
                    </p>
                  </Show>
                  <Show when={metadataBackendMayUseOllama()}>
                    <p class="mt-1">
                      Ollama: {status().ollamaReachable ? "reachable" : "unreachable"} at {status().ollamaEndpoint}
                    </p>
                  </Show>
                  <Show when={status().installedOllamaModels.length > 0}>
                    <p class="mt-1">Installed Ollama models: {status().installedOllamaModels.join(", ")}</p>
                  </Show>
                  <Show when={status().missingOllamaModels.length > 0}>
                    <p class="mt-1">Missing Ollama models: {status().missingOllamaModels.join(", ")}</p>
                  </Show>
                </div>
              )}
            </Show>
          </section>

          <section class="space-y-3 rounded-2xl border border-overlay bg-surface/30 p-4">
            <button
              type="button"
              class="rounded-xl border border-overlay px-3 py-1.5 text-xs font-semibold text-subtext transition hover:border-accent/35 hover:text-text disabled:opacity-60"
              disabled={state.isRunningPreflight}
              onClick={() => {
                void rerunPreflight();
              }}>
              {state.isRunningPreflight ? "Running preflight..." : "Re-run preflight checks"}
            </button>
            <Show when={preflightSummary()}>
              {(summary) => (
                <p class="rounded-xl border border-overlay bg-surface/35 px-3 py-2 text-xs text-subtext">
                  Preflight results: {summary().pass} pass, {summary().warn} warn, {summary().fail} fail.
                </p>
              )}
            </Show>
          </section>

          <RecordingDeviceActions
            isLoading={state.isLoading}
            hasPermission={state.hasPermission}
            requestPermission={requestPermission}
            refreshDevices={refreshDevices} />

          <DeviceSelector
            devices={state.devices}
            selectedDeviceId={state.selectedDeviceId}
            onDeviceChange={onDeviceChange} />

          <p class="rounded-xl border border-overlay bg-surface/35 px-3 py-2 text-xs text-subtext">
            Preferred input: {selectedDeviceName()}.
          </p>

          <Show when={state.devices.length === 0 && state.hasPermission && !state.isLoading}>
            <p class="rounded-xl border border-overlay bg-surface/35 px-3 py-2 text-xs text-subtext">
              No audio inputs were detected. Connect a microphone and refresh the device list.
            </p>
          </Show>
          <Show when={state.settingsInfo}>
            {(info) => <p class="rounded-xl border border-overlay bg-surface/35 p-3 text-sm text-subtext">{info()}</p>}
          </Show>
          <Show when={state.settingsError}>
            {(err) => (
              <p role="alert" class="rounded-xl border border-accent/50 bg-accent/10 p-3 text-sm text-text">{err()}</p>
            )}
          </Show>
          <Show when={state.info}>
            {(info) => <p class="rounded-xl border border-overlay bg-surface/35 p-3 text-sm text-subtext">{info()}</p>}
          </Show>
          <Show when={state.error}>
            {(err) => (
              <p role="alert" class="rounded-xl border border-accent/50 bg-accent/10 p-3 text-sm text-text">{err()}</p>
            )}
          </Show>
        </section>
      </ViewScaffold>
    </Show>
  );
}
