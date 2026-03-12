# Architecture

Audio X is a Tauri 2 desktop app (SolidJS + Tailwind v4 frontend, Rust backend) that transcribes audio to text using whisper.cpp and builds a searchable document library from transcripts. Embeddings are generated locally via fastembed-rs (ONNX Runtime), while Ollama-hosted Gemma models handle text generation (titles, summaries, keywords). It supports local audio files, microphone recording, and URL-based media import via yt-dlp.

```text
┌───────────────────────────────────────────────────────┐
│  SolidJS Frontend (WebView)                           │
│  ┌────────┐ ┌────────┐ ┌──────────┐ ┌──────────────┐  │
│  │ Splash │ │Recorder│ │ Library  │ │Search/Viewer │  │
│  └───┬────┘ └───┬────┘ └────┬─────┘ └───────┬──────┘  │
│      │ invoke() │ invoke()  │ invoke()      │         │
│      ├──────────┼───────────┼───────────────┼─────────┤
│      │ Rust Backend (Tauri)                           │
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
│  │ fastembed-rs (local ONNX embedding)              │ │
│  └──────────────────────────────────────────────────┘ │
│  ┌──────────────────────────────────────────────────┐ │
│  │ Ollama HTTP Client (generate only)               │ │
│  └──────────────────────────────────────────────────┘ │
│  ┌──────────────────────────────────────────────────┐ │
│  │ SQLite (documents, embeddings, metadata)         │ │
│  └──────────────────────────────────────────────────┘ │
└───────────────────────────────────────────────────────┘
```

## Tauri

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

| Plugin         | Crate                         | JS Package                        | Purpose                                   |
| -------------- | ----------------------------- | --------------------------------- | ----------------------------------------- |
| Shell          | `tauri-plugin-shell`          | `@tauri-apps/plugin-shell`        | Execute external tools safely             |
| File System    | `tauri-plugin-fs`             | `@tauri-apps/plugin-fs`           | Read/write appdata files                  |
| Dialog         | `tauri-plugin-dialog`         | `@tauri-apps/plugin-dialog`       | File picker for audio import              |
| Audio Recorder | `tauri-plugin-audio-recorder` | `tauri-plugin-audio-recorder-api` | Native mic recording (cpal-based, all OS) |

### Rust Dependencies (additional)

| Crate       | Purpose                                                 |
| ----------- | ------------------------------------------------------- |
| `rusqlite`  | SQLite database                                         |
| `uuid`      | Document IDs                                            |
| `fastembed` | Local embedding model (ONNX Runtime + nomic-embed-text) |
| `reqwest`   | HTTP client for Ollama API + model downloads            |
| `tokio`     | Async runtime (Tauri 2 uses tokio)                      |
| `chrono`    | Timestamps                                              |
| `regex`     | Parse yt-dlp/ffmpeg progress output                     |
