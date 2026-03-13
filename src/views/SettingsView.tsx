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
import { formatBytes } from "$/format-utils";
import { invoke } from "@tauri-apps/api/core";
import { createMemo, For, onMount, Show } from "solid-js";
import { createStore } from "solid-js/store";
import type { PreflightResult } from "../state/AppContext";
import { ViewScaffold } from "./ViewScaffold";

type AppSettings = { whisperModel: string; whisperLanguage: string; whisperThreads: number; ollamaEndpoint: string };
type WhisperModelInfo = { modelName: string; fileName: string; sizeBytes: number };
type WhisperModelInventory = { selectedModel: string; installedModels: WhisperModelInfo[]; totalSizeBytes: number };

type SettingsStore = {
  isBusy: boolean;
  isSavingSettings: boolean;
  isCheckingOllama: boolean;
  isRunningPreflight: boolean;
  isDownloadingModel: boolean;
  deletingModelName: string | null;
  settings: AppSettings;
  modelInventory: WhisperModelInventory | null;
  ollamaStatus: OllamaConnectionStatus | null;
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

type OllamaConnectionStatus = {
  endpoint: string;
  reachable: boolean;
  installedModels: string[];
  missingModels: string[];
  message: string;
};

type DeviceSelectorProps = {
  devices: AudioInputDevice[];
  selectedDeviceId: string;
  onDeviceChange: (deviceId: string) => void;
};

const WHISPER_MODEL_OPTIONS = [
  { value: "tiny", label: "tiny" },
  { value: "base", label: "base" },
  { value: "small", label: "small" },
  { value: "medium", label: "medium" },
  { value: "large", label: "large" },
  { value: "base.en", label: "base.en" },
];
const WHISPER_LANGUAGE_OPTIONS = ["auto", "en", "es", "fr", "de", "it", "pt", "ja", "zh"];

function DeviceSelector(props: DeviceSelectorProps) {
  return (
    <label class="grid gap-2">
      <span class="text-xs font-semibold tracking-[0.14em] text-subtext uppercase">Preferred microphone</span>
      <select
        class="rounded-xl border border-overlay bg-surface/45 px-3 py-2 text-sm text-text focus:border-accent/60 focus:outline-hidden"
        value={props.selectedDeviceId}
        onInput={(event) => {
          const target = event.currentTarget as HTMLSelectElement;
          props.onDeviceChange(target.value);
        }}>
        <option value="">System default input</option>
        <For each={props.devices}>
          {(device) => <option value={device.id}>{device.name || "Unnamed input"}</option>}
        </For>
      </select>
    </label>
  );
}

function Refresher() {
  return (
    <span class="flex items-center gap-1">
      <i class="i-ri-loader-line animate-spin" />
      <span>Refreshing...</span>
    </span>
  );
}

function RecordingDeviceActions(
  props: { isLoading: boolean; hasPermission: boolean; requestPermission: () => void; refreshDevices: () => void },
) {
  const hasPermission = () => props.hasPermission;
  const isLoading = () => props.isLoading;
  return (
    <div class="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
      <div>
        <p class="text-sm font-semibold text-text">Audio input device</p>
        <p class="text-xs text-subtext">Select a preferred microphone or keep using the system default.</p>
      </div>
      <div class="flex flex-wrap items-center gap-2">
        <button
          type="button"
          class="rounded-xl border border-overlay px-3 py-1.5 text-xs font-semibold text-subtext transition hover:border-accent/35 hover:text-text disabled:opacity-60"
          onClick={() => {
            void props.requestPermission();
          }}
          disabled={isLoading() || hasPermission()}>
          <Show when={hasPermission()} fallback={"Enable mic access"}>Permission granted</Show>
        </button>
        <button
          type="button"
          class="rounded-xl border border-overlay px-3 py-1.5 text-xs font-semibold text-subtext transition hover:border-accent/35 hover:text-text disabled:opacity-60"
          onClick={() => {
            void props.refreshDevices();
          }}
          disabled={isLoading()}>
          <Show when={isLoading()} fallback={"Refresh list"}>
            <Refresher />
          </Show>
        </button>
      </div>
    </div>
  );
}

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

function WhisperModelSelector(props: { whisperModel: string; onWhisperModelChange: (whisperModel: string) => void }) {
  const whisperModel = () => props.whisperModel;
  return (
    <label class="grid gap-2">
      <span class="text-xs font-semibold tracking-[0.14em] text-subtext uppercase">Whisper model</span>
      <select
        class="rounded-xl border border-overlay bg-surface/45 px-3 py-2 text-sm text-text focus:border-accent/60 focus:outline-hidden"
        value={whisperModel()}
        onInput={(event) => {
          const value = (event.currentTarget as HTMLSelectElement).value;
          props.onWhisperModelChange(value);
        }}>
        <For each={WHISPER_MODEL_OPTIONS}>{(option) => <option value={option.value}>{option.label}</option>}</For>
        <Show when={!WHISPER_MODEL_OPTIONS.some((option) => option.value === whisperModel())}>
          <option value={whisperModel()}>{whisperModel()}</option>
        </Show>
      </select>
    </label>
  );
}

function WhisperLanguageSelector(
  props: { whisperLanguage: string; onWhisperLanguageChange: (whisperLanguage: string) => void },
) {
  const whisperLanguage = () => props.whisperLanguage;
  return (
    <label class="grid gap-2">
      <span class="text-xs font-semibold tracking-[0.14em] text-subtext uppercase">Default language</span>
      <input
        list="whisper-language-options"
        class="rounded-xl border border-overlay bg-surface/45 px-3 py-2 text-sm text-text focus:border-accent/60 focus:outline-hidden"
        value={whisperLanguage()}
        onInput={(event) => {
          const value = (event.currentTarget as HTMLInputElement).value;
          props.onWhisperLanguageChange(value);
        }} />
      <datalist id="whisper-language-options">
        <For each={WHISPER_LANGUAGE_OPTIONS}>{(language) => <option value={language} />}</For>
      </datalist>
    </label>
  );
}

function WhisperThreadsInput(
  props: { whisperThreads: number; onWhisperThreadsChange: (whisperThreads: number) => void },
) {
  const whisperThreads = () => props.whisperThreads;
  return (
    <label class="grid gap-2">
      <span class="text-xs font-semibold tracking-[0.14em] text-subtext uppercase">Whisper threads</span>
      <input
        type="number"
        min="1"
        max="32"
        class="rounded-xl border border-overlay bg-surface/45 px-3 py-2 text-sm text-text focus:border-accent/60 focus:outline-hidden"
        value={whisperThreads()}
        onInput={(event) => {
          const value = Number((event.currentTarget as HTMLInputElement).value);
          if (Number.isFinite(value)) {
            props.onWhisperThreadsChange(value);
          }
        }} />
    </label>
  );
}

function OllamaEndpointInput(
  props: { ollamaEndpoint: string; onOllamaEndpointChange: (ollamaEndpoint: string) => void },
) {
  const ollamaEndpoint = () => props.ollamaEndpoint;
  return (
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
  );
}

function CheckOllamaButton(props: { isCheckingOllama: boolean; checkOllama: () => void }) {
  return (
    <div class="flex flex-wrap items-center gap-2">
      <button
        type="button"
        class="rounded-xl border border-overlay px-3 py-1.5 text-xs font-semibold text-subtext transition hover:border-accent/35 hover:text-text disabled:opacity-60"
        disabled={props.isCheckingOllama}
        onClick={() => {
          void props.checkOllama();
        }}>
        {props.isCheckingOllama ? "Checking Ollama..." : "Re-check Ollama connection"}
      </button>
    </div>
  );
}

function InstalledWhisperModels(
  props: {
    modelInventory: WhisperModelInventory | null;
    selectedModelName: string;
    deletingModelName: string | null;
    deleteModel: (modelName: string) => void;
    selectedModelInstalled: boolean;
  },
) {
  const modelInventory = () => props.modelInventory;
  const deletingModelName = () => props.deletingModelName;
  const selectedModelInstalled = () => props.selectedModelInstalled;

  return (
    <section class="space-y-3 rounded-2xl border border-overlay bg-surface/30 p-4">
      <div class="flex flex-wrap items-center justify-between gap-2">
        <p class="text-sm font-semibold text-text">Installed whisper models</p>
        <p class="text-xs text-subtext">Disk usage: {formatBytes(modelInventory()?.totalSizeBytes ?? 0)}</p>
      </div>
      <Show
        when={(modelInventory()?.installedModels.length ?? 0) > 0}
        fallback={
          <p class="rounded-xl border border-overlay bg-surface/35 px-3 py-2 text-xs text-subtext">
            No whisper models installed yet.
          </p>
        }>
        <div class="grid gap-2">
          <For each={modelInventory()?.installedModels ?? []}>
            {(model) => (
              <div class="flex items-center justify-between rounded-xl border border-overlay bg-surface/35 px-3 py-2 text-xs text-subtext">
                <div>
                  <p class="text-sm font-semibold text-text">{model.modelName}</p>
                  <p>{model.fileName}</p>
                </div>
                <div class="flex items-center gap-2">
                  <span>{formatBytes(model.sizeBytes)}</span>
                  <button
                    type="button"
                    class="rounded-lg border border-overlay px-2 py-1 text-[11px] font-semibold text-subtext transition hover:border-accent/35 hover:text-text disabled:opacity-60"
                    disabled={deletingModelName() === model.modelName}
                    onClick={() => {
                      void props.deleteModel(model.modelName);
                    }}>
                    {deletingModelName() === model.modelName ? "Deleting..." : "Delete"}
                  </button>
                </div>
              </div>
            )}
          </For>
        </div>
      </Show>
      <p class="text-xs text-subtext">Selected model is {selectedModelInstalled() ? "installed" : "not installed"}.</p>
    </section>
  );
}

export function SettingsView() {
  const isSupported = supportsMediaRecording();
  const [state, setState] = createStore<SettingsStore>({
    isBusy: false,
    isSavingSettings: false,
    isCheckingOllama: false,
    isRunningPreflight: false,
    isDownloadingModel: false,
    deletingModelName: null,
    settings: {
      whisperModel: "base.en",
      whisperLanguage: "auto",
      whisperThreads: 4,
      ollamaEndpoint: "http://localhost:11434",
    },
    modelInventory: null,
    ollamaStatus: null,
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
        ollamaEndpoint: state.settings.ollamaEndpoint,
      });
      setState("settings", saved);
      setState("settingsInfo", "Saved settings.");
      const inventory = await invoke<WhisperModelInventory>("list_whisper_models");
      setState("modelInventory", inventory);
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

  const checkOllama = async () => {
    setState("isCheckingOllama", true);
    setState("settingsError", null);
    try {
      const status = await invoke<OllamaConnectionStatus>("check_ollama_connection");
      setState("ollamaStatus", status);
    } catch (ollamaError) {
      setState("settingsError", normalizeError(ollamaError));
      setState("ollamaStatus", null);
    } finally {
      setState("isCheckingOllama", false);
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
          description="Configure microphone defaults for in-app recording. Choose which input device Audio X should use before opening the Record view.">
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
        description="Manage whisper/Ollama defaults, model downloads, and recording device preferences.">
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

            <OllamaEndpointInput
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
            <CheckOllamaButton isCheckingOllama={state.isCheckingOllama} checkOllama={checkOllama} />
            <Show when={state.ollamaStatus}>
              {(status) => (
                <div class="rounded-xl border border-overlay bg-surface/35 p-3 text-xs text-subtext">
                  <p class="text-sm font-semibold text-text">{status().reachable ? "Reachable" : "Unavailable"}</p>
                  <p class="mt-1">Endpoint: {status().endpoint}</p>
                  <p class="mt-1">{status().message}</p>
                  <Show when={status().installedModels.length > 0}>
                    <p class="mt-1">Installed: {status().installedModels.join(", ")}</p>
                  </Show>
                  <Show when={status().missingModels.length > 0}>
                    <p class="mt-1">Missing: {status().missingModels.join(", ")}</p>
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
