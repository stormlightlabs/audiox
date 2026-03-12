import { normalizeError } from "$/errors";
import { formatDate, formatDuration, formatTimestamp } from "$/format-utils";
import { useParams, useSearchParams } from "@solidjs/router";
import { invoke } from "@tauri-apps/api/core";
import { createEffect, createSignal, For, onCleanup, Show } from "solid-js";
import { ViewScaffold } from "./ViewScaffold";

type TranscriptSegment = { startMs: number; endMs: number; text: string };

type DocumentDetail = {
  id: string;
  title: string;
  summary: string | null;
  tags: string[];
  transcript: string;
  audioPath: string | null;
  subtitleSrtPath: string | null;
  subtitleVttPath: string | null;
  durationSeconds: number | null;
  createdAt: string;
  updatedAt: string;
  segments: TranscriptSegment[];
};

function parseTagsInput(raw: string): string[] {
  const deduped = new Set<string>();
  for (const chunk of raw.split(",")) {
    const value = chunk.trim();
    if (value.length > 0) {
      deduped.add(value);
    }
  }
  return [...deduped];
}

function tagsInput(tags: string[]): string {
  return tags.join(", ");
}

function tagsEqual(left: string[], right: string[]): boolean {
  if (left.length !== right.length) {
    return false;
  }

  for (const [index, value] of left.entries()) {
    if (value !== right[index]) {
      return false;
    }
  }
  return true;
}

function segmentDomKey(segment: TranscriptSegment): string {
  return `${segment.startMs}-${segment.endMs}`;
}

function segmentForTarget(segments: TranscriptSegment[], targetMs: number): TranscriptSegment | null {
  if (segments.length === 0) {
    return null;
  }

  const exact = segments.find((segment) => targetMs >= segment.startMs && targetMs <= segment.endMs);
  if (exact) {
    return exact;
  }

  let nearest = segments[0];
  let distance = Math.abs(segments[0].startMs - targetMs);
  for (const segment of segments.slice(1)) {
    const nextDistance = Math.abs(segment.startMs - targetMs);
    if (nextDistance < distance) {
      nearest = segment;
      distance = nextDistance;
    }
  }
  return nearest;
}

function LoadingSkeleton() {
  return (
    <div class="grid gap-3" aria-hidden="true">
      <div class="animate-pulse rounded-2xl border border-overlay bg-surface/35 p-4">
        <div class="h-4 w-1/4 rounded bg-overlay/70" />
        <div class="mt-3 h-3 w-10/12 rounded bg-overlay/60" />
        <div class="mt-2 h-3 w-8/12 rounded bg-overlay/60" />
      </div>
      <div class="animate-pulse rounded-2xl border border-overlay bg-surface/35 p-4">
        <div class="h-3 w-1/3 rounded bg-overlay/60" />
        <div class="mt-2 h-3 w-11/12 rounded bg-overlay/60" />
        <div class="mt-2 h-3 w-9/12 rounded bg-overlay/60" />
      </div>
    </div>
  );
}

export function DocumentView() {
  const params = useParams<{ id?: string }>();
  const [searchParams] = useSearchParams<{ segment?: string; q?: string }>();
  const [document, setDocument] = createSignal<DocumentDetail | null>(null);
  const [isLoading, setIsLoading] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [draftTitle, setDraftTitle] = createSignal("");
  const [draftTags, setDraftTags] = createSignal("");
  const [isSaving, setIsSaving] = createSignal(false);
  const [saveMessage, setSaveMessage] = createSignal<string | null>(null);
  const [saveError, setSaveError] = createSignal<string | null>(null);
  const [focusedSegmentKey, setFocusedSegmentKey] = createSignal<string | null>(null);

  const segmentElements = new Map<string, HTMLDivElement>();
  let focusTimer: number | undefined;

  createEffect(() => {
    const id = params.id;
    if (!id) {
      setDocument(null);
      setError(null);
      setIsLoading(false);
      setSaveMessage(null);
      setSaveError(null);
      setFocusedSegmentKey(null);
      return;
    }

    void (async () => {
      setIsLoading(true);
      setError(null);
      setSaveMessage(null);
      setSaveError(null);
      try {
        const result = await invoke<DocumentDetail>("get_document", { id });
        setDocument(result);
      } catch (loadError) {
        setError(normalizeError(loadError));
      } finally {
        setIsLoading(false);
      }
    })();
  });

  createEffect(() => {
    const currentDocument = document();
    if (!currentDocument) {
      setDraftTitle("");
      setDraftTags("");
      setSaveError(null);
      segmentElements.clear();
      return;
    }

    segmentElements.clear();
    setDraftTitle(currentDocument.title);
    setDraftTags(tagsInput(currentDocument.tags));
    setSaveError(null);
  });

  createEffect(() => {
    const currentDocument = document();
    const rawSegment = searchParams.segment;
    if (!currentDocument || !rawSegment) {
      return;
    }

    const targetMs = Number(rawSegment);
    if (!Number.isFinite(targetMs)) {
      return;
    }

    const targetSegment = segmentForTarget(currentDocument.segments, targetMs);
    if (!targetSegment) {
      return;
    }

    const key = segmentDomKey(targetSegment);
    setFocusedSegmentKey(key);

    const frame = globalThis.requestAnimationFrame(() => {
      segmentElements.get(key)?.scrollIntoView({ behavior: "smooth", block: "center" });
    });

    if (focusTimer !== undefined) {
      globalThis.clearTimeout(focusTimer);
    }
    focusTimer = globalThis.setTimeout(() => {
      setFocusedSegmentKey(null);
    }, 2800);

    onCleanup(() => {
      globalThis.cancelAnimationFrame(frame);
    });
  });

  onCleanup(() => {
    if (focusTimer !== undefined) {
      globalThis.clearTimeout(focusTimer);
    }
  });

  const hasUnsavedChanges = () => {
    const currentDocument = document();
    if (!currentDocument) {
      return false;
    }

    const nextTitle = draftTitle().trim();
    const nextTags = parseTagsInput(draftTags());
    return nextTitle !== currentDocument.title || !tagsEqual(nextTags, currentDocument.tags);
  };

  const saveMetadata = async () => {
    const currentDocument = document();
    if (!currentDocument) {
      return;
    }

    const nextTitle = draftTitle().trim();
    if (!nextTitle) {
      setSaveError("Title must not be empty.");
      setSaveMessage(null);
      return;
    }

    setIsSaving(true);
    setSaveError(null);
    setSaveMessage(null);
    try {
      const updated = await invoke<DocumentDetail>("update_document", {
        id: currentDocument.id,
        title: nextTitle,
        tags: parseTagsInput(draftTags()),
      });
      setDocument(updated);
      setSaveMessage("Saved document metadata.");
    } catch (saveMetadataError) {
      setSaveError(normalizeError(saveMetadataError));
    } finally {
      setIsSaving(false);
    }
  };

  return (
    <ViewScaffold
      eyebrow="Document"
      title={document()?.title ?? "Document reader"}
      description="Review transcript output, AI-generated metadata, and update title/tags as needed.">
      <section class="space-y-4 rounded-3xl border border-overlay bg-elevation/85 p-6">
        <Show when={!params.id}>
          <p class="rounded-xl border border-overlay bg-surface/40 p-4 text-sm text-subtext">
            Select a document from the library to view transcript details.
          </p>
        </Show>

        <Show when={searchParams.q}>
          {(query) => (
            <p class="rounded-xl border border-overlay bg-surface/40 p-3 text-xs text-subtext">
              Search context: {query()}
            </p>
          )}
        </Show>

        <Show when={isLoading()}>
          <LoadingSkeleton />
        </Show>

        <Show when={error()}>
          {(message) => (
            <p role="alert" class="rounded-xl border border-accent/50 bg-accent/10 p-3 text-sm text-text">
              {message()}
            </p>
          )}
        </Show>

        <Show when={document()}>
          {(currentDocument) => (
            <>
              <article class="space-y-3 rounded-2xl border border-overlay bg-surface/35 p-4">
                <p class="text-sm font-semibold text-text">Metadata</p>
                <div class="flex flex-wrap items-center gap-2 text-[11px] text-subtext">
                  <span class="rounded-full border border-overlay px-2 py-0.5">
                    {formatDuration(currentDocument().durationSeconds)}
                  </span>
                  <span class="rounded-full border border-overlay px-2 py-0.5">
                    {currentDocument().segments.length} segments
                  </span>
                  <span class="rounded-full border border-overlay px-2 py-0.5">
                    Created {formatDate(currentDocument().createdAt)}
                  </span>
                </div>
                <label class="grid gap-1 text-xs text-subtext">
                  Title
                  <input
                    type="text"
                    class="rounded-xl border border-overlay bg-elevation/70 px-3 py-2 text-sm text-text outline-none transition focus:border-accent/55"
                    value={draftTitle()}
                    onInput={(event) => {
                      setDraftTitle(event.currentTarget.value);
                    }} />
                </label>
                <label class="grid gap-1 text-xs text-subtext">
                  Tags (comma-separated)
                  <input
                    type="text"
                    class="rounded-xl border border-overlay bg-elevation/70 px-3 py-2 text-sm text-text outline-none transition focus:border-accent/55"
                    value={draftTags()}
                    onInput={(event) => {
                      setDraftTags(event.currentTarget.value);
                    }} />
                </label>
                <p class="text-xs text-subtext">
                  Summary: {currentDocument().summary?.trim() || "No summary generated."}
                </p>

                <Show when={currentDocument().tags.length > 0}>
                  <div class="flex flex-wrap gap-2">
                    <For each={currentDocument().tags}>
                      {(tag) => (
                        <span class="rounded-full border border-overlay bg-elevation/75 px-2.5 py-1 text-[11px] font-semibold text-subtext">
                          {tag}
                        </span>
                      )}
                    </For>
                  </div>
                </Show>

                <div class="flex flex-wrap items-center gap-3">
                  <button
                    type="button"
                    class="rounded-xl bg-accent px-3 py-1.5 text-xs font-semibold text-surface transition hover:brightness-110 disabled:cursor-not-allowed disabled:opacity-60"
                    onClick={() => {
                      void saveMetadata();
                    }}
                    disabled={!hasUnsavedChanges() || isSaving()}>
                    {isSaving() ? "Saving..." : "Save title/tags"}
                  </button>
                  <Show when={saveMessage()}>{(message) => <span class="text-xs text-subtext">{message()}</span>}</Show>
                </div>

                <Show when={saveError()}>
                  {(message) => (
                    <p role="alert" class="rounded-xl border border-accent/50 bg-accent/10 p-3 text-sm text-text">
                      {message()}
                    </p>
                  )}
                </Show>
              </article>

              <article class="rounded-2xl border border-overlay bg-surface/35 p-4">
                <p class="text-xs text-subtext">Audio path: {currentDocument().audioPath ?? "N/A"}</p>
                <p class="mt-1 text-xs text-subtext">SRT: {currentDocument().subtitleSrtPath ?? "N/A"}</p>
                <p class="mt-1 text-xs text-subtext">VTT: {currentDocument().subtitleVttPath ?? "N/A"}</p>
                <p class="mt-1 text-xs text-subtext">Updated: {formatDate(currentDocument().updatedAt)}</p>
              </article>

              <article class="rounded-2xl border border-overlay bg-surface/35 p-4">
                <p class="mb-3 text-sm font-semibold text-text">Transcript</p>
                <Show
                  when={currentDocument().segments.length > 0}
                  fallback={<p class="text-sm text-subtext">{currentDocument().transcript}</p>}>
                  <div class="grid gap-2">
                    <For each={currentDocument().segments}>
                      {(segment) => {
                        const key = segmentDomKey(segment);
                        return (
                          <div
                            ref={(element) => {
                              segmentElements.set(key, element);
                            }}
                            class="rounded-xl border border-overlay/80 bg-elevation/70 px-3 py-2 transition"
                            classList={{ "!border-accent/70 ring-2 ring-accent/40": focusedSegmentKey() === key }}>
                            <p class="text-[11px] font-semibold tracking-[0.14em] text-subtext uppercase">
                              {formatTimestamp(segment.startMs)} - {formatTimestamp(segment.endMs)}
                            </p>
                            <p class="mt-1 text-sm text-text">{segment.text}</p>
                          </div>
                        );
                      }}
                    </For>
                  </div>
                </Show>
              </article>
            </>
          )}
        </Show>
      </section>
    </ViewScaffold>
  );
}
