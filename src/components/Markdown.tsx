import { normalizeError } from "$/errors";
import { invoke } from "@tauri-apps/api/core";
import { createMemo, createResource, createSignal, For, Match, Show, Switch } from "solid-js";

const DEFAULT_MARKDOWN_THEME = "zenburn";

const MARKDOWN_THEME_STORAGE_KEY = "audiox.markdown.theme";

type MarkdownRenderResult = { html: string; theme: string };

type MarkdownProps = { content: string; class?: string; defaultTheme?: string; showThemePicker?: boolean };

function readPersistedTheme(defaultTheme?: string): string {
  if (globalThis.window === undefined) {
    return defaultTheme ?? DEFAULT_MARKDOWN_THEME;
  }

  const persisted = globalThis.localStorage.getItem(MARKDOWN_THEME_STORAGE_KEY)?.trim();
  return persisted || defaultTheme || DEFAULT_MARKDOWN_THEME;
}

// TODO: this should be saved in sqlite not localStorage
function writePersistedTheme(theme: string) {
  if (globalThis.window === undefined) {
    return;
  }

  globalThis.localStorage.setItem(MARKDOWN_THEME_STORAGE_KEY, theme);
}

function themeLabel(theme: string): string {
  return theme.split("-").map((part) => part.charAt(0).toUpperCase() + part.slice(1)).join(" ");
}

function ThemeSelector(props: { selectedTheme: string; availableThemes: string[]; update: (theme: string) => void }) {
  const selectedTheme = () => props.selectedTheme;
  const availableThemes = () => props.availableThemes;
  return (
    <label class="grid gap-1 text-xs text-subtext">
      <span class="sr-only">Syntax theme</span>
      <select
        aria-label="Syntax theme"
        class="rounded-xl border border-overlay bg-surface/70 px-3 py-2 text-sm text-text outline-none transition focus:border-accent/55"
        value={selectedTheme()}
        onInput={(event) => {
          const nextTheme = event.currentTarget.value;
          void props.update(nextTheme);
        }}>
        <For each={availableThemes()} fallback={<option value={selectedTheme()}>{themeLabel(selectedTheme())}</option>}>
          {(theme) => <option value={theme}>{themeLabel(theme)}</option>}
        </For>
      </select>
    </label>
  );
}

function LoadingMarkdown() {
  return (
    <p class="flex items-center gap-2 text-sm text-subtext">
      <span>Rendering markdown...</span>

      <span class="flex items-center">
        <i class="i-bi-circle animate-pulse" />
      </span>
    </p>
  );
}

export function Markdown(props: MarkdownProps) {
  const [selectedTheme, setSelectedTheme] = createSignal(readPersistedTheme(props.defaultTheme));
  const [availableThemes] = createResource(() => invoke<string[]>("list_markdown_themes"));
  const [rendered] = createResource(
    () => ({ content: props.content, theme: selectedTheme() }),
    async ({ content, theme }) => {
      if (!content.trim()) {
        return { html: "", theme };
      }

      return invoke<MarkdownRenderResult>("render_markdown", { content, theme });
    },
  );

  const bodyClass = createMemo(() => ["markdown-body", props.class].filter(Boolean).join(" "));
  const shouldShowThemePicker = createMemo(() =>
    props.showThemePicker !== false && (availableThemes()?.length ?? 0) > 1
  );
  const innerHtml = createMemo(() => rendered()?.html ?? "");

  function handleThemeChange(theme: string) {
    const themes = availableThemes();
    if (!themes?.length) {
      return;
    }

    if (!themes.includes(theme)) {
      setSelectedTheme(themes.includes(DEFAULT_MARKDOWN_THEME) ? DEFAULT_MARKDOWN_THEME : themes[0]!);
    }

    setSelectedTheme(theme);
    writePersistedTheme(theme);
  }

  return (
    <section class="grid gap-3">
      <Show when={shouldShowThemePicker()}>
        <div class="flex flex-wrap items-center justify-between gap-3 rounded-2xl border border-overlay bg-elevation/60 px-3 py-2">
          <div>
            <p class="text-[11px] font-semibold tracking-[0.16em] text-subtext uppercase">Markdown theme</p>
            <p class="text-xs text-subtext">Syntect highlights fenced code with embedded tmTheme palettes.</p>
          </div>

          <ThemeSelector
            selectedTheme={selectedTheme()}
            availableThemes={availableThemes() ?? []}
            update={handleThemeChange} />
        </div>
      </Show>

      <Switch fallback={<div class={bodyClass()} innerHTML={innerHtml()} />}>
        <Match when={rendered.loading}>
          <LoadingMarkdown />
        </Match>
        <Match when={rendered.error}>
          <p role="alert" class="rounded-xl border border-accent/50 bg-accent/10 p-3 text-sm text-text">
            {normalizeError(rendered.error)}
          </p>
        </Match>
      </Switch>
    </section>
  );
}
