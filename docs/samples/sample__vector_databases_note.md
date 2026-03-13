# Vector Databases and Similarity Search

## What Are Vector Embeddings

An embedding maps discrete data (text, images, audio) to a dense, fixed-dimensional floating-point vector. Semantically similar inputs produce vectors that are close in the embedding space. A typical text embedding model (e.g., nomic-embed-text) outputs 768-dimensional vectors.

## Distance Metrics

| Metric            | Formula                  | Range       | Use case               |
| ----------------- | ------------------------ | ----------- | ---------------------- | --- | --- | --- | --- | --- | --- | ------- | ---------------------------- |
| Cosine similarity | dot(a,b) / (             |             | a                      |     | \*  |     | b   |     | )   | [-1, 1] | Text similarity (normalized) |
| Euclidean (L2)    | sqrt(sum((a_i - b_i)^2)) | [0, inf)    | Spatial clustering     |
| Dot product       | sum(a_i \* b_i)          | (-inf, inf) | Pre-normalized vectors |

Cosine similarity is standard for text embeddings because it is scale-invariant — only the direction of the vector matters, not its magnitude.

## Indexing Strategies

Brute-force cosine similarity (O(n) per query) is feasible for small datasets (<100k vectors). Larger datasets require approximate nearest neighbor (ANN) indices:

- **HNSW (Hierarchical Navigable Small World)**: graph-based. O(log n) query time. High recall (>95%) with tunable `ef_construction` and `M` parameters. Memory-resident. Used by Qdrant, Weaviate, pgvector.
- **IVF (Inverted File Index)**: partition vectors into clusters via k-means, search only the nearest clusters at query time. Lower memory than HNSW, slightly lower recall. Used by FAISS.
- **Product Quantization (PQ)**: compress vectors by splitting into subvectors and quantizing each. Reduces memory 4-8x at the cost of recall. Often combined with IVF (IVF-PQ).

## Purpose-Built Vector Databases

| System   | Storage        | Index type  | Differentiator                    |
| -------- | -------------- | ----------- | --------------------------------- |
| Qdrant   | Rust, disk+mem | HNSW        | Rich filtering, payload storage   |
| Pinecone | Managed SaaS   | Proprietary | Zero-ops, metadata filtering      |
| Weaviate | Go, hybrid     | HNSW        | Built-in vectorizers, GraphQL API |
| Milvus   | Go/C++         | IVF/HNSW    | Billion-scale, GPU acceleration   |
| ChromaDB | Python         | HNSW        | Developer-friendly, embedded mode |

## Embedded/Lightweight Alternatives

For desktop or edge applications where a full server is impractical:

- **SQLite + brute-force**: store embeddings as BLOBs, compute cosine similarity in application code. AudioX uses this approach — viable for <10k chunks with sub-10ms query latency.
- **sqlite-vss**: SQLite extension adding FAISS-backed ANN. Adds ~5MB binary size.
- **LanceDB**: embedded columnar store with built-in ANN (IVF-PQ). Rust-native, no server.
- **usearch**: single-header C++ ANN library with Rust/Python bindings. HNSW index in <1MB of code.

## Chunking Strategy

Embedding models have a token limit (typically 512 tokens). Long documents must be split into chunks before embedding. Common strategies:

- **Fixed-size**: split every N tokens with M-token overlap. Simple but may break mid-sentence.
- **Semantic**: split on paragraph or sentence boundaries, merge small segments to reach target size. Better retrieval quality.
- **Recursive**: split by largest delimiter first (double newline → newline → sentence → word), recursing until chunks fit the token budget.

AudioX uses ~384-word chunks (~512 tokens) split on segment boundaries to preserve semantic coherence.

## Example

```sql
SELECT
  chunks.document_id,
  chunks.chunk_index,
  chunks.content,
  documents.title
FROM chunks
JOIN documents ON documents.id = chunks.document_id
WHERE documents.source_type IN ('file_import', 'text_note')
ORDER BY chunks.chunk_index ASC
LIMIT 5;
```
