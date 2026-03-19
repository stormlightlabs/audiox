# Murmur Roadmap

Each milestone produces a usable app. Later milestones build on earlier ones.

## Overview

- ~~Part 1 (MVP): M1 - M4~~
- ~~Part 2: M5 - M7~~
- ~~Part 3: M13~~
- ~~Part 4: M9~~
- Part 5: M15, M16
- Part 6: M11, M14
- Part 7: M8
- Part 8: M10
- Part 9: M12

## M15: Local Inference Engine (llama.cpp Sidecar)

**Goal:** Replace Ollama as the default inference backend with a bundled llama-server
sidecar for fully-local metadata generation.

See [migration spec](specs/migration.md) for architecture details.

### Backend

- Add `llama-server` as a new `RuntimeBinarySpec` entry following the existing
  sidecar pattern (platform-target-suffixed binaries in `src-tauri/binaries/`)
- Introduce `LlamaServerState` as Tauri managed state to hold the child process
  handle, assigned port, and idle shutdown timer
- Lazy startup: spawn llama-server on first generation request, health-check via
  `GET /health` before sending prompts
- Idle shutdown: kill the server process after 60 seconds of inactivity to free
  memory and GPU resources
- Graceful cleanup: kill the child process on app exit via the state drop handler
- GGUF model download with progress events, following the whisper model download
  pattern in `bootstrap.rs`. Store models in `appdata/models/gguf/`
- Abstract generation into a `GenerationBackend` dispatch so both llama-server
  and Ollama share the same `process_document_ai` pipeline
- Map the existing Ollama `/api/generate` call to llama-server's `/completion`
  endpoint (prompt in, generated text out)
- Retry logic, metadata parsing (`parse_generated_metadata`), and progress events
  remain unchanged

### Model tiers

Three downloadable GGUF models:

- Small: Gemma 3 1B Instruct Q4_K_M (~810 MB)
- Default: Qwen2.5 1.5B Instruct Q5_K_M (~1.3 GB)
- High: Qwen2.5 3B Instruct Q4_K_M (~2.1 GB)

### Preflight and setup

- Add `LlamaServer` variant to `PreflightCheck` enum
- Check for llama-server binary availability during preflight
  (same resolution chain: sidecar, managed path, system PATH)
- Add `GgufModel` preflight check for whether a GGUF model is downloaded
- Local inference checks use Pass/Warn (not Fail) since Ollama remains a
  fallback option

**Usable state:** Metadata generation works without Ollama installed. The app
bundles llama-server, downloads a GGUF model on first use, and runs inference
locally.

## M16: Inference Backend Selection

**Goal:** Let the user choose between local inference and Ollama, and select
a model quality tier.

### Backend

- New setting key `inference_backend` with values `local` (default) and `ollama`
- New setting key `local_model_tier` with values `small`, `default`, `high`
- `save_app_settings` accepts the new settings and persists them
- `load_runtime_settings` reads them and populates `AppSettings`
- `process_document_ai` reads the backend setting and dispatches accordingly

### Frontend

- Settings view: new "Inference" section with:
  - Backend toggle: Local / Ollama
  - When Local is selected: model tier selector (Small / Default / High) with
    file size displayed, download/delete buttons, and disk usage
  - When Ollama is selected: existing endpoint configuration and model checks
- Setup view: present the backend choice during first-run setup, defaulting
  to Local. Trigger GGUF model download if Local is selected
- Preflight splash: show llama-server and GGUF model status alongside existing
  checks when Local backend is active

### Preflight adaptations

- When `inference_backend = local`: check llama-server binary and GGUF model
  presence, skip Ollama server/model checks
- When `inference_backend = ollama`: run existing Ollama checks, skip
  llama-server/GGUF checks
- `SetupStatus` and `PreflightResult` reflect the active backend only

**Usable state:** User picks Local or Ollama during setup, selects a model
quality tier, and can switch between backends from Settings at any time.

## M8: URL Import via yt-dlp

**Goal:** Paste a URL to download and transcribe audio/video from the web.

- URL input field in Import view (accepts YouTube, Vimeo, SoundCloud, podcast URLs, etc.)
- Fetch metadata without downloading (`yt-dlp --dump-json --no-playlist`): title, duration, thumbnail, uploader
- Display preview card with metadata before confirming download
- Audio-only download mode:

  ```sh
  yt-dlp -x --audio-format wav --audio-quality 0 --no-playlist --newline --ffmpeg-location <bundled> -o <appdata>/audio/<uuid>.%(ext)s <url>
  ```

- Video download mode (for subtitle export):

  ```sh
  yt-dlp -f "bestvideo[ext=mp4]+bestaudio[ext=m4a]/best" --no-playlist --newline --ffmpeg-location <bundled> -o <appdata>/video/<uuid>.%(ext)s <url>
  ```

- Parse progress from stderr (percentage, speed, ETA) for UI progress bar
- Post-download: extract audio via ffmpeg if video → feed into transcription pipeline (M4 + M5)
- Store source URL and yt-dlp metadata on document record

**Usable state:** User pastes a URL → sees video/audio preview → downloads → gets a fully processed document in their library.

## M10: Subtitle Export & Video Burn-in

**Goal:** Export subtitles as files or burn them into downloaded videos.

- Download SRT/VTT files from the Document view
- For documents with a video file (yt-dlp downloads), offer "Export with subtitles"
- Burn subtitles into video via ffmpeg:

  ```sh
  ffmpeg -i <video.mp4> -vf "subtitles=<subs.srt>" -c:a copy -y <output.mp4>
  ```

- Progress reporting via ffmpeg `-progress pipe:1`
- Save/export the subtitled video file

**Usable state:** User can export standalone subtitle files or get a video with burned-in subtitles.

## ✅ M11: Settings & Model Management

**Goal:** User can configure whisper models, Ollama settings, and app preferences.

- Settings view:
  - Whisper model selector (tiny/base/small/medium/large) with download management
  - Ollama endpoint configuration (custom host/port)
  - Default language selection (auto, en, etc.)
  - Thread count for whisper
- Model download/delete management with disk usage display
- Re-check Ollama connection from settings
- Re-run preflight checks from settings
- Persist all settings in SQLite settings table

**Usable state:** User can tune performance/accuracy tradeoff and manage disk usage.

## M12: Polish & Quality of Life/Parking Lot

**Goal:** Production-quality UX.

- [ ] Page transitions with solid-motionone (view enter/exit animations)
- [ ] Loading/processing states with skeleton screens
- [ ] Error handling with user-friendly messages and retry actions
- [ ] Drag-and-drop audio/video file import (in addition to file picker)
- [ ] Keyboard shortcuts (Ctrl+N record, Ctrl+F search, etc.)
- [ ] Audio playback in Document view synced with transcript timestamps (click segment → play from that point)
- [ ] Export document as Markdown file
- [ ] Empty states for library, search results
- [ ] Window title updates based on current view
- [x] App icon and branding
- [ ] Allow playback of imported audio files

## ✅ M14: Text Note Import (.txt / .md)

**Goal:** Import plain text or Markdown notes and generate metadata, summaries, keywords, and embeddings, bypassing the audio/transcription pipeline entirely.

### Backend

- New Tauri command `import_text_note(source_path: String)`:
  - Read file contents from disk (`.txt` or `.md`)
  - Use file contents as the `transcript` field (the canonical text body)
  - Generate synthetic segments by splitting on paragraph boundaries (double newline), assigning sequential `start_ms`/`end_ms` offsets (e.g., 0–999, 1000–1999) for UI consistency
  - Feed text into existing `process_document_ai()` → title, summary, keywords, embeddings
  - Persist via `persist_document()` with `source_type = "text_note"`
  - Set `audio_path`, `subtitle_srt_path`, `subtitle_vtt_path` to empty strings; `duration_seconds = 0`
  - Store original file path in `source_uri`
- New Tauri command `import_text_content(title: String, content: String)`:
  - Same pipeline but accepts raw text (for paste-to-import)
  - `source_type = "text_paste"`
- Progress events emitted on `"import://metadata-progress"` (reuse existing event)

### Frontend

- Extend Import view with a "Notes" tab/section:
  - File picker filtered to `.txt, .md` extensions
  - Drag-and-drop zone accepting text files
  - Optional: textarea for paste-to-import with a "Process" button
- Preview: show first ~500 chars of the note before confirming import
- Reuse existing progress bar (metadata generation phase only — no conversion/transcription steps)
- On completion, navigate to `/document/{id}` as with audio imports

### Document View adaptations

- Hide audio player and subtitle controls when `source_type` is `text_note` or `text_paste`
- Hide duration display
- Show "Text Note" or "Pasted Note" badge in document header
- Render Markdown content if the source was `.md` (use a lightweight MD renderer)

### Library view adaptations

- Add `source_type` filter chip: Audio | Recording | Text Note | All
- Show a distinct icon for text-sourced documents (e.g., `i-bi-file-text`)

**Usable state:** User imports a `.txt`/`.md` file or pastes text → sees a preview → confirms → gets a fully indexed document with AI-generated title, summary, keywords, and semantic search support.

## Completed

These are summarized in the project's [CHANGELOG](../CHANGELOG.md)

- M1: Project Scaffold & Shell
- M2: Preflight Splash Screen
- M3: First-Run Setup & Dependency Management
- M4: Audio Import & Transcription (with ffmpeg)
- M5: AI-Powered Document Processing
- M6: Document Library
- M7: Semantic Search
- M9: Microphone Recording
- M13: Local Embedding (Decouple from Ollama)
