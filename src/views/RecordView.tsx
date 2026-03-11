import { ViewScaffold } from "./ViewScaffold";

export function RecordView() {
  return (
    <ViewScaffold
      eyebrow="Capture"
      title="Microphone recording"
      description="The recording flow is scaffolded so navigation is ready before implementing live capture and transcription.">
      <section class="rounded-3xl border border-overlay bg-elevation/85 p-6">
        <p class="text-sm text-subtext">Recording controls and waveform visualization are coming soon.</p>
      </section>
    </ViewScaffold>
  );
}
