# Audio X — Technical Specification

## Overview

Audio X is a Tauri 2 desktop app (SolidJS + Tailwind v4 frontend, Rust backend) that transcribes audio to text using whisper.cpp and builds a searchable document library from transcripts using Ollama-hosted Gemma models. It supports local audio files, microphone recording, and URL-based media import via yt-dlp.

## Architecture

```text
┌───────────────────────────────────────────────────────┐
│  SolidJS Frontend (WebView)                           │
│  ┌────────┐ ┌────────┐ ┌──────────┐ ┌──────────────┐  │
│  │ Splash │ │Recorder│ │ Library  │ │Search/Viewer │  │
│  └───┬────┘ └───┬────┘ └────┬─────┘ └───────┬──────┘  │
│      │ invoke() │ invoke()  │ invoke()      │         │
├──────┼──────────┼───────────┼───────────────┼─────────┤
│  Rust Backend (Tauri)                                 │
│  ┌───────────┐ ┌──────────┐ ┌──────────────────────┐  │
│  │ Preflight │ │ Doc Mgr  │ │ Search Engine        │  │
│  └─────┬─────┘ └─────┬────┘ └───────────┬──────────┘  │
│        │             │                  │             │
│  ┌─────▼─────────────▼──────────────────▼───────────┐ │
│  │      Sidecars + Optional Runtime Binaries        │ │
│  │  ┌──────────┐  ┌────────┐  ┌──────┐              │ │
│  │  │whisper   │  │ yt-dlp │  │ffmpeg│              │ │
│  │  │  -cli    │  │ (opt)  │  │      │              │ │
│  │  └──────────┘  └────────┘  └──────┘              │ │
│  └──────────────────────────────────────────────────┘ │
│                                                       │
│  ┌──────────────────────────────────────────────────┐ │
│  │ Ollama HTTP Client (embed + generate)            │ │
│  └──────────────────────────────────────────────────┘ │
│  ┌──────────────────────────────────────────────────┐ │
│  │ SQLite (documents, embeddings, metadata)         │ │
│  └──────────────────────────────────────────────────┘ │
└───────────────────────────────────────────────────────┘
```

## Data Directory Layout

All runtime data lives under the Tauri `appDataDir` (`~/Library/Application Support/org.stormlightlabs.audiox/` on macOS):

```sh
appdata/
  models/
    ggml-base.en.bin          # whisper model (downloaded on first run)
  audio/
    <uuid>.wav                # recorded/imported audio files
  video/
    <uuid>.mp4                # downloaded videos (yt-dlp)
  subtitles/
    <uuid>.srt                # generated subtitle files
    <uuid>.vtt
  bin/
    yt-dlp/<version>/         # optional runtime-managed yt-dlp executable
  db/
    audiox.db                 # SQLite database
```

## 1. Preflight Checks & Splash Screen

On every launch the app shows a splash screen while running preflight checks. The splash is the first thing the user sees — it validates that all runtime dependencies are available before showing the main UI.

### Check Sequence

1. **Executable dependencies** — ensure `whisper-cli`, `ffmpeg`, and `yt-dlp` are executable:
   - sidecar → managed runtime cache → PATH
2. **Whisper model** — check that at least one model file exists in `appdata/models/`
3. **Ollama server** — `GET http://localhost:11434/api/tags`, timeout 3s
4. **Ollama models** — parse `/api/tags` response, confirm `nomic-embed-text` and `gemma3:4b` are present
5. **Database** — open or create `appdata/db/audiox.db`, run migrations if schema version is stale

### Status Reporting

Each check reports one of three states to the frontend via Tauri events:

| State  | Meaning                                                       |
| ------ | ------------------------------------------------------------- |
| `pass` | Dependency is ready                                           |
| `fail` | Missing or broken — show actionable guidance                  |
| `warn` | Optional dependency missing (e.g., yt-dlp) — app can continue |

### Splash UI

- App logo + name centered
- Animated checklist (solid-motionone staggered entrance) showing each check with a spinner → checkmark/cross
- If all pass: auto-transition to Library view after a short delay
- If any fail: remain on splash, show inline guidance (e.g., "Ollama is not running. Start it with `ollama serve` or install from ollama.com") with a retry button
- First-run scenario: if whisper model or Ollama models are missing, transition to the Setup wizard (M2) instead

### Tauri Command

```rust
#[tauri::command]
async fn preflight(app: tauri::AppHandle) -> Result<PreflightResult, String> {
    // Returns status for each dependency
}
```

```typescript
type PreflightResult = {
  whisper_cli: CheckStatus; // sidecar-first executable check
  ffmpeg: CheckStatus; // sidecar-first executable check
  yt_dlp: CheckStatus; // optional executable check (warn if missing)
  whisper_model: CheckStatus; // model file
  ollama_server: CheckStatus; // server reachable
  ollama_models: CheckStatus; // required models present
  database: CheckStatus; // db accessible
}
type CheckStatus = "pass" | "fail" | "warn";
```

## 2. whisper.cpp Integration

### Distribution Strategy

Bundle `whisper-cli` as a **Tauri sidecar** (`bundle.externalBin`) for production builds.

- Preflight resolves in order: sidecar → managed runtime cache → `PATH`
- Runtime download is not required for `whisper-cli` in end-user flows
- Sidecar strategy keeps onboarding simple (no first-run binary install prompts)

### Model Management

- **Default model:** `ggml-base.en.bin` (142 MB) — good accuracy/speed tradeoff
- **Download source:** `https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-{name}.bin`
- **First-run flow:** Check if model file exists at `appdata/models/`. If missing, download with progress reporting via Tauri events.
- **Optional models:** Allow user to download larger models (small: 466 MB, medium: 1.5 GB) from settings. Store selection in config.

### Transcription Pipeline

**Input requirements:** whisper.cpp requires 16-bit PCM WAV, 16kHz, mono. The bundled ffmpeg sidecar handles format conversion (see §4).

**Rust command flow:**

1. Convert input audio to required format via ffmpeg sidecar (see §4)
2. Spawn whisper-cli sidecar:

   ```sh
   whisper-cli -m <model_path> -f <audio_path> -oj -l auto -t 4 -pp
   ```

3. Stream `stderr` for progress (`-pp` flag prints progress percentage)
4. Parse JSON output from `stdout` on completion
5. Return structured transcript with timestamps

**Output format (whisper JSON):**

```json
{
  "transcription": [
    {
      "timestamps": { "from": "00:00:00,000", "to": "00:00:05,230" },
      "offsets": { "from": 0, "to": 5230 },
      "text": " Hello, this is a recording."
    }
  ]
}
```

### Subtitle Generation

whisper-cli natively generates subtitle files alongside JSON:

```sh
# Generate SRT + VTT alongside the transcript
whisper-cli -m <model_path> -f <audio_path> -oj -osrt -ovtt -of <output_base_path>
```

- `-osrt` → `<output_base_path>.srt`
- `-ovtt` → `<output_base_path>.vtt`

Generated subtitle files are saved to `appdata/subtitles/<document_uuid>.*`.

### Audio Recording

Use the **Web Audio API / MediaRecorder** in the frontend WebView to capture microphone input. This avoids native plugin dependencies and works cross-platform in Tauri's WebView.

Flow:

1. `navigator.mediaDevices.getUserMedia({ audio: true })` → MediaStream
2. `MediaRecorder` with `audio/webm;codecs=opus` (or wav via AudioWorklet)
3. On stop, send blob to Rust backend via IPC
4. Rust passes to ffmpeg sidecar for conversion to 16kHz mono WAV, saves to `appdata/audio/`
5. Trigger transcription pipeline

## 3. yt-dlp Integration

### Distribution Strategy

Bundle `yt-dlp` as a sidecar as well, but keep it optional at the feature level (URL import).

| Platform        | Binary                                             | Size   |
| --------------- | -------------------------------------------------- | ------ |
| macOS universal | `yt-dlp_macos` → `yt-dlp-aarch64-apple-darwin`     | ~21 MB |
| Linux x86_64    | `yt-dlp_linux` → `yt-dlp-x86_64-unknown-linux-gnu` | ~21 MB |
| Windows x64     | `yt-dlp.exe` → `yt-dlp-x86_64-pc-windows-msvc.exe` | ~21 MB |

yt-dlp requires ffmpeg for audio extraction/conversion. The app points `--ffmpeg-location` to the bundled ffmpeg sidecar when present, otherwise uses system `PATH`.

### URL Import Flow

1. **User pastes a URL** (YouTube, podcast, any yt-dlp-supported site)
2. **Fetch metadata** (no download):

   ```sh
   yt-dlp --dump-json --no-playlist "<url>"
   ```

   Returns JSON to stdout with: `title`, `duration`, `thumbnail`, `uploader`, `description`, `webpage_url`, etc. Display a preview card in the UI.

3. **User confirms** → start download
4. **Download audio-only:**

   ```sh
   yt-dlp -x --audio-format wav --audio-quality 0 \
     --no-playlist --newline \
     --ffmpeg-location <bundled_ffmpeg_dir> \
     -o "<appdata>/audio/<uuid>.%(ext)s" "<url>"
   ```

5. **Download video (if user requests subtitled video export):**

   ```sh
   yt-dlp -f "bestvideo[ext=mp4]+bestaudio[ext=m4a]/best[ext=mp4]/best" \
     --no-playlist --newline \
     --ffmpeg-location <bundled_ffmpeg_dir> \
     -o "<appdata>/video/<uuid>.%(ext)s" "<url>"
   ```

6. **Parse progress** from stderr (each line with `--newline` flag):

   ```text
   [download]   5.2% of 45.30MiB at 2.50MiB/s ETA 00:17
   ```

   Parse percentage, speed, and ETA via regex for UI progress bar.

7. **Post-download:** feed the audio WAV into the transcription pipeline (§2). Store yt-dlp metadata (source URL, original title, uploader) on the document record.

### Metadata Storage

yt-dlp metadata is stored on the document record:

```sql
-- Added to documents table
source_url  TEXT,              -- original URL
source_meta TEXT,              -- yt-dlp JSON metadata (title, uploader, etc.)
```

### Supported Sites

yt-dlp supports 1000+ sites (YouTube, Vimeo, SoundCloud, Bandcamp, Twitter/X, Reddit, podcast RSS feeds, etc.). No special handling per-site needed — yt-dlp normalizes the interface.

## 4. ffmpeg Integration

### Distribution Strategy

Bundle ffmpeg as a **Tauri sidecar** (`bundle.externalBin`) for production builds.

- Preflight resolves in order: sidecar → managed runtime cache → `PATH`
- Runtime download is not required for `ffmpeg` in end-user flows

Suggested static build sources by platform:

| Platform    | Source                                                                                                                 | Approx. Size |
| ----------- | ---------------------------------------------------------------------------------------------------------------------- | ------------ |
| macOS arm64 | [evermeet.cx](https://evermeet.cx/ffmpeg/) or [shaka-project](https://github.com/shaka-project/static-ffmpeg-binaries) | ~43 MB       |
| macOS x64   | evermeet.cx                                                                                                            | ~45 MB       |
| Linux x64   | [johnvansickle.com](https://johnvansickle.com/ffmpeg/)                                                                 | ~50 MB       |
| Windows x64 | [gyan.dev](https://www.gyan.dev/ffmpeg/builds/)                                                                        | ~50 MB       |

### Roles in the Pipeline

ffmpeg is the universal audio/video format glue. It serves three roles:

**1. Audio Format Conversion (pre-transcription)**:

All audio must become 16kHz mono 16-bit PCM WAV before whisper.cpp can process it. ffmpeg handles any input format:

```sh
ffmpeg -i <input> -ar 16000 -ac 1 -c:a pcm_s16le -y <output.wav>
```

This replaces the `hound` crate for conversion — ffmpeg handles mp3, m4a, ogg, opus, flac, webm, and any video container.

**2. Audio Extraction from Video**:

When a video is downloaded via yt-dlp (or imported directly), extract audio for transcription:

```sh
ffmpeg -i <video.mp4> -vn -ar 16000 -ac 1 -c:a pcm_s16le -y <output.wav>
```

- `-vn` — discard video stream

**3. Subtitle Burn-in (video export)**:

After generating subtitles from a transcript, burn them into a video file for shareable export:

```sh
ffmpeg -i <video.mp4> -vf "subtitles=<subs.srt>" -c:a copy -y <output.mp4>
```

The `subtitles` filter requires libass (included in standard static builds). Custom styling can be applied via the `force_style` parameter.

### Progress Reporting

Use `-progress pipe:1` to get machine-readable progress on stdout:

```sh
ffmpeg -i <input> -ar 16000 -ac 1 -c:a pcm_s16le -progress pipe:1 -y <output.wav>
```

Output (key=value pairs, repeated every ~500ms):

```text
out_time=00:00:04.096000
speed=8.19x
progress=continue
```

Get total duration first via:

```sh
ffmpeg -i <input> 2>&1 | grep "Duration"
# or parse from yt-dlp metadata
```

Compare `out_time` against total duration for percentage. `progress=end` signals completion.

## 5. Ollama Integration

### Prerequisites

Ollama must be installed and running on `http://localhost:11434`. The app detects Ollama status during preflight (§1) and guides the user if it's missing.

**Health check:** `GET http://localhost:11434/api/tags` — confirms server is running and returns installed models.

### Required Models

| Purpose        | Model              | Pull Command                   | Dimensions |
| -------------- | ------------------ | ------------------------------ | ---------- |
| Embeddings     | `nomic-embed-text` | `ollama pull nomic-embed-text` | 768        |
| Text transform | `gemma3:4b`        | `ollama pull gemma3:4b`        | —          |

**Why nomic-embed-text:** Outperforms OpenAI ada-002, fast on Apple Silicon (~9k tokens/sec on M2 Max), 8192 token context, 768 dimensions (compact for local SQLite storage).

**Why gemma3:4b:** Multimodal, 128K context window, runs well on 8GB+ RAM with QAT quantization. Suitable for summarization, title generation, and keyword extraction.

### Model Setup Flow

On first launch (or when models are missing):

1. Check `GET /api/tags` for installed models
2. For each missing model, call `POST /api/pull` with streaming progress
3. Display download progress in the UI (Ollama handles the actual download)
4. Mark setup complete when both models respond successfully

### Embedding Pipeline

For each transcript segment (or chunk of ~512 tokens):

```text
POST http://localhost:11434/api/embed
{
  "model": "nomic-embed-text",
  "input": ["segment text here"]
}
→ { "embeddings": [[0.123, -0.456, ...]] }  // 768-dim vector
```

Store embeddings as binary blobs in SQLite alongside the source text chunk and document reference.

### Document Transformation Pipeline

After transcription completes, use Gemma to generate structured metadata:

**1. Title Generation:**

```text
POST http://localhost:11434/api/generate
{
  "model": "gemma3:4b",
  "prompt": "Generate a concise title for this transcript:\n\n<transcript_text>\n\nTitle:",
  "stream": false
}
```

**2. Summary Generation:**

```text
POST http://localhost:11434/api/generate
{
  "model": "gemma3:4b",
  "prompt": "Write a 2-3 sentence summary of this transcript:\n\n<transcript_text>\n\nSummary:",
  "stream": false
}
```

**3. Keyword/Tag Extraction:**

```text
POST http://localhost:11434/api/generate
{
  "model": "gemma3:4b",
  "prompt": "Extract 3-7 keywords from this transcript as a comma-separated list:\n\n<transcript_text>\n\nKeywords:",
  "stream": false
}
```

## 6. Data Model (SQLite)

Use `rusqlite` in the Rust backend. Database stored at `appdata/db/audiox.db`.

### Schema

```sql
CREATE TABLE documents (
  id          TEXT PRIMARY KEY,       -- UUID
  title       TEXT NOT NULL,
  summary     TEXT,
  keywords    TEXT,                   -- comma-separated
  transcript  TEXT NOT NULL,          -- full text
  segments    TEXT NOT NULL,          -- JSON array of timestamped segments
  audio_path  TEXT,                   -- relative path to audio file
  video_path  TEXT,                   -- relative path to video file (if from yt-dlp)
  subtitle_path TEXT,                 -- relative path to .srt/.vtt file
  source_url  TEXT,                   -- original URL (if from yt-dlp)
  source_meta TEXT,                   -- yt-dlp JSON metadata
  duration_ms INTEGER,
  model_used  TEXT,                   -- whisper model name
  created_at  TEXT NOT NULL,          -- ISO 8601
  updated_at  TEXT NOT NULL
);

CREATE TABLE chunks (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
  chunk_index INTEGER NOT NULL,
  content     TEXT NOT NULL,          -- text chunk (~512 tokens)
  embedding   BLOB NOT NULL,         -- 768 floats as f32 binary (3072 bytes)
  UNIQUE(document_id, chunk_index)
);

CREATE INDEX idx_chunks_document ON chunks(document_id);
CREATE INDEX idx_documents_created ON documents(created_at);

CREATE TABLE settings (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
);
```

### Semantic Search

Search is cosine similarity over chunk embeddings:

1. Embed the query: `POST /api/embed { model: "nomic-embed-text", input: [query] }`
2. Compute cosine similarity between query embedding and all chunk embeddings in SQLite (Rust-side, in-memory scan)
3. Return top-K chunks with their parent document references
4. For small-medium libraries (< 10k chunks), brute-force cosine similarity in Rust is fast enough. No vector DB needed.

## 7. Frontend Architecture

### Tech Stack

- **SolidJS** — reactive UI framework
- **Tailwind CSS v4** — utility-first styling (via `@tailwindcss/vite`)
- **solid-motionone** — animations (page transitions, list animations, recording pulse)
- **@tauri-apps/api** — IPC bridge to Rust backend

### Views

| View         | Purpose                                                                 |
| ------------ | ----------------------------------------------------------------------- |
| **Splash**   | Preflight checks with animated checklist, transitions to main or setup  |
| **Setup**    | First-run wizard: download whisper model, check/pull Ollama models      |
| **Record**   | Microphone recording with live waveform, stop → transcribe flow         |
| **Import**   | Drag-and-drop, file picker, or URL paste for audio/video import         |
| **Library**  | Grid/list of all documents with search bar, sort, filter by tags        |
| **Document** | Full transcript viewer with timestamps, subtitles, metadata, edit       |
| **Settings** | Whisper model selection, Ollama endpoint config, audio device selection |

### State Management

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
}
```

### Tauri IPC Commands

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
| `search`                 | `query`             | `SearchResult[]`  | Semantic search over embeddings               |
| `update_document`        | `id, fields`        | `Document`        | Edit title, tags, etc.                        |

### Sidecar + Runtime Configuration

Bundled sidecars (configured in `tauri.conf.json`):

```text
bundle.externalBin:
  binaries/whisper-cli
  binaries/ffmpeg
  binaries/yt-dlp
```

Optional runtime cache:

```text
appdata/bin/
  yt-dlp/<version>/yt-dlp(.exe)
```

Preflight resolution strategy:

1. Check bundled sidecar candidates
2. Check managed runtime cache (if present)
3. Check system `PATH`
4. If still missing, report actionable guidance (reinstall app for users, `setup.sh` for developers)

### Tauri Plugins Required

| Plugin      | Crate                 | JS Package                  | Purpose                       |
| ----------- | --------------------- | --------------------------- | ----------------------------- |
| Shell       | `tauri-plugin-shell`  | `@tauri-apps/plugin-shell`  | Execute external tools safely |
| File System | `tauri-plugin-fs`     | `@tauri-apps/plugin-fs`     | Read/write appdata files      |
| Dialog      | `tauri-plugin-dialog` | `@tauri-apps/plugin-dialog` | File picker for audio import  |

### Rust Dependencies (additional)

| Crate      | Purpose                                      |
| ---------- | -------------------------------------------- |
| `rusqlite` | SQLite database                              |
| `uuid`     | Document IDs                                 |
| `reqwest`  | HTTP client for Ollama API + model downloads |
| `tokio`    | Async runtime (Tauri 2 uses tokio)           |
| `chrono`   | Timestamps                                   |
| `regex`    | Parse yt-dlp/ffmpeg progress output          |
