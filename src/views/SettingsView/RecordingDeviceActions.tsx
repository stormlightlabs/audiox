import { Show } from "solid-js";
import { Refresher } from "./Refresher";

export function RecordingDeviceActions(
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
