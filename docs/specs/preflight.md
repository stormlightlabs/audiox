# Preflight Checks & Splash Screen

On every launch the app shows a splash screen while running preflight checks. The splash is the first thing the user sees — it validates that all runtime dependencies are available before showing the main UI.

## Check Sequence

1. **Executable dependencies** — ensure `whisper-cli`, `ffmpeg`, and `yt-dlp` are executable:
   - sidecar → managed runtime cache → PATH
2. **Whisper model** — check that at least one model file exists in `appdata/models/`
3. **Embedding model** — check that fastembed model files exist in `appdata/models/embed/` (warn if missing — auto-downloads on first use)
4. **Ollama server** — `GET http://localhost:11434/api/tags`, timeout 3s (warn if unavailable — only needed for document creation)
5. **Ollama models** — parse `/api/tags` response, confirm `gemma3:4b` is present (warn if missing — only needed for document creation)
6. **Database** — open or create `appdata/db/audiox.db`, run migrations if schema version is stale

## Status Reporting

Each check reports one of three states to the frontend via Tauri events:

| State  | Meaning                                                       |
| ------ | ------------------------------------------------------------- |
| `pass` | Dependency is ready                                           |
| `fail` | Missing or broken — show actionable guidance                  |
| `warn` | Optional dependency missing (e.g., yt-dlp) — app can continue |

## Splash UI

- App logo + name centered
- Animated checklist (solid-motionone staggered entrance) showing each check with a spinner → checkmark/cross
- If all pass: auto-transition to Library view after a short delay
- If any fail: remain on splash, show inline guidance (e.g., "Ollama is not running. Start it with `ollama serve` or install from ollama.com") with a retry button
- First-run scenario: if whisper model or Ollama models are missing, transition to the Setup wizard (M2) instead

## Tauri Command

```rust
#[tauri::command]
async fn preflight(app: tauri::AppHandle) -> Result<PreflightResult, String> {
    // Returns status for each dependency
}
```

```typescript
type PreflightResult = {
  whisper_cli: CheckStatus; // sidecar-first executable check
  ffmpeg: CheckStatus; // sidecar-first executable check
  yt_dlp: CheckStatus; // optional executable check (warn if missing)
  whisper_model: CheckStatus; // model file
  embedding_model: CheckStatus; // local fastembed model (warn if missing, auto-downloads)
  ollama_server: CheckStatus; // server reachable (warn if missing — only for doc creation)
  ollama_models: CheckStatus; // gemma3:4b present (warn if missing — only for doc creation)
  database: CheckStatus; // db accessible
};
type CheckStatus = "pass" | "fail" | "warn";
```
