import { formatTimestamp } from "$/format-utils";
import { For, Show } from "solid-js";
import { segmentDomKey, TranscriptSegment } from "./lib";

export function DocumentSegments(
  props: {
    segments: TranscriptSegment[];
    transcript: string;
    segmentElements: Map<string, HTMLElement>;
    focusedSegmentKey: string | null;
  },
) {
  const segmentElements = () => props.segmentElements;
  const focusedSegmentKey = () => props.focusedSegmentKey;
  const segments = () => props.segments;
  const transcript = () => props.transcript;

  return (
    <Show when={segments().length > 0} fallback={<p class="text-sm text-subtext">{transcript()}</p>}>
      <div class="grid gap-2">
        <For each={segments()}>
          {(segment) => {
            const key = segmentDomKey(segment);
            return (
              <div
                ref={(element) => {
                  segmentElements().set(key, element);
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
  );
}
