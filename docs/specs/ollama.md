# Ollama Integration

## Prerequisites

Ollama must be installed and running on `http://localhost:11434` for **document creation** (title, summary, keyword generation). It is **not required** for search or library browsing.

**Health check:** `GET http://localhost:11434/api/tags` — confirms server is running and returns installed models.

## Required Models

| Purpose        | Model       | Pull Command            |
| -------------- | ----------- | ----------------------- |
| Text transform | `gemma3:4b` | `ollama pull gemma3:4b` |

**Why gemma3:4b:** Multimodal, 128K context window, runs well on 8GB+ RAM with QAT quantization. Suitable for summarization, title generation, and keyword extraction.

### Model Setup Flow

On first launch (or when models are missing):

1. Check `GET /api/tags` for installed models
2. If `gemma3:4b` is missing, call `POST /api/pull` with streaming progress
3. Display download progress in the UI (Ollama handles the actual download)
4. Mark setup complete when the model responds successfully

## Document Transformation Pipeline

After transcription completes, use Gemma to generate structured metadata:

**1. Title Generation:**

```text
POST http://localhost:11434/api/generate
{
  "model": "gemma3:4b",
  "prompt": "Generate a concise title for this transcript:\n\n<transcript_text>\n\nTitle:",
  "stream": false
}
```

**2. Summary Generation:**

```text
POST http://localhost:11434/api/generate
{
  "model": "gemma3:4b",
  "prompt": "Write a 2-3 sentence summary of this transcript:\n\n<transcript_text>\n\nSummary:",
  "stream": false
}
```

**3. Keyword/Tag Extraction:**

```text
POST http://localhost:11434/api/generate
{
  "model": "gemma3:4b",
  "prompt": "Extract 3-7 keywords from this transcript as a comma-separated list:\n\n<transcript_text>\n\nKeywords:",
  "stream": false
}
```
