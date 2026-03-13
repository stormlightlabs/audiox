// TODO: Use a markdown library for parsing and rendering
import { normalizeError } from "$/errors";
import { formatDate, formatDuration } from "$/format-utils";
import { A } from "@solidjs/router";
import { invoke } from "@tauri-apps/api/core";
import { createEffect, createMemo, createSignal, For, onCleanup, onMount, Show } from "solid-js";
import { Motion } from "solid-motionone";
import { ViewScaffold } from "./ViewScaffold";

type DocumentSummary = {
  id: string;
  sourceType: string;
  title: string;
  summary: string | null;
  tags: string[];
  durationSeconds: number | null;
  createdAt: string;
  updatedAt: string;
};

type SourceFilter = "all" | "audio" | "recording" | "text";

type DocumentSort = "created_desc" | "created_asc" | "title_asc" | "title_desc" | "duration_asc" | "duration_desc";

type SearchResult = {
  documentId: string;
  documentTitle: string;
  documentSummary: string | null;
  documentTags: string[];
  chunkIndex: number;
  chunkContent: string;
  similarity: number;
  segmentStartMs: number | null;
  segmentEndMs: number | null;
};

const sortOptions: Array<{ value: DocumentSort; label: string }> = [
  { value: "created_desc", label: "Newest first" },
  { value: "created_asc", label: "Oldest first" },
  { value: "title_asc", label: "Title A-Z" },
  { value: "title_desc", label: "Title Z-A" },
  { value: "duration_asc", label: "Duration short-long" },
  { value: "duration_desc", label: "Duration long-short" },
];

const sourceFilterOptions: Array<{ value: SourceFilter; label: string }> = [
  { value: "all", label: "All" },
  { value: "audio", label: "Audio" },
  { value: "recording", label: "Recording" },
  { value: "text", label: "Text Note" },
];

function sourceBadgeLabel(sourceType: string): string {
  if (sourceType === "microphone_recording") {
    return "Recording";
  }
  if (sourceType === "text_note") {
    return "Text Note";
  }
  if (sourceType === "text_paste") {
    return "Pasted Note";
  }
  return "Audio";
}

function sourceIcon(sourceType: string): string {
  if (sourceType === "microphone_recording") {
    return "i-bi-mic-fill";
  }
  if (sourceType === "text_note" || sourceType === "text_paste") {
    return "i-bi-file-text-fill";
  }
  return "i-bi-file-music-fill";
}

function isTextSource(sourceType: string): boolean {
  return sourceType === "text_note" || sourceType === "text_paste";
}

function matchesSourceFilter(document: DocumentSummary, filter: SourceFilter): boolean {
  if (filter === "all") {
    return true;
  }
  if (filter === "audio") {
    return document.sourceType === "file_import";
  }
  if (filter === "recording") {
    return document.sourceType === "microphone_recording";
  }
  return isTextSource(document.sourceType);
}

function escapeForRegex(value: string): string {
  return value.replaceAll(/[.*+?^${}()|[\]\\]/g, String.raw`\$&`);
}

function queryTerms(query: string): string[] {
  const deduped = new Set<string>();
  for (const rawPart of query.split(/\s+/u)) {
    const part = rawPart.trim().toLowerCase();
    if (part.length >= 2) {
      deduped.add(part);
    }
  }
  return [...deduped];
}

function renderHighlightedChunk(content: string, query: string) {
  const terms = queryTerms(query);
  if (terms.length === 0) {
    return content;
  }

  const pattern = new RegExp(`(${terms.map((term) => escapeForRegex(term)).join("|")})`, "giu");
  const chunks = content.split(pattern).filter((chunk) => chunk.length > 0);
  return chunks.map((chunk) => {
    const lower = chunk.toLowerCase();
    const matches = terms.includes(lower);
    return matches ? <mark class="rounded-sm bg-accent/30 px-0.5 text-text">{chunk}</mark> : chunk;
  });
}

function SortControl(props: { sort: DocumentSort; update: (sort: DocumentSort) => void }) {
  const sort = () => props.sort;
  return (
    <label class="grid gap-1 text-xs text-subtext">
      Sort
      <select
        class="rounded-xl border border-overlay bg-elevation/70 px-3 py-2 text-sm text-text outline-none transition focus:border-accent/55"
        value={sort()}
        onInput={(event) => {
          const value = event.currentTarget.value as DocumentSort;
          props.update(value);
        }}>
        <For each={sortOptions}>{(option) => <option value={option.value}>{option.label}</option>}</For>
      </select>
    </label>
  );
}

function NoMatchesFound() {
  return (
    <p class="rounded-xl border border-overlay bg-elevation/65 p-3 text-sm text-subtext">
      No matching chunks found for this query.
    </p>
  );
}

function EmptyState() {
  return (
    <section class="rounded-2xl border border-dashed border-overlay bg-surface/35 p-6">
      <p class="text-base font-semibold text-text">No matching documents yet</p>
      <p class="mt-1 text-sm text-subtext">
        Import audio, record from your microphone, or import a text note to create your first document.
      </p>
      <div class="mt-4 flex flex-wrap gap-2">
        <A
          href="/import"
          class="rounded-xl bg-accent px-3 py-1.5 text-xs font-semibold text-surface transition hover:brightness-110">
          Open Import
        </A>
        <A
          href="/record"
          class="rounded-xl border border-overlay px-3 py-1.5 text-xs font-semibold text-subtext transition hover:border-accent/35 hover:text-text">
          Open Record
        </A>
      </div>
    </section>
  );
}

function DocumentTags(props: { tags: string[] }) {
  const tags = () => props.tags;
  return (
    <Show when={tags().length > 0}>
      <div class="mt-2 flex flex-wrap gap-1.5">
        <For each={tags()}>
          {(tag) => (
            <span class="rounded-full border border-overlay px-2 py-0.5 text-[10px] font-semibold text-subtext">
              {tag}
            </span>
          )}
        </For>
      </div>
    </Show>
  );
}

function DocumentTitle(props: { sourceType: string; title: string }) {
  const sourceType = () => props.sourceType;
  const title = () => props.title;
  return (
    <div class="flex flex-wrap items-center gap-2">
      <span class="flex items-center text-subtext">
        <i class={`${sourceIcon(sourceType())} size-4`} />
      </span>
      <p class="text-base font-semibold text-text">{title() || "Untitled document"}</p>
    </div>
  );
}

function DocumentMetadata(props: { sourceType: string; durationSeconds: number | null; createdAt: string }) {
  const sourceType = () => props.sourceType;
  const duration = () => formatDuration(props.durationSeconds);
  const createdAt = () => formatDate(props.createdAt);

  return (
    <div class="mt-2 flex flex-wrap items-center gap-2 text-[11px] text-subtext">
      <span class="rounded-full border border-overlay px-2 py-0.5">{sourceBadgeLabel(sourceType())}</span>
      <Show when={!isTextSource(sourceType())}>
        <span>{duration()}</span>
      </Show>
      <span>{createdAt()}</span>
    </div>
  );
}

function SearchResultsHeader(props: { isSearching: boolean }) {
  const isSearching = () => props.isSearching;
  return (
    <div class="flex items-center justify-between gap-3">
      <p class="text-sm font-semibold text-text">Semantic search results</p>
      <Show when={isSearching()}>
        <span class="text-xs font-semibold tracking-[0.16em] text-subtext uppercase">Searching</span>
      </Show>
    </div>
  );
}

export function LibraryView() {
  const [documents, setDocuments] = createSignal<DocumentSummary[]>([]);
  const [isLoading, setIsLoading] = createSignal(true);
  const [error, setError] = createSignal<string | null>(null);
  const [sort, setSort] = createSignal<DocumentSort>("created_desc");
  const [selectedTags, setSelectedTags] = createSignal<string[]>([]);
  const [sourceFilter, setSourceFilter] = createSignal<SourceFilter>("all");
  const [searchQuery, setSearchQuery] = createSignal("");
  const [searchResults, setSearchResults] = createSignal<SearchResult[]>([]);
  const [isSearching, setIsSearching] = createSignal(false);
  const [searchError, setSearchError] = createSignal<string | null>(null);
  const isEmpty = createMemo(() => !isLoading() && visibleDocuments().length === 0 && activeSearchQuery().length === 0);

  // eslint-disable-next-line no-unassigned-vars
  let searchInputRef: HTMLInputElement | undefined;

  let searchRequestCounter = 0;

  const activeSearchQuery = createMemo(() => searchQuery().trim());
  const availableTags = createMemo(() => {
    const unique = new Map<string, string>();
    for (const document of documents()) {
      for (const tag of document.tags) {
        const key = tag.toLowerCase();
        if (!unique.has(key)) {
          unique.set(key, tag);
        }
      }
    }
    return [...unique.values()].toSorted((left, right) => left.localeCompare(right));
  });
  const visibleDocuments = createMemo(() =>
    documents().filter((document) => matchesSourceFilter(document, sourceFilter()))
  );

  const loadDocuments = async (sortValue: DocumentSort, tags: string[]) => {
    setIsLoading(true);
    setError(null);
    try {
      const items = await invoke<DocumentSummary[]>("list_documents", { sort: sortValue, filterTags: tags });
      setDocuments(Array.isArray(items) ? items : []);
    } catch (loadError) {
      setError(normalizeError(loadError));
    } finally {
      setIsLoading(false);
    }
  };

  const runSearch = async (query: string) => {
    const requestId = ++searchRequestCounter;
    setIsSearching(true);
    setSearchError(null);
    try {
      const results = await invoke<SearchResult[]>("search", { query, limit: 12 });
      if (requestId === searchRequestCounter) {
        setSearchResults(Array.isArray(results) ? results : []);
      }
    } catch (searchFailure) {
      if (requestId === searchRequestCounter) {
        setSearchError(normalizeError(searchFailure));
      }
    } finally {
      if (requestId === searchRequestCounter) {
        setIsSearching(false);
      }
    }
  };

  const refresh = async () => {
    await loadDocuments(sort(), selectedTags());
    const query = activeSearchQuery();
    if (query.length > 0) {
      await runSearch(query);
    }
  };

  const toggleTagFilter = (tag: string) => {
    const lower = tag.toLowerCase();
    const current = selectedTags();
    if (current.some((candidate) => candidate.toLowerCase() === lower)) {
      setSelectedTags(current.filter((candidate) => candidate.toLowerCase() !== lower));
      return;
    }
    setSelectedTags([...current, tag]);
  };

  const removeDocument = async (documentId: string, title: string) => {
    const confirmed = globalThis.confirm(
      `Delete "${title}"? This removes transcript, embeddings, audio, and subtitle files from the library.`,
    );
    if (!confirmed) {
      return;
    }

    try {
      await invoke("delete_document", { id: documentId });
      await refresh();
    } catch (deleteError) {
      setError(normalizeError(deleteError));
    }
  };

  createEffect(() => {
    void loadDocuments(sort(), selectedTags());
  });

  createEffect(() => {
    const query = activeSearchQuery();
    if (query.length === 0) {
      searchRequestCounter += 1;
      setIsSearching(false);
      setSearchError(null);
      setSearchResults([]);
      return;
    }

    const timeoutId = globalThis.setTimeout(() => {
      void runSearch(query);
    }, 260);

    onCleanup(() => {
      globalThis.clearTimeout(timeoutId);
    });
  });

  const focusSearchHandler = () => {
    searchInputRef?.focus();
    searchInputRef?.select();
  };

  onMount(() => {
    globalThis.addEventListener("audiox:focus-library-search", focusSearchHandler);
    onCleanup(() => {
      globalThis.removeEventListener("audiox:focus-library-search", focusSearchHandler);
    });
  });

  return (
    <ViewScaffold
      eyebrow="Library"
      title="Document library"
      description="Browse processed transcripts, filter by tags, delete old records, and run semantic search across all embedded chunks.">
      <section class="space-y-5 rounded-3xl border border-overlay bg-elevation/85 p-6">
        <div class="grid gap-3 lg:grid-cols-[1.5fr_220px_auto] lg:items-center">
          <label class="grid gap-1 text-xs text-subtext">
            Semantic search
            <input
              type="search"
              placeholder="Ask a question about your transcripts"
              class="rounded-xl border border-overlay bg-elevation/70 px-3 py-2 text-sm text-text outline-none transition focus:border-accent/55"
              ref={searchInputRef}
              value={searchQuery()}
              onInput={(event) => {
                setSearchQuery(event.currentTarget.value);
              }} />
          </label>

          <SortControl sort={sort()} update={(value) => setSort(value)} />

          <button
            type="button"
            class="rounded-xl border border-overlay px-3 py-2 text-xs font-semibold text-subtext transition hover:border-accent/35 hover:text-text disabled:opacity-60"
            onClick={() => {
              void refresh();
            }}
            disabled={isLoading() || isSearching()}>
            {isLoading() || isSearching() ? "Refreshing..." : "Refresh"}
          </button>
        </div>

        <div class="flex flex-wrap items-center gap-2">
          <span class="text-xs text-subtext">Source:</span>
          <For each={sourceFilterOptions}>
            {(option) => (
              <button
                type="button"
                class="rounded-full border px-2.5 py-1 text-[11px] font-semibold transition"
                classList={{
                  "border-accent/60 bg-accent/15 text-text": sourceFilter() === option.value,
                  "border-overlay bg-surface/30 text-subtext hover:border-accent/35": sourceFilter() !== option.value,
                }}
                onClick={() => {
                  setSourceFilter(option.value);
                }}>
                {option.label}
              </button>
            )}
          </For>
        </div>

        <div class="flex flex-wrap items-center gap-2">
          <span class="text-xs text-subtext">Filter tags:</span>
          <Show
            when={availableTags().length > 0}
            fallback={<span class="text-xs text-subtext">No tags available yet.</span>}>
            <For each={availableTags()}>
              {(tag) => {
                const active = () => selectedTags().some((candidate) => candidate.toLowerCase() === tag.toLowerCase());
                return (
                  <button
                    type="button"
                    class="rounded-full border px-2.5 py-1 text-[11px] font-semibold transition"
                    classList={{
                      "border-accent/60 bg-accent/15 text-text": active(),
                      "border-overlay bg-surface/30 text-subtext hover:border-accent/35": !active(),
                    }}
                    onClick={() => {
                      toggleTagFilter(tag);
                    }}>
                    {tag}
                  </button>
                );
              }}
            </For>
          </Show>
          <Show when={selectedTags().length > 0}>
            <button
              type="button"
              class="rounded-full border border-overlay px-2.5 py-1 text-[11px] font-semibold text-subtext transition hover:border-accent/35"
              onClick={() => {
                setSelectedTags([]);
              }}>
              Clear filters
            </button>
          </Show>
        </div>

        <div class="flex items-center justify-between gap-3">
          <p class="text-sm text-subtext">{visibleDocuments().length} documents</p>
          <Show when={activeSearchQuery().length > 0}>
            <p class="text-xs text-subtext">{searchResults().length} ranked matches</p>
          </Show>
        </div>

        <Show when={error()}>
          {(message) => (
            <p role="alert" class="rounded-xl border border-accent/50 bg-accent/10 p-3 text-sm text-text">
              {message()}
            </p>
          )}
        </Show>

        <Show when={isEmpty()}>
          <EmptyState />
        </Show>

        <Show when={activeSearchQuery().length > 0}>
          <section class="space-y-3 rounded-2xl border border-overlay bg-surface/35 p-4">
            <SearchResultsHeader isSearching={isSearching()} />

            <Show
              when={searchError()}
              fallback={
                <Show when={!isSearching() && searchResults().length === 0}>
                  <NoMatchesFound />
                </Show>
              }>
              {(message) => (
                <p role="alert" class="rounded-xl border border-accent/50 bg-accent/10 p-3 text-sm text-text">
                  {message()}
                </p>
              )}
            </Show>

            <For each={searchResults()}>
              {(result, index) => (
                <Motion.article
                  initial={{ opacity: 0, y: 14 }}
                  animate={{ opacity: 1, y: 0 }}
                  transition={{ duration: 0.22, delay: index() * 0.02 }}
                  class="rounded-2xl border border-overlay bg-elevation/70 p-4">
                  <div class="flex flex-wrap items-center justify-between gap-2">
                    <A
                      href={`/document/${result.documentId}${
                        result.segmentStartMs !== null
                          ? `?segment=${result.segmentStartMs}&q=${encodeURIComponent(activeSearchQuery())}`
                          : `?q=${encodeURIComponent(activeSearchQuery())}`
                      }`}
                      class="text-sm font-semibold text-text transition hover:text-accent">
                      {result.documentTitle}
                    </A>
                    <span class="rounded-full border border-overlay px-2 py-0.5 text-[11px] font-semibold text-subtext">
                      score {result.similarity.toFixed(3)}
                    </span>
                  </div>
                  <p class="mt-2 text-sm leading-relaxed text-text">
                    {renderHighlightedChunk(result.chunkContent, activeSearchQuery())}
                  </p>
                  <div class="mt-2 flex flex-wrap items-center gap-2 text-[11px] text-subtext">
                    <span>Chunk #{result.chunkIndex + 1}</span>
                    <Show when={result.segmentStartMs !== null}>
                      <span>Jump to {Math.max(0, Math.floor((result.segmentStartMs ?? 0) / 1000))}s</span>
                    </Show>
                  </div>
                </Motion.article>
              )}
            </For>
          </section>
        </Show>

        <Show
          when={!isLoading()}
          fallback={
            <div class="grid gap-3">
              <For each={[0, 1, 2]}>
                {(item) => (
                  <article
                    aria-hidden="true"
                    class="animate-pulse rounded-2xl border border-overlay bg-surface/30 p-4"
                    data-index={item}>
                    <div class="h-4 w-1/3 rounded bg-overlay/70" />
                    <div class="mt-3 h-3 w-11/12 rounded bg-overlay/60" />
                    <div class="mt-2 h-3 w-7/12 rounded bg-overlay/60" />
                    <div class="mt-3 h-2 w-5/12 rounded bg-overlay/50" />
                  </article>
                )}
              </For>
            </div>
          }>
          <For each={visibleDocuments()}>
            {(document, index) => (
              <Motion.article
                initial={{ opacity: 0, y: 16 }}
                animate={{ opacity: 1, y: 0 }}
                transition={{ duration: 0.22, delay: index() * 0.02 }}
                class="rounded-2xl border border-overlay bg-surface/35 p-4">
                <div class="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
                  <A href={`/document/${document.id}`} class="block flex-1 transition hover:opacity-95">
                    <DocumentTitle sourceType={document.sourceType} title={document.title} />
                    <p class="mt-1 text-xs text-subtext">{document.summary || "No summary generated."}</p>

                    <DocumentMetadata
                      sourceType={document.sourceType}
                      durationSeconds={document.durationSeconds}
                      createdAt={document.createdAt} />

                    <DocumentTags tags={document.tags} />
                  </A>

                  <button
                    type="button"
                    class="rounded-xl border border-red-400/60 bg-red-500/10 px-3 py-1.5 text-xs font-semibold text-red-200 transition hover:border-red-300"
                    onClick={() => {
                      void removeDocument(document.id, document.title || "Untitled transcript");
                    }}>
                    Delete
                  </button>
                </div>
              </Motion.article>
            )}
          </For>
        </Show>
      </section>
    </ViewScaffold>
  );
}
