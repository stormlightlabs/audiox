import { useNavigate } from "@solidjs/router";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { createSignal, onCleanup, onMount, Show } from "solid-js";
import { ViewScaffold } from "./ViewScaffold";

const IMPORT_CONVERSION_PROGRESS_EVENT = "import://conversion-progress";
const IMPORT_TRANSCRIPTION_PROGRESS_EVENT = "import://transcription-progress";

type ConversionProgress = {
  status: "running" | "completed" | "error";
  message: string;
  outTimeMs: number;
  totalDurationMs: number | null;
  percent: number;
};

type TranscriptionProgress = {
  status: "running" | "completed" | "error";
  message: string;
  percent: number;
};

type ImportedDocument = {
  id: string;
  title: string;
  transcript: string;
  audioPath: string;
  subtitleSrtPath: string;
  subtitleVttPath: string;
  durationSeconds: number;
  createdAt: string;
  segments: Array<{ startMs: number; endMs: number; text: string }>;
};

function normalizeError(error: unknown): string {
  if (error instanceof Error) {
    return error.message;
  }
  return String(error);
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

export function ImportView() {
  const navigate = useNavigate();
  const [error, setError] = createSignal<string | null>(null);
  const [isImporting, setIsImporting] = createSignal(false);
  const [selectedFilePath, setSelectedFilePath] = createSignal<string | null>(null);
  const [conversionProgress, setConversionProgress] = createSignal<ConversionProgress | null>(null);
  const [transcriptionProgress, setTranscriptionProgress] = createSignal<TranscriptionProgress | null>(null);
  const [lastImportedDocument, setLastImportedDocument] = createSignal<ImportedDocument | null>(null);

  const importAudio = async (sourcePath: string) => {
    setError(null);
    setIsImporting(true);
    setSelectedFilePath(sourcePath);
    setConversionProgress(null);
    setTranscriptionProgress(null);
    setLastImportedDocument(null);

    try {
      const document = await invoke<ImportedDocument>("import_audio_file", { sourcePath });
      setLastImportedDocument(document);
      await navigate(`/document/${document.id}`);
    } catch (importError) {
      setError(normalizeError(importError));
    } finally {
      setIsImporting(false);
    }
  };

  const handlePickFile = async () => {
    const picked = await open({
      multiple: false,
      directory: false,
      filters: [
        {
          name: "Audio",
          extensions: ["mp3", "m4a", "wav", "flac", "ogg", "opus", "webm"],
        },
      ],
    });
    if (typeof picked !== "string" || picked.length === 0) {
      return;
    }
    await importAudio(picked);
  };

  onMount(() => {
    let unlistenConversion: UnlistenFn | undefined;
    let unlistenTranscription: UnlistenFn | undefined;

    void (async () => {
      try {
        unlistenConversion = await listen<ConversionProgress>(IMPORT_CONVERSION_PROGRESS_EVENT, (event) => {
          setConversionProgress(event.payload);
        });
        unlistenTranscription = await listen<TranscriptionProgress>(IMPORT_TRANSCRIPTION_PROGRESS_EVENT, (event) => {
          setTranscriptionProgress(event.payload);
        });
      } catch {
        // Event channels are unavailable in plain browser contexts.
      }
    })();

    onCleanup(() => {
      if (unlistenConversion) {
        void unlistenConversion();
      }
      if (unlistenTranscription) {
        void unlistenTranscription();
      }
    });
  });

  return (
    <ViewScaffold
      eyebrow="Import"
      title="Audio file import"
      description="Select a local audio file and Audio X will copy it into app data, convert it to 16kHz mono WAV, transcribe it with whisper, and store subtitles + transcript in the library.">
      <section class="space-y-4 rounded-3xl border border-overlay bg-elevation/85 p-6">
        <div class="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
          <div>
            <p class="text-sm font-semibold text-text">Supported formats</p>
            <p class="text-xs text-subtext">mp3, m4a, wav, flac, ogg, opus, webm</p>
          </div>
          <button
            type="button"
            class="rounded-xl bg-accent px-4 py-2 text-sm font-semibold text-surface transition hover:brightness-110 disabled:cursor-not-allowed disabled:opacity-60"
            onClick={() => {
              void handlePickFile();
            }}
            disabled={isImporting()}>
            {isImporting() ? "Processing..." : "Choose audio file"}
          </button>
        </div>

        <Show when={selectedFilePath()}>
          {(path) => (
            <p class="rounded-xl border border-overlay bg-surface/35 px-3 py-2 text-xs text-subtext">Source: {path()}</p>
          )}
        </Show>

        <Show when={conversionProgress()}>
          {(progress) => (
            <article class="space-y-2 rounded-2xl border border-overlay bg-surface/45 p-4">
              <div class="flex items-center justify-between gap-3">
                <p class="text-sm font-semibold text-text">ffmpeg conversion</p>
                <span class="text-xs font-semibold tracking-[0.16em] text-subtext uppercase">{progress().status}</span>
              </div>
              <p class="text-xs text-subtext">{progress().message}</p>
              <ProgressBar percent={progress().percent} />
            </article>
          )}
        </Show>

        <Show when={transcriptionProgress()}>
          {(progress) => (
            <article class="space-y-2 rounded-2xl border border-overlay bg-surface/45 p-4">
              <div class="flex items-center justify-between gap-3">
                <p class="text-sm font-semibold text-text">whisper transcription</p>
                <span class="text-xs font-semibold tracking-[0.16em] text-subtext uppercase">{progress().status}</span>
              </div>
              <p class="text-xs text-subtext">{progress().message}</p>
              <ProgressBar percent={progress().percent} />
            </article>
          )}
        </Show>

        <Show when={lastImportedDocument()}>
          {(document) => (
            <article class="rounded-2xl border border-overlay bg-surface/45 p-4">
              <p class="text-sm font-semibold text-text">Imported: {document().title}</p>
              <p class="mt-1 text-xs text-subtext">{document().segments.length} timestamped segments generated.</p>
            </article>
          )}
        </Show>

        <Show when={error()}>
          {(message) => (
            <p role="alert" class="rounded-xl border border-accent/50 bg-accent/10 p-3 text-sm text-text">
              {message()}
            </p>
          )}
        </Show>
      </section>
    </ViewScaffold>
  );
}
