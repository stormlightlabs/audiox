# Audio X Roadmap

Each milestone produces a usable app. Later milestones build on earlier ones.

## Overview

- Part 1 (MVP): M1 - M4
- Part 2: M5 - M7
- Part 3: M8
- Part 4: M9 - M10
- Part 5: M11 - M12

## ✅ M1: Project Scaffold & Shell

**Goal:** Tauri + SolidJS app boots, Tailwind styling works, basic navigation between views.

- Tauri 2 project with SolidJS + Tailwind v4 + solid-motionone (already bootstrapped)
- Add Tauri plugins: `shell`, `fs`, `dialog`
- Add Rust crates: `rusqlite`, `uuid`, `reqwest`, `tokio`, `chrono`, `serde_json`, `regex`
- App-wide SolidJS store + context provider
- Router/view switching (Splash, Setup, Record, Import, Library, Document, Settings (`@solidjs/router`))
- SQLite database initialization on startup (create tables if not exist)
- AppData directory structure creation on first launch (`models/`, `audio/`, `video/`, `subtitles/`, `bin/`, `db/`)
- Basic layout shell: sidebar/nav + content area with Tailwind

**Usable state:** App launches, navigates between empty views, database is ready.

## ✅ M2: Preflight Splash Screen

**Goal:** Every launch validates runtime dependencies before showing the main UI.

- Splash view as the app entry point (logo, animated checklist via solid-motionone)
- Add sidecar-first binary resolver in Rust (`whisper-cli`, `ffmpeg`, and `yt-dlp` bundled via Tauri `externalBin`)
- Preflight checks each binary by running `--version` / `-version`
- Resolution order: sidecar -> appdata runtime cache -> system PATH
- Check whisper model file exists in `appdata/models/`
- Check Ollama server reachable (`GET /api/tags`, 3s timeout)
- Check required Ollama models present (`nomic-embed-text`, `gemma3:4b`)
- Open/create SQLite database, run migrations
- Each check reports `pass` / `fail` / `warn` (yt-dlp is `warn` — optional)
- All pass → auto-transition to Library view
- Any fail → show inline guidance with retry button
- Missing models → transition to Setup wizard (M3)

**Usable state:** App boots → splash validates everything → user knows exactly what's ready or needs attention.

## ✅ M3: First-Run Setup & Dependency Management

**Goal:** App is one-click ready on first run (only models are downloaded).

- Detect missing whisper model, download `ggml-base.en.bin` (142 MB) from HuggingFace with progress bar
- Detect missing Ollama models, pull via `POST /api/pull` with streaming progress
- If Ollama is not running, show install/start guidance
- Setup wizard view: step-by-step status for each dependency with one primary CTA
- Persist setup completion state in SQLite settings table
- On completion, re-run preflight → transition to Library

**Usable state:** User launches app → clicks setup → models download/pull → app is ready for use.

## ✅ M4: Audio Import & Transcription (with ffmpeg)

**Goal:** Import an audio file, convert it with ffmpeg, and get a transcript.

- File picker dialog (via `tauri-plugin-dialog`) for audio file selection (mp3, m4a, wav, flac, ogg, opus, webm)
- Copy imported file to `appdata/audio/`
- ffmpeg sidecar (bundled) converts any format to 16kHz mono WAV:

  ```sh
  ffmpeg -i <input> -ar 16000 -ac 1 -c:a pcm_s16le -y <output.wav>
  ```

- Parse ffmpeg progress via `-progress pipe:1` for conversion status
- Spawn `whisper-cli` sidecar (bundled) for transcription with progress streaming
- Generate subtitles alongside transcript (`-osrt -ovtt` flags)
- Parse whisper JSON output into timestamped segments
- Display raw transcript in a Document view with segment timestamps
- Save transcript, audio path, and subtitle paths to SQLite

**Usable state:** User picks any audio file → sees conversion + transcription progress → reads the transcript with subtitles generated.

## ✅ M5: AI-Powered Document Processing

**Goal:** Transcripts are automatically enriched with AI-generated metadata and embeddings.

- After transcription, call Gemma via Ollama to generate:
  - Document title
  - 2-3 sentence summary
  - Keyword/tag list
- Chunk transcript into ~512-token segments
- Generate embeddings for each chunk via `nomic-embed-text`
- Store document metadata, chunks, and embeddings in SQLite
- Show generated title, summary, and tags in the Document view
- Allow user to edit title and tags after generation

**Usable state:** Import audio → automatic transcription → AI generates title/summary/tags → full document view.

## ✅ M6: Document Library

**Goal:** Browse, manage, and organize all transcribed documents.

- Library view: grid/list of documents showing title, summary, date, duration, tags
- Sort by date created, title, duration
- Filter by tags
- Delete document (with confirmation — removes audio, video, subtitles, transcript, embeddings)
- Click document → navigate to full Document view
- Animated list transitions with solid-motionone (enter/exit/reorder)

**Usable state:** User has a browsable library of all their transcribed documents.

## ✅ M7: Semantic Search

**Goal:** Search across all documents using natural language.

- Search input in Library view header
- Embed search query via `nomic-embed-text`
- Cosine similarity computation in Rust over all chunk embeddings
- Return top-K results with matched chunk text + parent document reference
- Search results view: ranked list with highlighted matching chunks and document links
- Click result → navigate to Document view scrolled to matching segment

**Usable state:** User types a question → sees relevant transcript passages ranked by relevance.

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

## ✅ M9: Microphone Recording

**Goal:** Record audio directly from the microphone within the app.

- WebView `getUserMedia` for mic access (permission prompt handling)
- MediaRecorder to capture audio stream
- Live recording UI: waveform visualization, elapsed time, pause/resume, stop
- Recording pulse animation with solid-motionone
- On stop: send audio blob to Rust backend → ffmpeg converts to WAV → trigger transcription pipeline (M4 + M5)
- Audio device selection in Settings (if multiple inputs available)

**Usable state:** User clicks record → speaks → stops → gets a fully processed document in their library.

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

## M11: Settings & Model Management

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

- [ ]  Page transitions with solid-motionone (view enter/exit animations)
- [ ] Loading/processing states with skeleton screens
- [ ] Error handling with user-friendly messages and retry actions
- [ ] Drag-and-drop audio/video file import (in addition to file picker)
- [ ] Keyboard shortcuts (Ctrl+N record, Ctrl+F search, etc.)
- [ ] Audio playback in Document view synced with transcript timestamps (click segment → play from that point)
- [ ] Export document as Markdown file
- [ ] Empty states for library, search results
- [ ] Window title updates based on current view
- [ ] App icon and branding
