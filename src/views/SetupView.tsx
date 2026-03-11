import { ViewScaffold } from "./ViewScaffold";

export function SetupView() {
  return (
    <ViewScaffold
      eyebrow="Setup"
      title="First-run setup wizard"
      description="Preflight routes here when required models are missing. Download and pull flows are coming soon.">
      <section class="rounded-3xl border border-overlay bg-elevation/85 p-6">
        <p class="text-sm text-subtext">
          Missing model dependencies were detected. Guided downloads for whisper and Ollama models are coming soon.
        </p>
      </section>
    </ViewScaffold>
  );
}
