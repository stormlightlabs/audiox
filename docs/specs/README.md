# Audio X Technical Specification

This directory contains the modular technical specification for Audio X.

## Index

1. [Architecture](architecture.md) — High level project summary & system diagram and component overview.
2. [Preflight Checks](preflight.md) — Startup validation and splash screen logic.
3. [whisper.cpp Integration](whisper.md) — Transcription pipeline and recording.
4. [yt-dlp Integration](yt-dlp.md) — URL-based media import.
5. [ffmpeg Integration](ffmpeg.md) — Audio/video processing and format conversion.
6. [Local Embeddings](embeddings.md) — RAG/Search via fastembed-rs.
7. [Ollama Integration](ollama.md) — LLM-powered metadata generation.
8. [Text Note Import](text-import.md) — Pipeline for .txt and .md files.
9. [Data Model](data-model.md) — SQLite schema and indexing.
10. [Frontend Architecture](frontend.md) — SolidJS views, state, and IPC.
