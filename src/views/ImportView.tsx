import { normalizeError } from "$/errors";
import type { ProgressStatus } from "$/types";
import { useNavigate } from "@solidjs/router";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { readTextFile } from "@tauri-apps/plugin-fs";
import { createSignal, onCleanup, onMount, Show } from "solid-js";
import { ViewScaffold } from "./ViewScaffold";

const IMPORT_CONVERSION_PROGRESS_EVENT = "import://conversion-progress";
const IMPORT_TRANSCRIPTION_PROGRESS_EVENT = "import://transcription-progress";
const IMPORT_METADATA_PROGRESS_EVENT = "import://metadata-progress";
const TEXT_EXTENSIONS = new Set(["txt", "md"]);

type ImportMode = "audio" | "notes";

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
  sourceType: string;
  sourceUri: string;
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

type NotePreview = { sourcePath: string | null; sourceName: string; title: string; content: string };

function ProgressBar(props: { percent: number }) {
  return (
    <div class="h-2 overflow-hidden rounded-full border border-overlay bg-surface/50">
      <div
        class="h-full rounded-full bg-accent/75 transition-[width] duration-200"
        style={{ width: `${Math.max(0, Math.min(100, props.percent))}%` }} />
    </div>
  );
}

function extensionFromName(name: string): string | null {
  const match = /\.([^.]+)$/.exec(name.trim());
  return match ? match[1].toLowerCase() : null;
}

function supportsTextExtension(pathOrFileName: string): boolean {
  const extension = extensionFromName(pathOrFileName);
  return extension !== null && TEXT_EXTENSIONS.has(extension);
}

function fileNameFromPath(path: string): string {
  const normalized = path.replaceAll("\\", "/");
  const segments = normalized.split("/");
  return segments.at(-1) || path;
}

function titleFromFileName(fileName: string): string {
  const withoutExtension = fileName.replace(/\.[^.]+$/u, "").trim();
  if (withoutExtension.length > 0) {
    return withoutExtension;
  }
  return "Imported note";
}

function snippet(content: string): string {
  const trimmed = content.trim();
  if (trimmed.length <= 500) {
    return trimmed;
  }
  return `${trimmed.slice(0, 500)}...`;
}
function PasteNoteInput(
  props: {
    pasteTitle: string;
    pasteContent: string;
    importPastedText: () => void;
    isImporting: boolean;
    updatePasteTitle: (title: string) => void;
    updatePasteContent: (content: string) => void;
  },
) {
  const pasteTitle = () => props.pasteTitle;
  const pasteContent = () => props.pasteContent;
  const importPastedText = () => props.importPastedText;
  const isImporting = () => props.isImporting;

  return (
    <section class="space-y-2 rounded-2xl border border-overlay bg-elevation/60 p-4">
      <p class="text-sm font-semibold text-text">Paste note content</p>
      <label class="grid gap-1 text-xs text-subtext">
        Title (optional)
        <input
          type="text"
          class="rounded-xl border border-overlay bg-elevation/70 px-3 py-2 text-sm text-text outline-none transition focus:border-accent/55"
          value={pasteTitle()}
          onInput={(event) => {
            void props.updatePasteTitle(event.currentTarget.value);
          }} />
      </label>
      <label class="grid gap-1 text-xs text-subtext">
        Content
        <textarea
          rows={7}
          class="rounded-xl border border-overlay bg-elevation/70 px-3 py-2 text-sm text-text outline-none transition focus:border-accent/55"
          value={pasteContent()}
          onInput={(event) => {
            void props.updatePasteContent(event.currentTarget.value);
          }} />
      </label>
      <div class="flex flex-wrap items-center justify-between gap-2">
        <p class="text-xs text-subtext">Preview: {snippet(pasteContent()) || "(empty note)"}</p>
        <button
          type="button"
          class="rounded-xl bg-accent px-3 py-1.5 text-xs font-semibold text-surface transition hover:brightness-110 disabled:cursor-not-allowed disabled:opacity-60"
          onClick={() => {
            void importPastedText();
          }}
          disabled={isImporting()}>
          {isImporting() ? "Processing..." : "Process pasted note"}
        </button>
      </div>
    </section>
  );
}

function NotesHeader(props: { pickTextFile: () => void; isImporting: boolean }) {
  const isImporting = () => props.isImporting;

  return (
    <div class="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
      <div>
        <p class="text-sm font-semibold text-text">Supported note formats</p>
        <p class="text-xs text-subtext">.txt, .md</p>
      </div>
      <button
        type="button"
        class="rounded-xl bg-accent px-4 py-2 text-sm font-semibold text-surface transition hover:brightness-110 disabled:cursor-not-allowed disabled:opacity-60"
        onClick={() => {
          void props.pickTextFile();
        }}
        disabled={isImporting()}>
        Choose note file
      </button>
    </div>
  );
}

function AudioHeader(props: { pickAudioFile: () => void; isImporting: boolean }) {
  const isImporting = () => props.isImporting;

  return (
    <div class="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
      <div>
        <p class="text-sm font-semibold text-text">Supported formats</p>
        <p class="text-xs text-subtext">mp3, m4a, wav, flac, ogg, opus, webm</p>
      </div>
      <button
        type="button"
        class="rounded-xl bg-accent px-4 py-2 text-sm font-semibold text-surface transition hover:brightness-110 disabled:cursor-not-allowed disabled:opacity-60"
        onClick={() => {
          void props.pickAudioFile();
        }}
        disabled={isImporting()}>
        {isImporting() ? "Processing..." : "Choose audio file"}
      </button>
    </div>
  );
}

export function ImportView() {
  const navigate = useNavigate();
  const [error, setError] = createSignal<string | null>(null);
  const [mode, setMode] = createSignal<ImportMode>("audio");
  const [isImporting, setIsImporting] = createSignal(false);
  const [selectedAudioPath, setSelectedAudioPath] = createSignal<string | null>(null);
  const [conversionProgress, setConversionProgress] = createSignal<ConversionProgress | null>(null);
  const [transcriptionProgress, setTranscriptionProgress] = createSignal<TranscriptionProgress | null>(null);
  const [metadataProgress, setMetadataProgress] = createSignal<MetadataProgress | null>(null);
  const [lastImportedDocument, setLastImportedDocument] = createSignal<ImportedDocument | null>(null);
  const [notePreview, setNotePreview] = createSignal<NotePreview | null>(null);
  const [pasteTitle, setPasteTitle] = createSignal("");
  const [pasteContent, setPasteContent] = createSignal("");
  const [isDropActive, setIsDropActive] = createSignal(false);

  const resetProgress = () => {
    setConversionProgress(null);
    setTranscriptionProgress(null);
    setMetadataProgress(null);
    setLastImportedDocument(null);
  };

  const clearNotePreview = () => {
    setError(null);
    setNotePreview(null);
  };

  const importAudio = async (sourcePath: string) => {
    setError(null);
    setIsImporting(true);
    setSelectedAudioPath(sourcePath);
    resetProgress();

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

  const importTextNote = async (preview: NotePreview) => {
    setError(null);
    setIsImporting(true);
    resetProgress();

    try {
      const document = preview.sourcePath
        ? await invoke<ImportedDocument>("import_text_note", { sourcePath: preview.sourcePath })
        : await invoke<ImportedDocument>("import_text_content", { title: preview.title, content: preview.content });
      setLastImportedDocument(document);
      await navigate(`/document/${document.id}`);
    } catch (importError) {
      setError(normalizeError(importError));
    } finally {
      setIsImporting(false);
    }
  };

  const importPastedText = async () => {
    const content = pasteContent().trim();
    if (!content) {
      setError("Paste content must not be empty.");
      return;
    }
    const title = pasteTitle().trim() || "Pasted note";
    await importTextNote({ sourcePath: null, sourceName: "Pasted note", title, content });
  };

  const prepareNotePreviewFromPath = async (sourcePath: string) => {
    const fileName = fileNameFromPath(sourcePath);
    if (!supportsTextExtension(fileName)) {
      setError("Only .txt and .md files are supported for note import.");
      return;
    }

    const content = await readTextFile(sourcePath);
    if (!content.trim()) {
      setError("Selected note is empty.");
      return;
    }

    setError(null);
    setNotePreview({ sourcePath, sourceName: fileName, title: titleFromFileName(fileName), content });
  };

  const handlePickAudioFile = async () => {
    const picked = await open({
      multiple: false,
      directory: false,
      filters: [{ name: "Audio", extensions: ["mp3", "m4a", "wav", "flac", "ogg", "opus", "webm"] }],
    });
    if (typeof picked !== "string" || picked.length === 0) {
      return;
    }
    await importAudio(picked);
  };

  const handlePickTextFile = async () => {
    const picked = await open({
      multiple: false,
      directory: false,
      filters: [{ name: "Notes", extensions: ["txt", "md"] }],
    });
    if (typeof picked !== "string" || picked.length === 0) {
      return;
    }

    try {
      await prepareNotePreviewFromPath(picked);
    } catch (readError) {
      setError(normalizeError(readError));
    }
  };

  const handleDrop = async (event: DragEvent) => {
    event.preventDefault();
    event.stopPropagation();
    setIsDropActive(false);

    const dropped = event.dataTransfer?.files?.item(0);
    if (!dropped) {
      return;
    }

    const droppedWithPath = dropped as File & { path?: string };
    const droppedPath = droppedWithPath.path;
    if (typeof droppedPath === "string" && droppedPath.length > 0) {
      try {
        await prepareNotePreviewFromPath(droppedPath);
      } catch (readError) {
        setError(normalizeError(readError));
      }
      return;
    }

    if (!supportsTextExtension(dropped.name)) {
      setError("Only .txt and .md files are supported for note import.");
      return;
    }

    const content = await dropped.text();
    if (!content.trim()) {
      setError("Dropped note is empty.");
      return;
    }

    setError(null);
    setNotePreview({ sourcePath: null, sourceName: dropped.name, title: titleFromFileName(dropped.name), content });
  };

  onMount(() => {
    let unlistenConversion: UnlistenFn | undefined;
    let unlistenTranscription: UnlistenFn | undefined;
    let unlistenMetadata: UnlistenFn | undefined;

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
    });
  });

  return (
    <ViewScaffold
      eyebrow="Import"
      title="Import audio and notes"
      description="Import local audio or text notes. Note imports skip ffmpeg/whisper and go directly into metadata enrichment and embeddings.">
      <section class="space-y-4 rounded-3xl border border-overlay bg-elevation/85 p-6">
        <div class="flex flex-wrap gap-2">
          <button
            type="button"
            class="rounded-xl border px-3 py-1.5 text-xs font-semibold transition"
            classList={{
              "border-accent/65 bg-accent/15 text-text": mode() === "audio",
              "border-overlay bg-surface/30 text-subtext hover:border-accent/35": mode() !== "audio",
            }}
            onClick={() => {
              setMode("audio");
              setError(null);
              resetProgress();
            }}>
            Audio
          </button>
          <button
            type="button"
            class="rounded-xl border px-3 py-1.5 text-xs font-semibold transition"
            classList={{
              "border-accent/65 bg-accent/15 text-text": mode() === "notes",
              "border-overlay bg-surface/30 text-subtext hover:border-accent/35": mode() !== "notes",
            }}
            onClick={() => {
              setMode("notes");
              setError(null);
              resetProgress();
            }}>
            Notes
          </button>
        </div>

        <Show when={mode() === "audio"}>
          <article class="space-y-4 rounded-2xl border border-overlay bg-surface/35 p-4">
            <AudioHeader pickAudioFile={handlePickAudioFile} isImporting={isImporting()} />

            <Show when={selectedAudioPath()}>
              {(path) => (
                <p class="rounded-xl border border-overlay bg-surface/35 px-3 py-2 text-xs text-subtext">
                  Source: {path()}
                </p>
              )}
            </Show>
          </article>
        </Show>

        <Show when={mode() === "notes"}>
          <article class="space-y-4 rounded-2xl border border-overlay bg-surface/35 p-4">
            <NotesHeader pickTextFile={handlePickTextFile} isImporting={isImporting()} />

            <div
              class="rounded-2xl border border-dashed p-4 text-sm transition"
              classList={{
                "border-accent/65 bg-accent/10": isDropActive(),
                "border-overlay bg-elevation/50": !isDropActive(),
              }}
              onDragOver={(event) => {
                event.preventDefault();
                setIsDropActive(true);
              }}
              onDragLeave={(event) => {
                event.preventDefault();
                setIsDropActive(false);
              }}
              onDrop={(event) => {
                void handleDrop(event);
              }}>
              Drop a `.txt` or `.md` file here to preview before import.
            </div>

            <Show when={notePreview()}>
              {(preview) => (
                <section class="space-y-2 rounded-2xl border border-overlay bg-elevation/60 p-4">
                  <div class="flex flex-wrap items-center justify-between gap-2">
                    <p class="text-sm font-semibold text-text">Preview: {preview().sourceName}</p>
                    <div class="flex flex-wrap items-center gap-2">
                      <button
                        type="button"
                        class="rounded-xl border border-overlay px-3 py-1.5 text-xs font-semibold text-subtext transition hover:border-accent/35 hover:text-text disabled:cursor-not-allowed disabled:opacity-60"
                        onClick={clearNotePreview}
                        disabled={isImporting()}>
                        Remove file
                      </button>
                      <button
                        type="button"
                        class="rounded-xl bg-accent px-3 py-1.5 text-xs font-semibold text-surface transition hover:brightness-110 disabled:cursor-not-allowed disabled:opacity-60"
                        onClick={() => {
                          void importTextNote(preview());
                        }}
                        disabled={isImporting()}>
                        {isImporting() ? "Processing..." : "Process note file"}
                      </button>
                    </div>
                  </div>
                  <p class="text-xs text-subtext whitespace-pre-wrap">{snippet(preview().content) || "(empty note)"}</p>
                </section>
              )}
            </Show>

            <Show when={!notePreview()}>
              <PasteNoteInput
                pasteTitle={pasteTitle()}
                pasteContent={pasteContent()}
                importPastedText={importPastedText}
                isImporting={isImporting()}
                updatePasteTitle={(value) => void setPasteTitle(value)}
                updatePasteContent={(value) => void setPasteContent(value)} />
            </Show>
          </article>
        </Show>

        <Show when={mode() === "audio" && conversionProgress()}>
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

        <Show when={mode() === "audio" && transcriptionProgress()}>
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

        <Show when={metadataProgress()}>
          {(progress) => (
            <article class="space-y-2 rounded-2xl border border-overlay bg-surface/45 p-4">
              <div class="flex items-center justify-between gap-3">
                <p class="text-sm font-semibold text-text">gemma enrichment + embeddings</p>
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
              <p class="mt-1 text-xs text-subtext">{document().segments.length} segments generated.</p>
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
