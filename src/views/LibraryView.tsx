import { A } from "@solidjs/router";
import { invoke } from "@tauri-apps/api/core";
import { createSignal, For, onMount, Show } from "solid-js";
import { ViewScaffold } from "./ViewScaffold";

type DocumentSummary = {
  id: string;
  title: string;
  summary: string | null;
  durationSeconds: number | null;
  createdAt: string;
  updatedAt: string;
};

function normalizeError(error: unknown): string {
  if (error instanceof Error) {
    return error.message;
  }
  return String(error);
}

function formatDuration(seconds: number | null): string {
  if (!seconds || seconds <= 0) {
    return "Unknown duration";
  }
  const minutes = Math.floor(seconds / 60);
  const remainingSeconds = seconds % 60;
  return `${minutes}:${String(remainingSeconds).padStart(2, "0")}`;
}

export function LibraryView() {
  const [documents, setDocuments] = createSignal<DocumentSummary[]>([]);
  const [isLoading, setIsLoading] = createSignal(true);
  const [error, setError] = createSignal<string | null>(null);

  const loadDocuments = async () => {
    setIsLoading(true);
    setError(null);
    try {
      const items = await invoke<DocumentSummary[]>("list_documents");
      setDocuments(Array.isArray(items) ? items : []);
    } catch (loadError) {
      setError(normalizeError(loadError));
    } finally {
      setIsLoading(false);
    }
  };

  onMount(() => {
    void loadDocuments();
  });

  return (
    <ViewScaffold
      eyebrow="Library"
      title="Document library"
      description="Imported and transcribed documents are indexed here. Select any entry to open the full transcript with timestamped segments.">
      <section class="space-y-4 rounded-3xl border border-overlay bg-elevation/85 p-6">
        <div class="flex items-center justify-between gap-3">
          <p class="text-sm text-subtext">{documents().length} documents</p>
          <button
            type="button"
            class="rounded-xl border border-overlay px-3 py-1.5 text-xs font-semibold text-subtext transition hover:border-accent/35 hover:text-text disabled:opacity-60"
            onClick={() => {
              void loadDocuments();
            }}
            disabled={isLoading()}>
            {isLoading() ? "Refreshing..." : "Refresh"}
          </button>
        </div>

        <Show when={error()}>
          {(message) => (
            <p role="alert" class="rounded-xl border border-accent/50 bg-accent/10 p-3 text-sm text-text">
              {message()}
            </p>
          )}
        </Show>

        <Show when={!isLoading() && documents().length === 0}>
          <p class="rounded-xl border border-overlay bg-surface/40 p-4 text-sm text-subtext">
            No transcripts yet. Go to Import to process your first audio file.
          </p>
        </Show>

        <div class="grid gap-3">
          <For each={documents()}>
            {(document) => (
              <A
                href={`/document/${document.id}`}
                class="rounded-2xl border border-overlay bg-surface/35 p-4 transition hover:border-accent/40 hover:bg-surface/55">
                <div class="flex flex-col gap-2 sm:flex-row sm:items-start sm:justify-between">
                  <div>
                    <p class="text-base font-semibold text-text">{document.title || "Untitled transcript"}</p>
                    <p class="mt-1 text-xs text-subtext">{document.summary || "Raw transcript only."}</p>
                  </div>
                  <p class="text-xs text-subtext">{formatDuration(document.durationSeconds)}</p>
                </div>
              </A>
            )}
          </For>
        </div>
      </section>
    </ViewScaffold>
  );
}
