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
import { createSignal, For, onMount, Show } from "solid-js";
import { ViewScaffold } from "./ViewScaffold";

type DeviceSelectorProps = {
  devices: AudioInputDevice[];
  selectedDeviceId: string;
  onDeviceChange: (deviceId: string) => void;
};

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
          {hasPermission() ? "Permission granted" : "Enable mic access"}
        </button>
        <button
          type="button"
          class="rounded-xl border border-overlay px-3 py-1.5 text-xs font-semibold text-subtext transition hover:border-accent/35 hover:text-text disabled:opacity-60"
          onClick={() => {
            void props.refreshDevices();
          }}
          disabled={isLoading()}>
          {isLoading() ? "Refreshing..." : "Refresh list"}
        </button>
      </div>
    </div>
  );
}

export function SettingsView() {
  const [isSupported] = createSignal(supportsMediaRecording());
  const [isLoading, setIsLoading] = createSignal(false);
  const [devices, setDevices] = createSignal<AudioInputDevice[]>([]);
  const [selectedDeviceId, setSelectedDeviceId] = createSignal<string>(getPreferredAudioInputDeviceId() ?? "");
  const [hasPermission, setHasPermission] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [info, setInfo] = createSignal<string | null>(null);

  const refreshDevices = async () => {
    if (!isSupported()) {
      return;
    }

    setIsLoading(true);
    setError(null);
    try {
      const permission = await checkMicrophonePermission();
      setHasPermission(permission.granted);
      if (!permission.granted) {
        setDevices([]);
        return;
      }

      const inputs = await listAudioInputDevices();
      setDevices(inputs);
      if (selectedDeviceId().length > 0 && !inputs.some((device) => device.id === selectedDeviceId())) {
        setSelectedDeviceId("");
        setPreferredAudioInputDeviceId(null);
      }
    } catch (refreshError) {
      setError(normalizeError(refreshError));
    } finally {
      setIsLoading(false);
    }
  };

  const requestPermission = async () => {
    setInfo(null);
    setError(null);
    try {
      const permission = await requestMicrophonePermission();
      setHasPermission(permission.granted);
      if (!permission.granted) {
        setError(
          permission.canRequest
            ? "Microphone permission was not granted."
            : "Microphone permission is denied at the system level.",
        );
        return;
      }
      setInfo("Microphone permission granted.");
      await refreshDevices();
    } catch (permissionError) {
      setError(normalizeError(permissionError));
    }
  };

  const onDeviceChange = (nextDeviceId: string) => {
    setSelectedDeviceId(nextDeviceId);
    setPreferredAudioInputDeviceId(nextDeviceId || null);
    setInfo(nextDeviceId ? "Saved preferred microphone." : "Using the system default microphone.");
  };

  const selectedDeviceName = () => {
    const selected = selectedDeviceId();
    if (!selected) {
      return "System default input";
    }
    const device = devices().find((item) => item.id === selected);
    return device?.name || "Saved input device";
  };

  onMount(() => {
    void refreshDevices();
  });

  return (
    <Show
      when={isSupported()}
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
        description="Configure microphone defaults for in-app recording. Choose which input device Audio X should use before opening the Record view.">
        <section class="space-y-4 rounded-3xl border border-overlay bg-elevation/85 p-6">
          <RecordingDeviceActions
            isLoading={isLoading()}
            hasPermission={hasPermission()}
            requestPermission={requestPermission}
            refreshDevices={refreshDevices} />

          <DeviceSelector devices={devices()} selectedDeviceId={selectedDeviceId()} onDeviceChange={onDeviceChange} />

          <p class="rounded-xl border border-overlay bg-surface/35 px-3 py-2 text-xs text-subtext">
            Preferred input: {selectedDeviceName()}.
          </p>

          {devices().length === 0 && hasPermission() && !isLoading() && (
            <p class="rounded-xl border border-overlay bg-surface/35 px-3 py-2 text-xs text-subtext">
              No audio inputs were detected. Connect a microphone and refresh the device list.
            </p>
          )}
          {info() && <p class="rounded-xl border border-overlay bg-surface/35 p-3 text-sm text-subtext">{info()}</p>}
          {error() && (
            <p role="alert" class="rounded-xl border border-accent/50 bg-accent/10 p-3 text-sm text-text">{error()}</p>
          )}
        </section>
      </ViewScaffold>
    </Show>
  );
}
