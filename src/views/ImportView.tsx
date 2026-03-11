import { ViewScaffold } from "./ViewScaffold";

export function ImportView() {
  return (
    <ViewScaffold
      eyebrow="Import"
      title="Audio and URL import"
      description="Import routes are in place for local files and web URLs. Download and conversion workflows are introduced in later milestones.">
      <section class="rounded-3xl border border-overlay bg-elevation/85 p-6">
        <p class="text-sm text-subtext">This view will host file picker and URL preview actions.</p>
      </section>
    </ViewScaffold>
  );
}
