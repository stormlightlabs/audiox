import { AudioInputDevice } from "$/devices";
import { For } from "solid-js";

type DeviceSelectorProps = {
  devices: AudioInputDevice[];
  selectedDeviceId: string;
  onDeviceChange: (deviceId: string) => void;
};

export function DeviceSelector(props: DeviceSelectorProps) {
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
