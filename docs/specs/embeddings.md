# Local Embedding (fastembed-rs)

Embeddings are generated locally via the `fastembed` Rust crate (ONNX Runtime-based), removing the Ollama dependency for search and document indexing.

## Why Local Embedding

- **No external server required** — search and library browsing work without Ollama running
- **Same model, same dimensions** — `NomicEmbedTextV15` produces 768-dim vectors identical to Ollama's `nomic-embed-text`. Existing embeddings remain valid with no migration.
- **Batteries-included** — fastembed handles model download, caching, tokenization, pooling, and normalization automatically
- **Production-proven** — used by SurrealDB and Qdrant

## Model

| Model                | Format    | Dimensions | Size    | Download                 |
| -------------------- | --------- | ---------- | ------- | ------------------------ |
| `NomicEmbedTextV15`  | ONNX fp16 | 768        | ~262 MB | Auto-downloaded by crate |
| `NomicEmbedTextV15Q` | ONNX int8 | 768        | smaller | Auto-downloaded by crate |

Model files are cached in `appdata/models/embed/` on first use. CPU inference for the 137M-param model is single-digit milliseconds per embedding — no GPU acceleration required.

## Rust Integration

```rust
use fastembed::{TextEmbedding, InitOptions, EmbeddingModel};

// Initialize once at app startup, hold in Tauri managed state
let model = TextEmbedding::try_new(
    InitOptions::new(EmbeddingModel::NomicEmbedTextV15)
        .with_cache_dir(appdata.join("models/embed"))
        .with_show_download_progress(true),
)?;
```

### Embedding Pipeline

For each transcript chunk (~384 words / ~512 tokens):

```rust
// Document indexing — prefix with "search_document:"
let chunks = vec!["search_document: segment text here"];
let embeddings: Vec<Vec<f32>> = model.embed(chunks, None)?;
// embeddings[0].len() == 768

// Search query — prefix with "search_query:"
let query = vec!["search_query: user's search terms"];
let query_embedding: Vec<Vec<f32>> = model.embed(query, None)?;
```

Task-type prefixes (`search_document:` / `search_query:`) improve retrieval quality over unprefixed embeddings.

Store embeddings as binary blobs in SQLite alongside the source text chunk and document reference.
