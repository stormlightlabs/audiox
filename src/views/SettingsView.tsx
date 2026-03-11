import { ViewScaffold } from "./ViewScaffold";

export function SettingsView() {
  return (
    <ViewScaffold
      eyebrow="Settings"
      title="System configuration"
      description="Preferences for models, Ollama, and app defaults are routed and ready for future implementation.">
      <section class="rounded-3xl border border-overlay bg-elevation/85 p-6">
        <p class="text-sm text-subtext">Settings controls will be added as model management features ship.</p>
      </section>
    </ViewScaffold>
  );
}
