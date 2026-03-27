import { formatBytes } from "$/format-utils";
import { For, Show } from "solid-js";
import { WHISPER_LANGUAGE_OPTIONS, WHISPER_MODEL_OPTIONS, WhisperModelInventory } from "./lib";

export function WhisperModelSelector(
  props: { whisperModel: string; onWhisperModelChange: (whisperModel: string) => void },
) {
  const whisperModel = () => props.whisperModel;
  return (
    <label class="grid gap-2">
      <span class="text-xs font-semibold tracking-[0.14em] text-subtext uppercase">Whisper model</span>
      <select
        class="rounded-xl border border-overlay bg-surface/45 px-3 py-2 text-sm text-text focus:border-accent/60 focus:outline-hidden"
        value={whisperModel()}
        onInput={(event) => {
          const value = (event.currentTarget as HTMLSelectElement).value;
          props.onWhisperModelChange(value);
        }}>
        <For each={WHISPER_MODEL_OPTIONS}>{(option) => <option value={option.value}>{option.label}</option>}</For>
        <Show when={!WHISPER_MODEL_OPTIONS.some((option) => option.value === whisperModel())}>
          <option value={whisperModel()}>{whisperModel()}</option>
        </Show>
      </select>
    </label>
  );
}

export function WhisperLanguageSelector(
  props: { whisperLanguage: string; onWhisperLanguageChange: (whisperLanguage: string) => void },
) {
  const whisperLanguage = () => props.whisperLanguage;
  return (
    <label class="grid gap-2">
      <span class="text-xs font-semibold tracking-[0.14em] text-subtext uppercase">Default language</span>
      <input
        list="whisper-language-options"
        class="rounded-xl border border-overlay bg-surface/45 px-3 py-2 text-sm text-text focus:border-accent/60 focus:outline-hidden"
        value={whisperLanguage()}
        onInput={(event) => {
          const value = (event.currentTarget as HTMLInputElement).value;
          props.onWhisperLanguageChange(value);
        }} />
      <datalist id="whisper-language-options">
        <For each={WHISPER_LANGUAGE_OPTIONS}>{(language) => <option value={language} />}</For>
      </datalist>
    </label>
  );
}

export function WhisperThreadsInput(
  props: { whisperThreads: number; onWhisperThreadsChange: (whisperThreads: number) => void },
) {
  const whisperThreads = () => props.whisperThreads;
  return (
    <label class="grid gap-2">
      <span class="text-xs font-semibold tracking-[0.14em] text-subtext uppercase">Whisper threads</span>
      <input
        type="number"
        min="1"
        max="32"
        class="rounded-xl border border-overlay bg-surface/45 px-3 py-2 text-sm text-text focus:border-accent/60 focus:outline-hidden"
        value={whisperThreads()}
        onInput={(event) => {
          const value = Number((event.currentTarget as HTMLInputElement).value);
          if (Number.isFinite(value)) {
            props.onWhisperThreadsChange(value);
          }
        }} />
    </label>
  );
}

export function InstalledWhisperModels(
  props: {
    modelInventory: WhisperModelInventory | null;
    selectedModelName: string;
    deletingModelName: string | null;
    deleteModel: (modelName: string) => void;
    selectedModelInstalled: boolean;
  },
) {
  const modelInventory = () => props.modelInventory;
  const deletingModelName = () => props.deletingModelName;
  const selectedModelInstalled = () => props.selectedModelInstalled;

  return (
    <section class="space-y-3 rounded-2xl border border-overlay bg-surface/30 p-4">
      <div class="flex flex-wrap items-center justify-between gap-2">
        <p class="text-sm font-semibold text-text">Installed whisper models</p>
        <p class="text-xs text-subtext">Disk usage: {formatBytes(modelInventory()?.totalSizeBytes ?? 0)}</p>
      </div>
      <Show
        when={(modelInventory()?.installedModels.length ?? 0) > 0}
        fallback={
          <p class="rounded-xl border border-overlay bg-surface/35 px-3 py-2 text-xs text-subtext">
            No whisper models installed yet.
          </p>
        }>
        <div class="grid gap-2">
          <For each={modelInventory()?.installedModels ?? []}>
            {(model) => (
              <div class="flex items-center justify-between rounded-xl border border-overlay bg-surface/35 px-3 py-2 text-xs text-subtext">
                <div>
                  <p class="text-sm font-semibold text-text">{model.modelName}</p>
                  <p>{model.fileName}</p>
                </div>
                <div class="flex items-center gap-2">
                  <span>{formatBytes(model.sizeBytes)}</span>
                  <button
                    type="button"
                    class="rounded-lg border border-overlay px-2 py-1 text-[11px] font-semibold text-subtext transition hover:border-accent/35 hover:text-text disabled:opacity-60"
                    disabled={deletingModelName() === model.modelName}
                    onClick={() => {
                      void props.deleteModel(model.modelName);
                    }}>
                    {deletingModelName() === model.modelName ? "Deleting..." : "Delete"}
                  </button>
                </div>
              </div>
            )}
          </For>
        </div>
      </Show>
      <p class="text-xs text-subtext">Selected model is {selectedModelInstalled() ? "installed" : "not installed"}.</p>
    </section>
  );
}
