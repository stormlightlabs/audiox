---
title: Architecture Migration
updated: 2026-03-19
---

This document outlines migrating the default inference/summarization engine from an Ollama server connection to sidecar usage of llama.cpp.
The migration preserves Ollama as a fallback option while making fully-local inference the default path.

## Current state

Metadata generation (title, summary, tags) currently relies on an external Ollama server at `http://localhost:11434`.
The app calls Ollama's `/api/generate` endpoint with `gemma3:4b` as the default model.
Ollama is optional in preflight (Warn status, not Fail), so the app works without it but loses AI-powered metadata.

Embeddings are already local via the `fastembed` crate (M13), so this migration only affects the generation/summarization pipeline.

## Models

Three GGUF model tiers for local inference, downloaded from HuggingFace on demand:

- _Small_: [Gemma 3 1B Instruct Q4_K_M](https://huggingface.co/bartowski/google_gemma-3-1B-it-GGUF/resolve/main/google_gemma-3-1b-it-Q4_K_M.gguf) (~806 MB)
- _Default_: [Qwen2.5 1.5B Instruct Q5_K_M](https://huggingface.co/Qwen/Qwen2.5-1.5B-Instruct-GGUF/resolve/main/qwen2.5-1.5b-instruct-q5_k_m.gguf) (~1.29 GB)
- _High_: [Qwen2.5 3B Instruct Q4_K_M](https://huggingface.co/Qwen/Qwen2.5-3B-Instruct-GGUF/resolve/main/qwen2.5-3b-instruct-q4_k_m.gguf) (~2.10 GB)

All models use the Q4_K_M or Q5_K_M quantization level, which balances quality and file size for on-device inference on consumer hardware.

## Flow

During preflight, the user chooses how to interact with a model:

1. Use a local model (default)
2. Use Ollama

If the user selects a local model, the app downloads the selected GGUF file and
stores it in `appdata/models/gguf/`. The llama-server sidecar starts on demand when
metadata generation is needed and stops after an idle timeout.

If the user selects Ollama, the app uses the existing Ollama server connection and looks for models they have downloaded.
If there are none, it suggests the existing default, Gemma3.

## Research

### llama-server

The `llama-server` binary (from the llama.cpp project) runs a local HTTP inference server.
It provides an OpenAI-compatible API at `/v1/chat/completions` and a native `/completion` endpoint.
The server is started as a subprocess with a model path and port:

```sh
llama-server -m model.gguf --port 9741 --ctx-size 2048
```

On macOS, llama.cpp uses Metal for GPU acceleration automatically.
On Linux/Windows, Vulkan provides cross-vendor GPU support.
CPU-only fallback works everywhere.

### Sidecar integration

The existing `RuntimeBinarySpec` pattern handles binary resolution through a priority chain:
bundled sidecar, managed binary path, system PATH, then runtime download.
llama-server fits this pattern as a new `RuntimeBinarySpec` entry alongside whisper-cli, ffmpeg, and yt-dlp.

Platform-target-suffixed binaries go in `src-tauri/binaries/` following the established naming convention (for example, `llama-server-aarch64-apple-darwin`).

### Lifecycle management

Unlike whisper-cli and ffmpeg which run as one-shot commands, llama-server is a long-running process.
The app needs to:

- Start the server before the first generation request
- Health-check via `GET /health` before sending requests
- Stop the server after an idle timeout (for example, 60 seconds after last request)
- Handle process crashes and restart on next request

### API compatibility

The Ollama `/api/generate` call maps to llama-server's `/completion` endpoint.
The request/response shapes differ slightly:

- Ollama: `{ "model": "...", "prompt": "...", "stream": false }`
- llama-server: `{ "prompt": "...", "stream": false, "n_predict": 512 }`

The response field changes from `response` (Ollama) to `content` (llama-server).
This mapping lives in the generation abstraction layer so both backends share the same `process_document_ai` pipeline.

## Implementation

The migration splits into two milestones: M15 (core engine) and M16 (backend selection UI).
See the [roadmap](../roadmap.md) for details.

### Backend abstraction

Introduce a `GenerationBackend` trait or enum that `generate_metadata_once` dispatches on.
Both backends accept a transcript prompt and return generated text.
The existing retry logic, metadata parsing, and progress events remain unchanged.

### Model management

GGUF model downloads follow the same pattern as whisper model downloads in `bootstrap.rs`:
HTTP GET with progress events, temporary file with rename on completion, stored in `appdata/models/gguf/`.
A settings key tracks the selected model tier.

### Process lifecycle

A `LlamaServerState` managed by Tauri holds the child process handle, port, and idle timer.
The server starts lazily on first generation request and shuts down after the idle timeout.
On app exit, the process is killed via the drop handler.

## Roadmap

See milestones M15 and M16 in the project [roadmap](../roadmap.md).
