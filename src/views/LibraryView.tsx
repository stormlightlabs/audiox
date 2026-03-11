import { ViewScaffold } from "./ViewScaffold";

export function LibraryView() {
  return (
    <ViewScaffold
      eyebrow="Library"
      title="Document library"
      description="The library shell is ready for indexed transcripts and semantic search results once ingestion pipelines are complete.">
      <section class="rounded-3xl border border-overlay bg-elevation/85 p-6">
        <p class="text-sm text-subtext">Documents will appear here after import and transcription are implemented.</p>
      </section>
    </ViewScaffold>
  );
}
