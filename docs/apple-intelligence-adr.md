---
title: ADR - Apple Intelligence on macOS, Ollama Elsewhere
updated: 2026-03-27
---

## Status

Accepted

## Context

Murmur already performs transcription with `whisper.cpp` and embeddings locally with `fastembed`.
The only AI feature that still depended on Ollama was metadata generation for titles, summaries,
and tags.

The migration goal was:

- Prefer Apple Intelligence on supported Macs.
- Keep Ollama as the existing metadata backend for Windows and Linux.
- Preserve Ollama as a fallback on macOS when Apple Intelligence is unavailable, disabled,
  unsupported, or locale-blocked.
- Avoid turning metadata generation into a hard startup dependency.

## Decision

We implemented a metadata-backend abstraction instead of a generic inference abstraction.

### 1. Backend scope

- The backend switch only affects metadata generation.
- Embeddings remain local via `fastembed`.
- Search, chunk storage, and embedding schema are unchanged.

### 2. Backend modes

We introduced:

- `auto`
- `apple_intelligence`
- `ollama`

Resolution behavior is:

- macOS `auto`: use Apple Intelligence when available, otherwise fall back to Ollama if reachable.
- macOS forced `apple_intelligence`: do not silently switch to Ollama.
- macOS forced `ollama`: use Ollama.
- Windows/Linux: resolve to Ollama regardless of `auto`.

### 3. Apple Intelligence bridge design

We implemented the macOS integration as a Swift bridge compiled from
`src-tauri/native/apple_intelligence_bridge.swift`.

Key decisions:

- The bridge is built as a `.dylib`, not statically linked into Rust.
- Rust loads the bridge dynamically with `libloading`.
- This avoids a hard link-time dependency on `FoundationModels` for every macOS build/run path.
- Older Macs can still run the app and fall back to Ollama if the bridge or framework is unavailable.

The bridge exports a small C ABI:

- `murmur_probe_apple_intelligence`
- `murmur_generate_apple_metadata`
- `murmur_free_bridge_string`

### 4. Structured generation approach

The original implementation asked Ollama for JSON and parsed the returned text.

For Apple Intelligence, we intentionally did not use the `@Generable` macro path because the
toolchain/plugin-server path was unreliable in the local sandboxed build environment.

Instead, the Swift bridge uses:

- `FoundationModels.LanguageModelSession`
- `FoundationModels.GenerationSchema`
- `FoundationModels.DynamicGenerationSchema`

That keeps the Apple path structured without depending on macro expansion.

### 5. Setup and settings behavior

We added `metadata_backend_mode` to persisted app settings and exposed a new
`get_metadata_backend_status` Tauri command.

Settings now:

- Show backend mode selection on macOS.
- Keep Ollama endpoint editing available only when the selected mode may use Ollama.
- Show the resolved backend plus Apple/Ollama availability details.

Setup now:

- Downloads whisper and embedding models as before.
- Skips Ollama pulls on macOS when Apple Intelligence is already the resolved backend.
- Uses Ollama model pull flow when Ollama is the resolved backend.
- Keeps metadata guidance optional rather than blocking transcription/search readiness.

### 6. Preflight behavior

Preflight still treats metadata support as optional.

On macOS:

- If Apple Intelligence is available and Ollama is not selected, the Ollama preflight entries are
  reported as optional/pass instead of nagging about a missing server.

On Windows/Linux:

- Existing Ollama warnings remain.

## Consequences

### Positive

- Macs can use the system on-device model without requiring Ollama.
- Windows and Linux behavior stays stable.
- Older Macs retain a fallback path through Ollama.
- Metadata generation remains a soft requirement.

### Tradeoffs

- The macOS path now depends on a generated Swift bridge artifact.
- Packaging includes a generated bridge `.dylib` as a bundled resource.
- The Apple path currently uses explicit schema construction instead of the nicer macro syntax.

## Follow-up Notes

- If Apple’s macro toolchain becomes reliable in this build environment, the bridge can be
  simplified from dynamic schema construction to `@Generable` types.
- If packaged-app resource lookup ever becomes brittle, the bridge location should be formalized
  further in the Tauri bundling pipeline instead of relying on the current generated resource path.
