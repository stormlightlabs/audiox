# Audio X

Audio X transcribes audio locally with `whisper.cpp` and builds searchable documents with Ollama models.

## UX Strategy

The app is optimized for "open app -> click setup -> get started":

- `whisper-cli`, `ffmpeg`, and `yt-dlp` are **sidecars** (bundled with app builds).
- First-run setup downloads only:
  - `ggml-base.en.bin` whisper model
  - missing Ollama models (`nomic-embed-text`, `gemma3:4b`)
- `yt-dlp` remains optional at the feature level (URL import), but now uses the same sidecar-first resolution.

## Requirements

- Ollama installed and running locally (`http://localhost:11434`)
- Internet connection on first run for model downloads

## Local Development

### 1. Install dependencies

```sh
pnpm install
```

### 2. Run the app

```sh
bash setup.sh
pnpm tauri dev
```

### 3. Dev sidecar wrappers

`setup.sh` (and `pnpm setup:sidecars`) creates target-suffixed wrapper sidecars in `src-tauri/binaries/` that forward to system PATH binaries.

This keeps local development smooth without committing large binaries.
For production builds, replace wrappers with real release binaries.

## Sidecar Packaging

Tauri sidecar entries are configured in `src-tauri/tauri.conf.json`:

- `binaries/whisper-cli`
- `binaries/ffmpeg`
- `binaries/yt-dlp`

Place target-suffixed sidecar binaries in `src-tauri/binaries/` before packaging. See [README.md](./src-tauri/binaries/README.md).

## Commands

```sh
pnpm lint
pnpm test
pnpm typecheck
cargo test --manifest-path src-tauri/Cargo.toml
```
