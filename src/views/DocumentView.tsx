import { ViewScaffold } from "./ViewScaffold";

export function DocumentView() {
  return (
    <ViewScaffold
      eyebrow="Document"
      title="Document reader"
      description="Selected transcript detail, subtitles, and playback sync will be rendered in this view as document features are added.">
      <section class="rounded-3xl border border-overlay bg-elevation/85 p-6">
        <p class="text-sm text-subtext">The reader layout is ready for segment timelines and transcript content.</p>
      </section>
    </ViewScaffold>
  );
}
