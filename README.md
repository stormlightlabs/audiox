<!-- markdownlint-disable MD033 -->
# Audio X

A desktop app that transcribes audio locally with [whisper.cpp](https://github.com/ggerganov/whisper.cpp) and builds a searchable document library using [Ollama](https://ollama.com) models.

## Features

- **Audio import** — Import mp3, m4a, wav, flac, ogg, opus, or webm files
- **Microphone recording** — Record directly in the app with device selection
- **Local transcription** — whisper.cpp transcribes audio with timestamped segments
- **AI-powered metadata** — Ollama (Gemma 3) generates titles, summaries, and tags
- **Semantic search** — Natural language search across all documents via embeddings
- **Document library** — Browse, sort, filter, and manage transcribed documents
- **Subtitle generation** — SRT and VTT files generated alongside transcripts

## Tech Stack

| Layer    | Technology                               |
| -------- | ---------------------------------------- |
| Frontend | SolidJS, Tailwind v4, TypeScript         |
| Backend  | Rust, Tauri 2                            |
| Database | SQLite (`rusqlite`)                      |
| AI       | Ollama (`gemma3:4b`, `nomic-embed-text`) |
| Audio    | whisper.cpp, ffmpeg                      |

## Requirements

- [Ollama](https://ollama.com) installed and running (`http://localhost:11434`)
- Internet connection on first run (to download models)

For development, you also need `whisper-cli`, `ffmpeg`, and `yt-dlp` on your PATH (or let `setup.sh` create sidecar wrappers that forward to them).

## Getting Started

### 1. Install dependencies

```sh
pnpm install
```

### 2. Set up dev sidecars

```sh
bash setup.sh
```

This creates target-suffixed wrapper scripts in `src-tauri/binaries/` that forward to your system PATH binaries (`whisper-cli`, `ffmpeg`, `yt-dlp`). This keeps development smooth without committing large binaries to the repo.

### 3. Run the app

```sh
pnpm tauri dev
```

On first launch, the app runs preflight checks and walks you through downloading the required models (whisper model + Ollama models).

## Project

<details>
<summary>Structure</summary>

```sh
src/                    # Frontend (SolidJS + TypeScript)
  views/                # App views (Splash, Setup, Record, Import, Library, Document, Settings)
  state/                # Global app state (AppContext)

src-tauri/              # Backend (Rust + Tauri 2)
  src/
    commands.rs         # Tauri IPC commands
    bootstrap.rs        # Dependency checking & setup
    storage.rs          # SQLite database & file management
    models.rs           # Data structures & constants
    parsers.rs          # Whisper/Ollama output parsing
  binaries/             # Sidecar binaries (whisper-cli, ffmpeg, yt-dlp)

docs/
  spec.md              # Technical specification
  roadmap.md           # Development roadmap
```

</details>

<details>
<summary>How It Works</summary>

1. **Preflight** — App checks for whisper-cli, ffmpeg, whisper model, Ollama, and required Ollama models
2. **Setup** — First-run wizard downloads `ggml-base.en.bin` and pulls Ollama models
3. **Import/Record** — Audio is converted to 16kHz mono WAV via ffmpeg
4. **Transcribe** — whisper.cpp produces timestamped transcript + SRT/VTT subtitles
5. **Enrich** — Gemma 3 generates a title, summary, and tags from the transcript
6. **Embed** — Transcript is chunked (~512 tokens) and embedded via nomic-embed-text
7. **Search** — Queries are embedded and matched against chunks using cosine similarity

</details>

<details>
<summary>Sidecar Packaging</summary>

For production builds, place real target-suffixed binaries in `src-tauri/binaries/` before packaging. Sidecar entries are configured in `src-tauri/tauri.conf.json`:

- `binaries/whisper-cli`
- `binaries/ffmpeg`
- `binaries/yt-dlp`

See [src-tauri/binaries/README.md](./src-tauri/binaries/README.md) for details.

</details>

<details>
<summary>Commands</summary>

### Frontend

```sh
pnpm dev                # Vite dev server only
pnpm tauri dev          # Full app (Vite + Tauri)
pnpm build              # Build frontend
pnpm lint               # ESLint
pnpm test               # Vitest
pnpm check              # TypeScript check (can use pnpm typecheck)
```

### Rust

```sh
cargo test --manifest-path src-tauri/Cargo.toml                         # Tests
cargo clippy --manifest-path src-tauri/Cargo.toml  --fix --allow-dirty  # Linting
cargo fmt --manifest-path src-tauri/Cargo.toml                          # Formatting
```

</details>
