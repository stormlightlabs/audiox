# Frontend Architecture

## Stack

- **SolidJS** — reactive UI framework
- **Tailwind CSS v4** — utility-first styling (via `@tailwindcss/vite`)
- **solid-motionone** — animations (page transitions, list animations, recording pulse)
- **@tauri-apps/api** — IPC bridge to Rust backend

## Views

| View         | Purpose                                                                 |
| ------------ | ----------------------------------------------------------------------- |
| **Splash**   | Preflight checks with animated checklist, transitions to main or setup  |
| **Setup**    | First-run wizard: download whisper model, check/pull Ollama models      |
| **Record**   | Microphone recording with live waveform, stop → transcribe flow         |
| **Import**   | Drag-and-drop, file picker, or URL paste for audio/video import         |
| **Library**  | Grid/list of all documents with search bar, sort, filter by tags        |
| **Document** | Full transcript viewer with timestamps, subtitles, metadata, edit       |
| **Settings** | Whisper model selection, Ollama endpoint config, audio device selection |

## State Management

Use SolidJS `createStore` for app-wide state via Context:

```typescript
type AppState = {
  documents: DocumentMeta[]; // library listing (no full transcript)
  activeDocument: Document | null; // currently viewed document
  recording: {
    active: boolean;
    duration: number;
  };
  preflight: {
    status: "running" | "passed" | "failed";
    checks: Record<string, CheckStatus>;
  };
  setup: {
    whisperReady: boolean;
    embeddingReady: boolean;
    ollamaReady: boolean;
    modelsReady: boolean;
  };
  search: {
    query: string;
    results: SearchResult[];
  };
  urlImport: {
    loading: boolean;
    meta: YtDlpMeta | null; // preview card data
    progress: number; // download percentage
  };
};
```

## Tauri IPC Commands

Exposed Rust commands called from the frontend via `invoke()`:

| Command                  | Args                | Returns           | Description                                   |
| ------------------------ | ------------------- | ----------------- | --------------------------------------------- |
| `preflight`              | —                   | `PreflightResult` | Run all startup checks                        |
| `check_setup`            | —                   | `SetupStatus`     | Check whisper model + Ollama status           |
| `download_whisper_model` | `model_name`        | stream events     | Download whisper model to appdata             |
| `pull_ollama_model`      | `model_name`        | stream events     | Pull Ollama model with progress               |
| `save_audio`             | `audio_bytes`       | `audio_path`      | Save recorded audio to appdata                |
| `convert_audio`          | `input_path`        | `wav_path`        | ffmpeg convert to 16kHz mono WAV              |
| `transcribe`             | `audio_path, model` | `Transcript`      | Run whisper-cli sidecar                       |
| `generate_subtitles`     | `audio_path, model` | `SubtitlePaths`   | Run whisper-cli with -osrt -ovtt              |
| `burn_subtitles`         | `video, srt`        | `output_path`     | ffmpeg subtitle burn-in                       |
| `fetch_url_meta`         | `url`               | `YtDlpMeta`       | yt-dlp --dump-json (no download)              |
| `download_url`           | `url, audio_only`   | stream events     | yt-dlp download with progress                 |
| `process_document`       | `transcript`        | `Document`        | Generate title, summary, keywords, embeddings |
| `list_documents`         | `sort, filter`      | `DocumentMeta[]`  | List library documents                        |
| `get_document`           | `id`                | `Document`        | Full document with transcript                 |
| `delete_document`        | `id`                | —                 | Remove document and audio                     |
| `search`                 | `query`             | `SearchResult[]`  | Semantic search via local fastembed           |
| `update_document`        | `id, fields`        | `Document`        | Edit title, tags, etc.                        |
