# Text Note Import Pipeline

Text note import reuses the metadata generation and embedding pipelines, bypassing audio conversion and transcription entirely. This enables the document library to index plain text and Markdown content with the same AI-powered metadata and semantic search capabilities as transcribed audio.

## Supported Formats

| Format     | Extension | Handling                                       |
| ---------- | --------- | ---------------------------------------------- |
| Plain text | `.txt`    | Read as-is, treated as transcript body         |
| Markdown   | `.md`     | Read as-is (raw Markdown stored as transcript) |

## Tauri Commands

```rust
#[tauri::command]
async fn import_text_note(
    app: tauri::AppHandle,
    source_path: String,
) -> Result<String, String> {
    // 1. Read file from source_path
    // 2. Generate synthetic segments (paragraph-split)
    // 3. Call process_document_ai() with file contents
    // 4. Persist with source_type = "text_note"
    // Returns: document ID
}

#[tauri::command]
async fn import_text_content(
    app: tauri::AppHandle,
    title: String,
    content: String,
) -> Result<String, String> {
    // Same as above but content is passed directly
    // source_type = "text_paste"
}
```

## Synthetic Segment Generation

Text notes have no real timestamps. To maintain schema compatibility with audio-sourced documents, the pipeline generates synthetic segments from paragraph boundaries:

```rust
fn build_text_segments(text: &str) -> Vec<TranscriptSegment> {
    text.split("\n\n")
        .filter(|p| !p.trim().is_empty())
        .enumerate()
        .map(|(i, paragraph)| TranscriptSegment {
            start_ms: (i as i64) * 1000,
            end_ms: ((i + 1) as i64) * 1000 - 1,
            text: paragraph.trim().to_string(),
        })
        .collect()
}
```

Each paragraph gets a 1-second virtual window. These synthetic timestamps are not displayed in the UI for text-sourced documents but allow the existing chunk/segment storage to work unchanged.

## Pipeline Flow

```text
.txt/.md file (or pasted text)
  │
  ├─ Read file contents
  │
  ├─ Split into paragraphs → synthetic segments
  │
  ├─ process_document_ai()          ← reused from ollama.md
  │   ├─ Ollama: title, summary, keywords
  │   └─ fastembed: chunk + embed
  │
  └─ persist_document()             ← reused from data-model.md
      source_type = "text_note" | "text_paste"
      audio_path  = ""
      duration_seconds = 0
```

## Document Differentiation

The `source_type` field distinguishes text-sourced documents from audio-sourced ones. Frontend views use this to conditionally render UI elements:

| source_type            | Audio player | Subtitles | Duration | Badge         |
| ---------------------- | ------------ | --------- | -------- | ------------- |
| `file_import`          | Yes          | Yes       | Yes      | —             |
| `microphone_recording` | Yes          | Yes       | Yes      | —             |
| `text_note`            | Hidden       | Hidden    | Hidden   | "Text Note"   |
| `text_paste`           | Hidden       | Hidden    | Hidden   | "Pasted Note" |

## IPC Command Addition

| Command               | Args             | Returns  | Description                             |
| --------------------- | ---------------- | -------- | --------------------------------------- |
| `import_text_note`    | `source_path`    | `doc_id` | Import .txt/.md file, generate metadata |
| `import_text_content` | `title, content` | `doc_id` | Import pasted text, generate metadata   |
