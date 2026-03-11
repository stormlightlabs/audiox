import { For, Match, Show, Switch } from "solid-js";
import { type AppBootstrapResult, type BootPhase, useAppContext } from "../state/AppContext";
import { ViewScaffold } from "./ViewScaffold";

function PhaseBadge(props: { phase: BootPhase }) {
  return (
    <Switch>
      <Match when={props.phase === "ready"}>
        <span class="rounded-full bg-accent/20 px-3 py-1 text-xs font-semibold tracking-wide text-accent uppercase">
          ready
        </span>
      </Match>
      <Match when={props.phase === "loading"}>
        <span class="rounded-full bg-raised px-3 py-1 text-xs font-semibold tracking-wide text-text uppercase">
          loading
        </span>
      </Match>
      <Match when={props.phase === "error"}>
        <span class="rounded-full border border-accent/50 bg-overlay px-3 py-1 text-xs font-semibold tracking-wide text-text uppercase">
          error
        </span>
      </Match>
      <Match when={true}>
        <span class="rounded-full bg-raised px-3 py-1 text-xs font-semibold tracking-wide text-subtext uppercase">
          idle
        </span>
      </Match>
    </Switch>
  );
}

function CreatedDirectories(props: { directories: string[] }) {
  if (props.directories.length === 0) {
    return <span>no new directories</span>;
  }

  return (
    <ul class="flex flex-wrap gap-2">
      <For each={props.directories}>
        {(directory) => (
          <li class="rounded-full border border-overlay bg-raised px-3 py-1 font-mono text-xs text-text">
            {directory}
          </li>
        )}
      </For>
    </ul>
  );
}

function BootstrapDetails(props: { bootstrap: AppBootstrapResult }) {
  return (
    <dl class="grid gap-3 text-sm text-subtext">
      <div class="grid gap-1">
        <dt class="font-semibold text-text">App data directory</dt>
        <dd class="font-mono text-xs">{props.bootstrap.appDataDir}</dd>
      </div>
      <div class="grid gap-1">
        <dt class="font-semibold text-text">Database path</dt>
        <dd class="font-mono text-xs">{props.bootstrap.databasePath}</dd>
      </div>
      <div class="grid gap-1">
        <dt class="font-semibold text-text">Created on first run</dt>
        <dd>
          <CreatedDirectories directories={props.bootstrap.createdDirectories} />
        </dd>
      </div>
    </dl>
  );
}

function ErrorAlert(props: { message: string }) {
  return (
    <p role="alert" class="rounded-xl border border-accent/50 bg-accent/10 p-3 text-sm text-text">{props.message}</p>
  );
}

export function SplashView() {
  const { state, initialize } = useAppContext();

  return (
    <ViewScaffold
      eyebrow="Launch"
      title="Splash and startup checks"
      description="Audio X initializes local storage and the SQLite schema during app startup. This milestone keeps startup checks intentionally simple.">
      <section class="grid gap-5 rounded-3xl border border-overlay bg-elevation/85 p-6">
        <div class="flex items-center justify-between">
          <span class="text-sm font-semibold tracking-wide text-subtext uppercase">Bootstrap status</span>
          <PhaseBadge phase={state.bootPhase} />
        </div>
        <Show when={state.bootstrap}>{(bootstrap) => <BootstrapDetails bootstrap={bootstrap()} />}</Show>
        <Show when={state.bootError}>{(bootError) => <ErrorAlert message={bootError()} />}</Show>
        <div>
          <button
            type="button"
            class="rounded-xl bg-accent px-4 py-2 text-sm font-semibold text-surface transition hover:brightness-110"
            onClick={() => {
              void initialize();
            }}>
            Retry initialization
          </button>
        </div>
      </section>
    </ViewScaffold>
  );
}
