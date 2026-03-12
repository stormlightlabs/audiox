import { getPreferredAudioInputDeviceId, setPreferredAudioInputDeviceId, supportsMediaRecording } from "$/devices";
import { normalizeError } from "$/errors";
import { formatElapsed } from "$/format-utils";
import type { ProgressStatus } from "$/types";
import { useNavigate } from "@solidjs/router";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { createSignal, onCleanup, onMount } from "solid-js";
import { Motion } from "solid-motionone";
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

function preferredRecordingMimeType(): string {
  const candidates = ["audio/webm;codecs=opus", "audio/webm", "audio/ogg;codecs=opus", "audio/ogg", "audio/mp4"];

  if (typeof MediaRecorder === "undefined" || typeof MediaRecorder.isTypeSupported !== "function") {
    return "";
  }

  for (const mimeType of candidates) {
    if (MediaRecorder.isTypeSupported(mimeType)) {
      return mimeType;
    }
  }
  return "";
}

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
        <p class="text-xs text-subtext">Preferred input from Settings is used automatically.</p>
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
  let mediaRecorder: MediaRecorder | null = null;
  let mediaStream: MediaStream | null = null;
  let recordedChunks: BlobPart[] = [];
  let activeMimeType = "";
  let elapsedInterval: number | undefined;
  let elapsedBaseMs = 0;
  let segmentStartedAt = 0;
  let audioContext: AudioContext | null = null;
  let analyser: AnalyserNode | null = null;
  let sourceNode: MediaStreamAudioSourceNode | null = null;
  let waveformFrame: number | undefined;
  let isCleaningUp = false;

  const stopElapsedTimer = () => {
    if (elapsedInterval !== undefined) {
      globalThis.clearInterval(elapsedInterval);
      elapsedInterval = undefined;
    }
  };

  const currentElapsedMilliseconds = () => {
    if (phase() === "recording") {
      return elapsedBaseMs + (performance.now() - segmentStartedAt);
    }
    return elapsedBaseMs;
  };

  const startElapsedTimer = () => {
    stopElapsedTimer();
    elapsedInterval = globalThis.setInterval(() => {
      setElapsedMs(currentElapsedMilliseconds());
    }, 50);
  };

  const stopWaveform = () => {
    if (waveformFrame !== undefined) {
      globalThis.cancelAnimationFrame(waveformFrame);
      waveformFrame = undefined;
    }
    if (sourceNode) {
      sourceNode.disconnect();
      sourceNode = null;
    }
    if (analyser) {
      analyser.disconnect();
      analyser = null;
    }
    if (audioContext) {
      void audioContext.close();
      audioContext = null;
    }
    if (waveformCanvas) {
      const context = waveformCanvas.getContext("2d");
      if (context) {
        context.fillStyle = "rgba(20, 28, 46, 0.9)";
        context.fillRect(0, 0, waveformCanvas.width || 320, waveformCanvas.height || 80);
      }
    }
  };

  const drawWaveform = () => {
    if (!waveformCanvas || !analyser) {
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

    const samples = new Uint8Array(analyser.fftSize);
    analyser.getByteTimeDomainData(samples);

    context.fillStyle = "rgba(10, 16, 30, 0.9)";
    context.fillRect(0, 0, width, height);

    context.lineWidth = Math.max(1, pixelRatio * 1.2);
    context.strokeStyle = "rgba(70, 148, 255, 0.95)";
    context.beginPath();
    for (const [index, sample] of samples.entries()) {
      const x = (index / (samples.length - 1)) * width;
      const normalized = sample / 128;
      const y = (normalized * height) / 2;
      if (index === 0) {
        context.moveTo(x, y);
      } else {
        context.lineTo(x, y);
      }
    }
    context.stroke();

    waveformFrame = globalThis.requestAnimationFrame(drawWaveform);
  };

  const startWaveform = (stream: MediaStream) => {
    stopWaveform();
    audioContext = new AudioContext();
    analyser = audioContext.createAnalyser();
    analyser.fftSize = 2048;
    sourceNode = audioContext.createMediaStreamSource(stream);
    sourceNode.connect(analyser);
    drawWaveform();
  };

  const stopMediaStream = () => {
    if (!mediaStream) {
      return;
    }
    for (const track of mediaStream.getTracks()) {
      track.stop();
    }
    mediaStream = null;
  };

  const resetRecorderState = () => {
    stopElapsedTimer();
    stopWaveform();
    stopMediaStream();
    mediaRecorder = null;
    recordedChunks = [];
    activeMimeType = "";
    elapsedBaseMs = 0;
    segmentStartedAt = 0;
    setElapsedMs(0);
  };

  const handleRecordingCompleted = async () => {
    const blobType = activeMimeType || "audio/webm";
    const recordingBlob = new Blob(recordedChunks, { type: blobType });
    recordedChunks = [];
    if (recordingBlob.size === 0) {
      setPhase("idle");
      setError("Recording was empty. Capture at least a short sample and try again.");
      return;
    }

    setPhase("processing");
    setConversionProgress(null);
    setTranscriptionProgress(null);
    setMetadataProgress(null);
    try {
      const payload = new Uint8Array(await recordingBlob.arrayBuffer());
      const imported = await invoke<ImportedDocument>("import_recorded_audio", {
        audioBytes: [...payload],
        mimeType: recordingBlob.type || blobType,
      });
      setLastDocument(imported);
      setPhase("idle");
      await navigate(`/document/${imported.id}`);
    } catch (processingError) {
      setPhase("idle");
      setError(normalizeError(processingError));
    }
  };

  const stopRecording = () => {
    if (!mediaRecorder || mediaRecorder.state === "inactive") {
      return;
    }
    if (phase() === "recording") {
      elapsedBaseMs += performance.now() - segmentStartedAt;
      setElapsedMs(elapsedBaseMs);
    }
    stopElapsedTimer();
    stopWaveform();
    mediaRecorder.stop();
    stopMediaStream();
  };

  const pauseRecording = () => {
    if (!mediaRecorder || mediaRecorder.state !== "recording") {
      return;
    }
    elapsedBaseMs += performance.now() - segmentStartedAt;
    setElapsedMs(elapsedBaseMs);
    mediaRecorder.pause();
    stopElapsedTimer();
    stopWaveform();
    setPhase("paused");
  };

  const resumeRecording = () => {
    if (!mediaRecorder || mediaRecorder.state !== "paused") {
      return;
    }
    segmentStartedAt = performance.now();
    mediaRecorder.resume();
    startElapsedTimer();
    if (mediaStream) {
      startWaveform(mediaStream);
    }
    setPhase("recording");
  };

  const startRecording = async () => {
    if (!isSupported()) {
      setError("This environment does not support microphone recording.");
      return;
    }
    if (!navigator.mediaDevices?.getUserMedia) {
      setError("Media device APIs are unavailable in this environment.");
      return;
    }

    setError(null);
    setLastDocument(null);
    setConversionProgress(null);
    setTranscriptionProgress(null);
    setMetadataProgress(null);

    const preferredDeviceId = getPreferredAudioInputDeviceId();
    const preferredConstraints = preferredDeviceId ? { deviceId: { exact: preferredDeviceId } } : true;
    try {
      mediaStream = await navigator.mediaDevices.getUserMedia({ audio: preferredConstraints });
    } catch (streamError) {
      const canRetryWithDefault = preferredDeviceId
        && streamError instanceof DOMException
        && ["NotFoundError", "OverconstrainedError", "NotReadableError"].includes(streamError.name);
      if (!canRetryWithDefault) {
        setError(normalizeError(streamError));
        return;
      }

      try {
        setPreferredAudioInputDeviceId(null);
        mediaStream = await navigator.mediaDevices.getUserMedia({ audio: true });
      } catch (fallbackError) {
        setError(normalizeError(fallbackError));
        return;
      }
    }

    const mimeType = preferredRecordingMimeType();
    const options = mimeType ? { mimeType } : undefined;

    try {
      mediaRecorder = new MediaRecorder(mediaStream, options);
    } catch (recorderError) {
      stopMediaStream();
      setError(normalizeError(recorderError));
      return;
    }

    activeMimeType = mimeType;
    recordedChunks = [];
    elapsedBaseMs = 0;
    segmentStartedAt = performance.now();
    setElapsedMs(0);
    setPhase("recording");
    startElapsedTimer();
    startWaveform(mediaStream);

    mediaRecorder.ondataavailable = (event) => {
      if (event.data.size > 0) {
        recordedChunks.push(event.data);
      }
    };
    mediaRecorder.onstop = () => {
      if (isCleaningUp) {
        return;
      }
      void handleRecordingCompleted();
    };
    mediaRecorder.addEventListener("error", () => {
      setError("MediaRecorder encountered an error.");
      setPhase("idle");
    });
    mediaRecorder.start(250);
  };

  onMount(() => {
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
    isCleaningUp = true;
    if (unlistenConversion) {
      void unlistenConversion();
    }
    if (unlistenTranscription) {
      void unlistenTranscription();
    }
    if (unlistenMetadata) {
      void unlistenMetadata();
    }

    if (mediaRecorder && mediaRecorder.state !== "inactive") {
      mediaRecorder.onstop = null;
      mediaRecorder.stop();
    }
    resetRecorderState();
  });

  if (!isSupported()) {
    return (
      <ViewScaffold
        eyebrow="Capture"
        title="Microphone recording"
        description="Capture speech directly from your microphone, monitor a live waveform, then process the recording through ffmpeg + whisper into a library document.">
        <section class="space-y-4 rounded-3xl border border-overlay bg-elevation/85 p-6">
          <p role="alert" class="rounded-xl border border-accent/50 bg-accent/10 p-3 text-sm text-text">
            This environment does not support WebView microphone recording.
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
          onPause={pauseRecording}
          onResume={resumeRecording}
          onStop={stopRecording}
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
