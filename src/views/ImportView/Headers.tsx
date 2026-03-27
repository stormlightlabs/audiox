export function NotesHeader(props: { pickTextFile: () => void; isImporting: boolean }) {
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

export function AudioHeader(props: { pickAudioFile: () => void; isImporting: boolean }) {
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
