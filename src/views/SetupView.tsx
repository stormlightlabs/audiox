import { ViewScaffold } from "./ViewScaffold";

export function SetupView() {
  return (
    <ViewScaffold
      eyebrow="Setup"
      title="First-run setup wizard"
      description="Model downloads and dependency management land in milestone two and three. This placeholder confirms routing and shell integration.">
      <section class="rounded-3xl border border-overlay bg-elevation/85 p-6">
        <p class="text-sm text-subtext">
          Setup steps will appear here once preflight checks begin tracking missing dependencies.
        </p>
      </section>
    </ViewScaffold>
  );
}
