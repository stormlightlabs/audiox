import { checkMicrophonePermission, requestMicrophonePermission, supportsMediaRecording } from "$/devices";
import { normalizeError } from "$/errors";
import { formatElapsed } from "$/format-utils";
import type { ProgressStatus } from "$/types";
import { useNavigate } from "@solidjs/router";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { appDataDir, join } from "@tauri-apps/api/path";
import { createSignal, onCleanup, onMount } from "solid-js";
import { Motion } from "solid-motionone";
import {
  getStatus,
  pauseRecording as pauseNativeRecording,
  resumeRecording as resumeNativeRecording,
  startRecording as startNativeRecording,
  stopRecording as stopNativeRecording,
} from "tauri-plugin-audio-recorder-api";
import { ViewScaffold } from "./ViewScaffold";

const IMPORT_CONVERSION_PROGRESS_EVENT = "import://conversion-progress";
const IMPORT_TRANSCRIPTION_PROGRESS_EVENT = "import://transcription-progress";
const IMPORT_METADATA_PROGRESS_EVENT = "import://metadata-progress";

type ConversionProgress = {
  status: ProgressStatus;
  message: string;
  outTimeMs: number;
  totalDurationMs: number | null;
  percent: number;
};

type TranscriptionProgress = { status: ProgressStatus; message: string; percent: number };
type MetadataProgress = { status: ProgressStatus; message: string; percent: number };

type ImportedDocument = {
  id: string;
  title: string;
  summary: string | null;
  tags: string[];
  transcript: string;
  audioPath: string;
  subtitleSrtPath: string;
  subtitleVttPath: string;
  durationSeconds: number;
  createdAt: string;
  segments: Array<{ startMs: number; endMs: number; text: string }>;
};

type RecordingPhase = "idle" | "recording" | "paused" | "processing";

function ProgressBar(props: { percent: number }) {
  return (
    <div class="h-2 overflow-hidden rounded-full border border-overlay bg-surface/50">
      <div
        class="h-full rounded-full bg-accent/75 transition-[width] duration-200"
        style={{ width: `${Math.max(0, Math.min(100, props.percent))}%` }} />
    </div>
  );
}

type RecorderControlsProps = {
  phase: RecordingPhase;
  elapsedMs: number;
  onStart: () => void;
  onPause: () => void;
  onResume: () => void;
  onStop: () => void;
  setCanvasRef: (element: HTMLCanvasElement) => void;
};

function RecorderControls(props: RecorderControlsProps) {
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
          onClick={props.onStart}
          disabled={props.phase !== "idle"}>
          Start recording
        </button>

        <button
          type="button"
          class="rounded-xl border border-overlay px-4 py-2 text-sm font-semibold text-text transition hover:border-accent/35 disabled:cursor-not-allowed disabled:opacity-60"
          onClick={props.onPause}
          disabled={props.phase !== "recording"}>
          Pause
        </button>

        <button
          type="button"
          class="rounded-xl border border-overlay px-4 py-2 text-sm font-semibold text-text transition hover:border-accent/35 disabled:cursor-not-allowed disabled:opacity-60"
          onClick={props.onResume}
          disabled={props.phase !== "paused"}>
          Resume
        </button>

        <button
          type="button"
          class="rounded-xl border border-red-400/70 bg-red-500/10 px-4 py-2 text-sm font-semibold text-red-200 transition hover:border-red-300 disabled:cursor-not-allowed disabled:opacity-60"
          onClick={props.onStop}
          disabled={props.phase !== "recording" && props.phase !== "paused"}>
          Stop
        </button>
      </div>
    </div>
  );
}

function PipelineProgressCard(props: { title: string; status: ProgressStatus; message: string; percent: number }) {
  return (
    <article class="space-y-2 rounded-2xl border border-overlay bg-surface/45 p-4">
      <div class="flex items-center justify-between gap-3">
        <p class="text-sm font-semibold text-text">{props.title}</p>
        <span class="text-xs font-semibold tracking-[0.16em] text-subtext uppercase">{props.status}</span>
      </div>
      <p class="text-xs text-subtext">{props.message}</p>
      <ProgressBar percent={props.percent} />
    </article>
  );
}

async function buildRecordingOutputPath(): Promise<string> {
  const root = await appDataDir();
  const recordingId = typeof crypto !== "undefined" && typeof crypto.randomUUID === "function"
    ? crypto.randomUUID()
    : `${Date.now()}-${Math.round(Math.random() * 100_000)}`;
  return join(root, "audio", `${recordingId}-recording`);
}

export function RecordView() {
  const navigate = useNavigate();
  const [isSupported] = createSignal(supportsMediaRecording());
  const [phase, setPhase] = createSignal<RecordingPhase>("idle");
  const [error, setError] = createSignal<string | null>(null);
  const [elapsedMs, setElapsedMs] = createSignal(0);
  const [conversionProgress, setConversionProgress] = createSignal<ConversionProgress | null>(null);
  const [transcriptionProgress, setTranscriptionProgress] = createSignal<TranscriptionProgress | null>(null);
  const [metadataProgress, setMetadataProgress] = createSignal<MetadataProgress | null>(null);
  const [lastDocument, setLastDocument] = createSignal<ImportedDocument | null>(null);
  let waveformCanvas: HTMLCanvasElement | undefined;

  let unlistenConversion: UnlistenFn | undefined;
  let unlistenTranscription: UnlistenFn | undefined;
  let unlistenMetadata: UnlistenFn | undefined;
  let waveformFrame: number | undefined;
  let statusInterval: number | undefined;

  const stopStatusPolling = () => {
    if (statusInterval !== undefined) {
      globalThis.clearInterval(statusInterval);
      statusInterval = undefined;
    }
  };

  const drawIdleWaveform = () => {
    if (!waveformCanvas) {
      return;
    }

    const context = waveformCanvas.getContext("2d");
    if (!context) {
      return;
    }

    const { width: cssWidth, height: cssHeight } = waveformCanvas.getBoundingClientRect();
    const pixelRatio = Math.max(1, globalThis.devicePixelRatio || 1);
    const width = Math.max(1, Math.floor(cssWidth * pixelRatio));
    const height = Math.max(1, Math.floor(cssHeight * pixelRatio));
    if (waveformCanvas.width !== width || waveformCanvas.height !== height) {
      waveformCanvas.width = width;
      waveformCanvas.height = height;
    }

    context.fillStyle = "rgba(10, 16, 30, 0.9)";
    context.fillRect(0, 0, width, height);
    context.lineWidth = Math.max(1, pixelRatio * 1.2);
    context.strokeStyle = "rgba(70, 148, 255, 0.38)";
    context.beginPath();
    context.moveTo(0, height / 2);
    context.lineTo(width, height / 2);
    context.stroke();
  };

  const stopWaveformLoop = () => {
    if (waveformFrame !== undefined) {
      globalThis.cancelAnimationFrame(waveformFrame);
      waveformFrame = undefined;
    }
    drawIdleWaveform();
  };

  const drawWaveformFrame = (time: number) => {
    if (!waveformCanvas) {
      waveformFrame = undefined;
      return;
    }

    const context = waveformCanvas.getContext("2d");
    if (!context) {
      waveformFrame = undefined;
      return;
    }

    const { width: cssWidth, height: cssHeight } = waveformCanvas.getBoundingClientRect();
    const pixelRatio = Math.max(1, globalThis.devicePixelRatio || 1);
    const width = Math.max(1, Math.floor(cssWidth * pixelRatio));
    const height = Math.max(1, Math.floor(cssHeight * pixelRatio));
    if (waveformCanvas.width !== width || waveformCanvas.height !== height) {
      waveformCanvas.width = width;
      waveformCanvas.height = height;
    }

    const recordingPhase = phase();
    const isActive = recordingPhase === "recording" || recordingPhase === "paused";
    const baseAmplitude = recordingPhase === "recording" ? 0.26 : 0.06;
    const amplitude = height * baseAmplitude;

    context.fillStyle = "rgba(10, 16, 30, 0.9)";
    context.fillRect(0, 0, width, height);

    context.lineWidth = Math.max(1, pixelRatio * 1.2);
    context.strokeStyle = recordingPhase === "recording" ? "rgba(70, 148, 255, 0.96)" : "rgba(70, 148, 255, 0.55)";
    context.beginPath();

    const centerY = height / 2;
    const points = 120;
    for (let index = 0; index < points; index += 1) {
      const progress = index / Math.max(1, points - 1);
      const x = progress * width;
      const envelope = Math.sin(progress * Math.PI);
      const waveA = Math.sin((progress * 24) + (time * 0.006));
      const waveB = Math.sin((progress * 41) - (time * 0.0042));
      const waveC = Math.sin((progress * 73) + (time * 0.0028));
      const signal = isActive ? (waveA * 0.52) + (waveB * 0.33) + (waveC * 0.15) : 0;
      const y = centerY + (signal * amplitude * envelope);

      if (index === 0) {
        context.moveTo(x, y);
      } else {
        context.lineTo(x, y);
      }
    }
    context.stroke();

    if (isActive) {
      waveformFrame = globalThis.requestAnimationFrame(drawWaveformFrame);
      return;
    }

    waveformFrame = undefined;
    drawIdleWaveform();
  };

  const startWaveformLoop = () => {
    if (waveformFrame !== undefined) {
      return;
    }
    waveformFrame = globalThis.requestAnimationFrame(drawWaveformFrame);
  };

  const pollRecorderStatus = async () => {
    try {
      const status = await getStatus();
      setElapsedMs(status.durationMs);
    } catch {
      // Ignore transient status polling failures.
    }
  };

  const startStatusPolling = () => {
    stopStatusPolling();
    void pollRecorderStatus();
    statusInterval = globalThis.setInterval(() => {
      void pollRecorderStatus();
    }, 120);
  };

  const ensurePermission = async () => {
    const currentPermission = await checkMicrophonePermission();
    if (currentPermission.granted) {
      return true;
    }

    if (!currentPermission.canRequest) {
      setError("Microphone permission is denied at the system level.");
      return false;
    }

    const requested = await requestMicrophonePermission();
    if (requested.granted) {
      return true;
    }

    setError(requested.canRequest ? "Microphone permission was not granted." : "Microphone access is blocked.");
    return false;
  };

  const resetProgressCards = () => {
    setConversionProgress(null);
    setTranscriptionProgress(null);
    setMetadataProgress(null);
  };

  const startRecording = async () => {
    if (!isSupported()) {
      setError("This environment does not support native microphone recording.");
      return;
    }

    setError(null);
    setLastDocument(null);
    resetProgressCards();

    try {
      const allowed = await ensurePermission();
      if (!allowed) {
        return;
      }

      const outputPath = await buildRecordingOutputPath();
      await startNativeRecording({ outputPath, quality: "low", format: "wav", maxDuration: 0 });

      setElapsedMs(0);
      setPhase("recording");
      startStatusPolling();
      startWaveformLoop();
    } catch (startError) {
      stopStatusPolling();
      stopWaveformLoop();
      setPhase("idle");
      setError(normalizeError(startError));
    }
  };

  const pauseRecording = async () => {
    if (phase() !== "recording") {
      return;
    }

    try {
      await pauseNativeRecording();
      setPhase("paused");
      startWaveformLoop();
      await pollRecorderStatus();
    } catch (pauseError) {
      setError(normalizeError(pauseError));
    }
  };

  const resumeRecording = async () => {
    if (phase() !== "paused") {
      return;
    }

    try {
      await resumeNativeRecording();
      setPhase("recording");
      startWaveformLoop();
      await pollRecorderStatus();
    } catch (resumeError) {
      setError(normalizeError(resumeError));
    }
  };

  const stopRecording = async () => {
    if (phase() !== "recording" && phase() !== "paused") {
      return;
    }

    setError(null);
    setPhase("processing");
    stopStatusPolling();
    stopWaveformLoop();

    try {
      const recordingResult = await stopNativeRecording();
      setElapsedMs(recordingResult.durationMs);

      const imported = await invoke<ImportedDocument>("import_recorded_audio", {
        sourcePath: recordingResult.filePath,
      });
      setLastDocument(imported);
      setPhase("idle");
      await navigate(`/document/${imported.id}`);
    } catch (stopError) {
      setPhase("idle");
      setError(normalizeError(stopError));
    }
  };

  onMount(() => {
    drawIdleWaveform();

    void (async () => {
      try {
        unlistenConversion = await listen<ConversionProgress>(IMPORT_CONVERSION_PROGRESS_EVENT, (event) => {
          setConversionProgress(event.payload);
        });
        unlistenTranscription = await listen<TranscriptionProgress>(IMPORT_TRANSCRIPTION_PROGRESS_EVENT, (event) => {
          setTranscriptionProgress(event.payload);
        });
        unlistenMetadata = await listen<MetadataProgress>(IMPORT_METADATA_PROGRESS_EVENT, (event) => {
          setMetadataProgress(event.payload);
        });
      } catch {
        // Event channels are unavailable in plain browser contexts.
      }
    })();
  });

  onCleanup(() => {
    if (unlistenConversion) {
      void unlistenConversion();
    }
    if (unlistenTranscription) {
      void unlistenTranscription();
    }
    if (unlistenMetadata) {
      void unlistenMetadata();
    }

    stopStatusPolling();
    stopWaveformLoop();

    void (async () => {
      try {
        const status = await getStatus();
        if (status.state !== "idle") {
          await stopNativeRecording();
        }
      } catch {
        // Ignore cleanup errors.
      }
    })();
  });

  if (!isSupported()) {
    return (
      <ViewScaffold
        eyebrow="Capture"
        title="Microphone recording"
        description="Capture speech directly from your microphone, monitor a live waveform, then process the recording through ffmpeg + whisper into a library document.">
        <section class="space-y-4 rounded-3xl border border-overlay bg-elevation/85 p-6">
          <p role="alert" class="rounded-xl border border-accent/50 bg-accent/10 p-3 text-sm text-text">
            This view requires the native Tauri runtime.
          </p>
        </section>
      </ViewScaffold>
    );
  }

  const conversion = conversionProgress();
  const transcription = transcriptionProgress();
  const metadata = metadataProgress();
  const document = lastDocument();
  const errorMessage = error();

  return (
    <ViewScaffold
      eyebrow="Capture"
      title="Microphone recording"
      description="Capture speech directly from your microphone, monitor a live waveform, then process the recording through ffmpeg + whisper into a library document.">
      <section class="space-y-4 rounded-3xl border border-overlay bg-elevation/85 p-6">
        <RecorderControls
          phase={phase()}
          elapsedMs={elapsedMs()}
          onStart={() => {
            void startRecording();
          }}
          onPause={() => {
            void pauseRecording();
          }}
          onResume={() => {
            void resumeRecording();
          }}
          onStop={() => {
            void stopRecording();
          }}
          setCanvasRef={(element) => {
            waveformCanvas = element;
          }} />

        {phase() === "processing" && (
          <p class="rounded-xl border border-overlay bg-surface/35 p-3 text-sm text-subtext">
            Processing recording with ffmpeg, whisper, and metadata generation.
          </p>
        )}
        {conversion && (
          <PipelineProgressCard
            title="ffmpeg conversion"
            status={conversion.status}
            message={conversion.message}
            percent={conversion.percent} />
        )}
        {transcription && (
          <PipelineProgressCard
            title="whisper transcription"
            status={transcription.status}
            message={transcription.message}
            percent={transcription.percent} />
        )}
        {metadata && (
          <PipelineProgressCard
            title="metadata generation"
            status={metadata.status}
            message={metadata.message}
            percent={metadata.percent} />
        )}
        {document && (
          <article class="rounded-2xl border border-overlay bg-surface/35 p-4">
            <p class="text-sm font-semibold text-text">{document.title} saved.</p>
            <p class="mt-1 text-xs text-subtext">{document.segments.length} timestamped segments captured.</p>
          </article>
        )}
        {errorMessage && (
          <p role="alert" class="rounded-xl border border-accent/50 bg-accent/10 p-3 text-sm text-text">
            {errorMessage}
          </p>
        )}
      </section>
    </ViewScaffold>
  );
}
