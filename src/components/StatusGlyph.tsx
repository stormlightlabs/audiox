import { StepStatus } from "$/views/SetupView/lib";
import { Match, Switch } from "solid-js";

export function StatusGlyph(props: { status: StepStatus }) {
  return (
    <Switch>
      <Match when={props.status === "pass"}>
        <span class="text-accent">✓</span>
      </Match>
      <Match when={props.status === "running"}>
        <span class="inline-block size-4 rounded-full border-2 border-accent/40 border-t-accent align-middle animate-spin" />
      </Match>
      <Match when={props.status === "fail"}>
        <span class="text-text">✕</span>
      </Match>
      <Match when={props.status === "blocked"}>
        <span class="text-subtext">⏸</span>
      </Match>
      <Match when={props.status === "warn"}>
        <span class="text-subtext">⚠</span>
      </Match>
      <Match when={props.status === "pending"}>
        <span class="text-subtext">•</span>
      </Match>
    </Switch>
  );
}
