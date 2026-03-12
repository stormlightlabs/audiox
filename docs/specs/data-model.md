# Data Model (SQLite)

Use `rusqlite` in the Rust backend. Database stored at `appdata/db/audiox.db`.

## File System

All runtime data lives under the Tauri `appDataDir` (`~/Library/Application Support/org.stormlightlabs.audiox/` on macOS):

```sh
appdata/
  models/
    ggml-base.en.bin          # whisper model (downloaded on first run)
    embed/                    # fastembed model cache (auto-downloaded on first use)
      nomic-embed-text-v1.5/  # ~262 MB ONNX model files
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

## Database

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

1. Embed the query locally via fastembed (`search_query:` prefix) — no external server needed
2. Compute cosine similarity between query embedding and all chunk embeddings in SQLite (Rust-side, in-memory scan)
3. Return top-K chunks with their parent document references
4. For small-medium libraries (< 10k chunks), brute-force cosine similarity in Rust is fast enough. No vector DB needed.
