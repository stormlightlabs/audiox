import { formatDate, formatDuration } from "$/format-utils";
import { For, Show } from "solid-js";
import { isTextSource, sourceBadgeLabel, sourceIcon } from "./lib";

export function DocumentTitle(props: { sourceType: string; title: string }) {
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

export function DocumentMetadata(props: { sourceType: string; durationSeconds: number | null; createdAt: string }) {
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

export function DocumentTags(props: { tags: string[] }) {
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
