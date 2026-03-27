import { formatElapsed } from "$/format-utils";
import { Motion } from "solid-motionone";
import { RecordingPhase } from "./lib";

type RecorderControlsProps = {
  phase: RecordingPhase;
  elapsedMs: number;
  onStart: () => void;
  onPause: () => void;
  onResume: () => void;
  onStop: () => void;
  setCanvasRef: (element: HTMLCanvasElement) => void;
};

export function RecorderControls(props: RecorderControlsProps) {
  return (
    <div class="grid gap-3 rounded-2xl border border-overlay bg-surface/35 p-4">
      <div class="flex flex-wrap items-center justify-between gap-3">
        <div class="flex items-center gap-3">
          {props.phase === "recording" && (
            <Motion.div
              class="h-3 w-3 rounded-full bg-red-400"
              animate={{ scale: [1, 1.5, 1], opacity: [1, 0.45, 1] }}
              transition={{ duration: 1, repeat: Number.POSITIVE_INFINITY }} />
          )}
          <p class="text-lg font-semibold text-text">{formatElapsed(props.elapsedMs)}</p>
          <p class="text-xs font-semibold tracking-[0.16em] text-subtext uppercase">{props.phase}</p>
        </div>
        <p class="text-xs text-subtext">Native recorder plugin active (16kHz mono WAV).</p>
      </div>

      <canvas
        ref={(element) => {
          props.setCanvasRef(element);
        }}
        class="h-24 w-full rounded-xl border border-overlay bg-[#0a101e]"
        width="600"
        height="120" />

      <div class="flex flex-wrap items-center gap-2">
        <button
          type="button"
          class="rounded-xl bg-accent px-4 py-2 text-sm font-semibold text-surface transition hover:brightness-110 disabled:cursor-not-allowed disabled:opacity-60"
          onClick={() => props.onStart()}
          disabled={props.phase !== "idle"}>
          Start recording
        </button>

        <button
          type="button"
          class="rounded-xl border border-overlay px-4 py-2 text-sm font-semibold text-text transition hover:border-accent/35 disabled:cursor-not-allowed disabled:opacity-60"
          onClick={() => props.onPause()}
          disabled={props.phase !== "recording"}>
          Pause
        </button>

        <button
          type="button"
          class="rounded-xl border border-overlay px-4 py-2 text-sm font-semibold text-text transition hover:border-accent/35 disabled:cursor-not-allowed disabled:opacity-60"
          onClick={() => props.onResume()}
          disabled={props.phase !== "paused"}>
          Resume
        </button>

        <button
          type="button"
          class="rounded-xl border border-red-400/70 bg-red-500/10 px-4 py-2 text-sm font-semibold text-red-200 transition hover:border-red-300 disabled:cursor-not-allowed disabled:opacity-60"
          onClick={() => props.onStop()}
          disabled={props.phase !== "recording" && props.phase !== "paused"}>
          Stop
        </button>
      </div>
    </div>
  );
}
