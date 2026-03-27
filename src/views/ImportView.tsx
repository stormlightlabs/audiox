import { ProgressBar } from "$/components/ProgressBar";
import { normalizeError } from "$/errors";
import type { ProgressStatus } from "$/types";
import { useNavigate } from "@solidjs/router";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { readTextFile } from "@tauri-apps/plugin-fs";
import * as logger from "@tauri-apps/plugin-log";
import { createSignal, onCleanup, onMount, Show } from "solid-js";
import { TranscriptSegment } from "./DocumentView/lib";
import { AudioHeader, NotesHeader } from "./ImportView/Headers";
import {
  IMPORT_CONVERSION_PROGRESS_EVENT,
  IMPORT_METADATA_PROGRESS_EVENT,
  IMPORT_TRANSCRIPTION_PROGRESS_EVENT,
  snippet,
  TEXT_EXTENSIONS,
} from "./ImportView/lib";
import type { NotePreview } from "./ImportView/lib";
import { PasteNoteInput } from "./ImportView/PasteNoteInput";
import { ViewScaffold } from "./ViewScaffold";

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
  segments: TranscriptSegment[];
};

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
      } catch (error) {
        logger.debug(
          "Event listeners are unavailable, progress updates will not work. This is expected if running in a browser context.",
          { keyValues: { error: normalizeError(error) } },
        );
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

            <PasteNoteInput
              preview={notePreview()}
              pasteTitle={pasteTitle()}
              pasteContent={pasteContent()}
              importPastedText={importPastedText}
              isImporting={isImporting()}
              updatePasteTitle={(value) => void setPasteTitle(value)}
              updatePasteContent={(value) => void setPasteContent(value)} />
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
                <p class="text-sm font-semibold text-text">metadata enrichment + embeddings</p>
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
