import { Show } from "solid-js";
import { NotePreview, snippet } from "./lib";

export function PasteNoteInput(
  props: {
    pasteTitle: string;
    pasteContent: string;
    isImporting: boolean;
    preview: NotePreview | null;
    importPastedText: () => void;
    updatePasteTitle: (title: string) => void;
    updatePasteContent: (content: string) => void;
  },
) {
  const pasteTitle = () => props.pasteTitle;
  const pasteContent = () => props.pasteContent;
  const importPastedText = () => props.importPastedText;
  const isImporting = () => props.isImporting;
  const preview = () => props.preview;

  return (
    <Show when={!preview()}>
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
    </Show>
  );
}
