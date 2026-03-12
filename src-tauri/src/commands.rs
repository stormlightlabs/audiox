use super::{bootstrap, embedding, models, parsers, storage};
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;
use std::cmp::Ordering;
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::time::Duration;
use tauri::{Emitter, Manager};
use uuid::Uuid;

#[derive(Debug, Default)]
struct GeneratedMetadata {
    title: Option<String>,
    summary: Option<String>,
    tags: Vec<String>,
}

#[derive(Clone, Copy)]
enum MaxAttempts {
    Transcription,
    Metadata,
}

impl From<MaxAttempts> for usize {
    fn from(val: MaxAttempts) -> Self {
        match val {
            MaxAttempts::Transcription => 3,
            MaxAttempts::Metadata => 3,
        }
    }
}

impl MaxAttempts {
    pub fn value(self) -> usize {
        self.into()
    }
}

impl std::fmt::Display for MaxAttempts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = format!("{}", self.value());
        f.write_str(s.as_str())
    }
}

const RETRY_BASE_DELAY_MS: u64 = 500;

fn retry_delay(attempt: usize) -> Duration {
    let multiplier = u64::try_from(attempt).unwrap_or(1);
    Duration::from_millis(RETRY_BASE_DELAY_MS.saturating_mul(multiplier))
}

fn parse_whisper_thread_setting(raw: Option<String>) -> usize {
    raw.and_then(|value| value.trim().parse::<usize>().ok())
        .map(|threads| threads.clamp(models::WHISPER_MIN_THREADS, models::WHISPER_MAX_THREADS))
        .unwrap_or(models::WHISPER_DEFAULTS.threads)
}

fn read_setting_with_default(connection: &Connection, key: &str, default_value: &str) -> Result<String, String> {
    Ok(storage::read_setting(connection, key)?.unwrap_or_else(|| default_value.to_string()))
}

fn load_runtime_settings_from_connection(connection: &Connection) -> Result<models::AppSettings, String> {
    let whisper_model_raw = read_setting_with_default(
        connection,
        models::SETTING_KEY_WHISPER_MODEL,
        models::WHISPER_DEFAULTS.model_name,
    )?;
    let whisper_model = parsers::validate_whisper_model_name(&whisper_model_raw)
        .unwrap_or_else(|_| models::WHISPER_DEFAULTS.model_name.to_string());

    let whisper_language_raw = read_setting_with_default(
        connection,
        models::SETTING_KEY_WHISPER_LANGUAGE,
        models::WHISPER_LANGUAGE_AUTO,
    )?;
    let whisper_language = parsers::validate_whisper_language(&whisper_language_raw)
        .unwrap_or_else(|_| models::WHISPER_LANGUAGE_AUTO.to_string());

    let whisper_threads =
        parse_whisper_thread_setting(storage::read_setting(connection, models::SETTING_KEY_WHISPER_THREADS)?);

    let ollama_endpoint_raw = read_setting_with_default(
        connection,
        models::SETTING_KEY_OLLAMA_ENDPOINT,
        models::OLLAMA_DEFAULT_ENDPOINT,
    )?;
    let ollama_endpoint = parsers::normalize_ollama_endpoint(&ollama_endpoint_raw)
        .unwrap_or_else(|_| models::OLLAMA_DEFAULT_ENDPOINT.to_string());

    Ok(models::AppSettings { whisper_model, whisper_language, whisper_threads, ollama_endpoint })
}

fn load_runtime_settings(database_path: &Path) -> Result<models::AppSettings, String> {
    let connection = Connection::open(database_path)
        .map_err(|error| format!("failed to open database {}: {error}", database_path.display()))?;
    load_runtime_settings_from_connection(&connection)
}

fn whisper_model_name_from_file_name(file_name: &str) -> Option<String> {
    file_name
        .strip_prefix("ggml-")
        .and_then(|value| value.strip_suffix(".bin"))
        .map(ToString::to_string)
}

fn collect_whisper_model_inventory(
    app_data_dir: &Path, selected_model: &str,
) -> Result<models::WhisperModelInventory, String> {
    let models_dir = app_data_dir.join("models");
    let mut installed_models = Vec::new();
    let mut total_size_bytes = 0_u64;

    if models_dir.exists() {
        let entries = fs::read_dir(&models_dir)
            .map_err(|error| format!("failed to read models directory {}: {error}", models_dir.display()))?;
        for entry in entries {
            let entry = entry
                .map_err(|error| format!("failed to inspect models directory {}: {error}", models_dir.display()))?;
            let path = entry.path();
            if !path.is_file() {
                continue;
            }

            let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            let Some(model_name) = whisper_model_name_from_file_name(file_name) else {
                continue;
            };

            let size_bytes = path
                .metadata()
                .map_err(|error| format!("failed to inspect file {}: {error}", path.display()))?
                .len();
            total_size_bytes = total_size_bytes.saturating_add(size_bytes);
            installed_models.push(models::WhisperModelInfo {
                model_name,
                file_name: file_name.to_string(),
                size_bytes,
            });
        }
    }

    installed_models.sort_by(|left, right| left.model_name.cmp(&right.model_name));
    Ok(
        models::WhisperModelInventory {
            selected_model: selected_model.to_string(),
            installed_models,
            total_size_bytes,
        },
    )
}

fn fallback_summary(transcript: &str) -> Option<String> {
    let cleaned = transcript.split_whitespace().collect::<Vec<_>>().join(" ");
    if cleaned.is_empty() {
        return None;
    }

    let char_count = cleaned.chars().count();
    if char_count <= 240 {
        return Some(cleaned);
    }

    Some(format!("{}...", cleaned.chars().take(237).collect::<String>()))
}

/// TODO: embed this with [include_str!] or [include_bytes!]
fn metadata_prompt(transcript: &str) -> String {
    let clipped_transcript = transcript.chars().take(16_000).collect::<String>();
    format!(
        "You are an assistant that extracts structured metadata from a transcript.\n\
Return ONLY valid JSON with this exact shape:\n\
{{\"title\":\"...\",\"summary\":\"...\",\"tags\":[\"tag1\",\"tag2\",\"tag3\"]}}\n\
Rules:\n\
- title: concise and descriptive (max 12 words)\n\
- summary: exactly 2-3 sentences\n\
- tags: 3-7 short keywords, no hashtags\n\
\n\
Transcript:\n\
{clipped_transcript}"
    )
}

fn parse_generated_metadata(response_text: &str) -> GeneratedMetadata {
    let json_slice = response_text
        .find('{')
        .zip(response_text.rfind('}'))
        .and_then(|(start, end)| (start <= end).then_some(&response_text[start..=end]));

    let Some(payload) = json_slice else {
        return GeneratedMetadata::default();
    };

    let Ok(parsed) = serde_json::from_str::<Value>(payload) else {
        return GeneratedMetadata::default();
    };

    let title = parsed
        .get("title")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToString::to_string);

    let summary = parsed
        .get("summary")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToString::to_string);

    let tags = parsed
        .get("tags")
        .and_then(Value::as_array)
        .into_iter()
        .flat_map(|items| items.iter())
        .filter_map(Value::as_str)
        .map(ToString::to_string)
        .collect::<Vec<_>>();

    GeneratedMetadata { title, summary, tags: parsers::sanitize_tags(&tags) }
}

async fn resolve_generate_model_name(ollama_endpoint: &str) -> String {
    match bootstrap::fetch_ollama_model_names(ollama_endpoint).await {
        Ok(model_names) => {
            if let Some(model_name) = parsers::select_ollama_generate_model(&model_names) {
                if model_name != models::OllamaModel::GenerateDefault.as_str() {
                    log::info!(
                        "using Ollama model '{}' for metadata generation (default '{}' unavailable)",
                        model_name,
                        models::OllamaModel::GenerateDefault.as_str()
                    );
                }
                model_name
            } else {
                log::warn!(
                    "no installed '{}' model variants detected; falling back to '{}'",
                    models::OllamaModel::GenerateFamily.as_str(),
                    models::OllamaModel::GenerateDefault.as_str()
                );
                models::OllamaModel::GenerateDefault.to_string()
            }
        }
        Err(error) => {
            log::warn!(
                "failed to fetch Ollama model tags from {} before metadata generation: {}; falling back to '{}'",
                ollama_endpoint,
                error,
                models::OllamaModel::GenerateDefault.as_str()
            );
            models::OllamaModel::GenerateDefault.to_string()
        }
    }
}

fn emit_metadata_progress(
    app: &tauri::AppHandle, progress_event: models::ProgressEvent, status: &str, message: impl Into<String>,
    percent: f64,
) {
    let payload = models::TranscriptionProgress {
        status: status.to_string(),
        message: message.into(),
        percent: percent.clamp(0.0, 100.0),
    };
    let _ = app.emit(progress_event.as_str(), payload);
}

fn emit_embedding_setup_progress(app: &tauri::AppHandle, status: &str, message: impl Into<String>, percent: f64) {
    let payload = models::TranscriptionProgress {
        status: status.to_string(),
        message: message.into(),
        percent: percent.clamp(0.0, 100.0),
    };
    let _ = app.emit(models::ProgressEvent::SetupEmbedding.as_str(), payload);
}

async fn generate_metadata_once(
    client: &reqwest::Client, transcript: &str, generate_model: &str, ollama_endpoint: &str,
) -> Result<GeneratedMetadata, String> {
    let generate_url = models::OllamaUrl::Generate.url(ollama_endpoint);
    let generation_response = client
        .post(&generate_url)
        .json(&serde_json::json!({
            "model": generate_model,
            "prompt": metadata_prompt(transcript),
            "stream": false
        }))
        .send()
        .await
        .map_err(|error| {
            format!(
                "failed to call Ollama generate endpoint {} with model '{generate_model}': {error}",
                generate_url
            )
        })?;

    if !generation_response.status().is_success() {
        let status = generation_response.status();
        let body = generation_response.text().await.unwrap_or_default();
        return Err(format!(
            "ollama metadata generation failed for model '{generate_model}' with status {status}: {body}"
        ));
    }

    let generation_payload = generation_response
        .json::<Value>()
        .await
        .map_err(|error| format!("failed to parse ollama generate response for model '{generate_model}': {error}"))?;
    let generated_text = generation_payload
        .get("response")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("ollama generate response for model '{generate_model}' did not include text output"))?;
    Ok(parse_generated_metadata(generated_text))
}

async fn generate_metadata_with_retry(
    app: &tauri::AppHandle, client: &reqwest::Client, transcript: &str, progress_event: models::ProgressEvent,
    ollama_endpoint: &str,
) -> Result<(GeneratedMetadata, String), String> {
    let mut last_error = "metadata generation did not run".to_string();

    for attempt in 1..=MaxAttempts::Metadata.value() {
        let generate_model = resolve_generate_model_name(ollama_endpoint).await;
        emit_metadata_progress(
            app,
            progress_event,
            "running",
            format!(
                "Generating title, summary, and tags with {generate_model} (attempt {attempt}/{})...",
                MaxAttempts::Metadata.value()
            ),
            12.0 + ((attempt.saturating_sub(1) as f64) * 12.0),
        );
        match generate_metadata_once(client, transcript, &generate_model, ollama_endpoint).await {
            Ok(generated) => {
                emit_metadata_progress(
                    app,
                    progress_event,
                    "running",
                    format!("Metadata generated with {generate_model}. Preparing embeddings..."),
                    62.0,
                );
                return Ok((generated, generate_model));
            }
            Err(error) => {
                last_error = error;
                if attempt == MaxAttempts::Metadata.value() {
                    break;
                }

                let delay = retry_delay(attempt);
                log::warn!(
                    "metadata generation attempt {attempt}/{} failed (retry in {}ms): {}",
                    MaxAttempts::Metadata.value(),
                    delay.as_millis(),
                    last_error
                );
                tokio::time::sleep(delay).await;
            }
        }
    }

    let final_error = format!(
        "ollama metadata generation failed after {} attempts: {}",
        MaxAttempts::Metadata.value(),
        last_error
    );
    emit_metadata_progress(app, progress_event, "error", final_error.clone(), 0.0);
    Err(final_error)
}

fn cleanup_transcription_outputs(output_base: &Path) {
    for extension in ["json", "srt", "vtt", "txt"] {
        let output_path = output_base.with_extension(extension);
        if output_path.is_file() {
            let _ = std::fs::remove_file(output_path);
        }
    }
}

async fn run_whisper_transcription_with_retry(
    app: &tauri::AppHandle, whisper_program: &str, model_path: &Path, wav_path: &Path, output_base: &Path,
    language: &str, threads: usize,
) -> Result<Vec<models::TranscriptSegment>, String> {
    let mut last_error = "transcription did not run".to_string();

    for attempt in 1..=MaxAttempts::Transcription.value() {
        cleanup_transcription_outputs(output_base);
        match bootstrap::run_whisper_transcription(
            app,
            whisper_program,
            model_path,
            wav_path,
            output_base,
            language,
            threads,
        )
        .await
        {
            Ok(segments) => return Ok(segments),
            Err(error) => {
                last_error = error;
                if attempt == MaxAttempts::Transcription.value() {
                    break;
                }

                let delay = retry_delay(attempt);
                log::warn!(
                    "transcription attempt {attempt}/{} failed (retry in {}ms): {}",
                    MaxAttempts::Transcription.value(),
                    delay.as_millis(),
                    last_error
                );
                tokio::time::sleep(delay).await;
            }
        }
    }

    Err(format!(
        "whisper transcription failed after {} attempts: {}",
        MaxAttempts::Transcription.value(),
        last_error
    ))
}

fn managed_paths(app: &tauri::AppHandle) -> (std::path::PathBuf, std::path::PathBuf) {
    let state = storage::state_from_manager(app);
    (state.app_data_dir().to_path_buf(), state.database_path().to_path_buf())
}

fn apply_document_sort(documents: &mut [models::DocumentSummary], sort: models::DocumentSort) {
    match sort {
        models::DocumentSort::CreatedDesc => {
            documents.sort_by(|left, right| right.created_at.cmp(&left.created_at).then(right.id.cmp(&left.id)));
        }
        models::DocumentSort::CreatedAsc => {
            documents.sort_by(|left, right| left.created_at.cmp(&right.created_at).then(left.id.cmp(&right.id)));
        }
        models::DocumentSort::TitleAsc => {
            documents.sort_by(|left, right| {
                let left_title = left.title.to_ascii_lowercase();
                let right_title = right.title.to_ascii_lowercase();
                left_title.cmp(&right_title).then(left.id.cmp(&right.id))
            });
        }
        models::DocumentSort::TitleDesc => {
            documents.sort_by(|left, right| {
                let left_title = left.title.to_ascii_lowercase();
                let right_title = right.title.to_ascii_lowercase();
                right_title.cmp(&left_title).then(right.id.cmp(&left.id))
            });
        }
        models::DocumentSort::DurationAsc => {
            documents.sort_by(|left, right| {
                left.duration_seconds
                    .unwrap_or_default()
                    .cmp(&right.duration_seconds.unwrap_or_default())
                    .then(left.id.cmp(&right.id))
            });
        }
        models::DocumentSort::DurationDesc => {
            documents.sort_by(|left, right| {
                right
                    .duration_seconds
                    .unwrap_or_default()
                    .cmp(&left.duration_seconds.unwrap_or_default())
                    .then(right.id.cmp(&left.id))
            });
        }
    }
}

fn matches_all_tags(document_tags: &[String], filter_tags: &[String]) -> bool {
    if filter_tags.is_empty() {
        return true;
    }

    let tag_set = document_tags
        .iter()
        .map(|tag| tag.to_ascii_lowercase())
        .collect::<HashSet<_>>();
    filter_tags
        .iter()
        .all(|tag| tag_set.contains(&tag.to_ascii_lowercase()))
}

fn embedding_from_blob(blob: &[u8]) -> Result<Vec<f32>, String> {
    if !blob.len().is_multiple_of(4) {
        return Err(format!(
            "invalid embedding blob size {}; expected a multiple of 4",
            blob.len()
        ));
    }

    let mut embedding = Vec::with_capacity(blob.len() / 4);
    for chunk in blob.chunks_exact(4) {
        embedding.push(f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
    }

    if embedding.is_empty() {
        return Err("embedding blob decoded to an empty vector".to_string());
    }

    Ok(embedding)
}

fn embedding_to_blob(embedding: &[f32]) -> Vec<u8> {
    let mut blob = Vec::with_capacity(std::mem::size_of_val(embedding));
    for value in embedding {
        blob.extend_from_slice(&value.to_le_bytes());
    }
    blob
}

fn cosine_similarity(query: &[f32], candidate: &[f32]) -> Option<f64> {
    if query.len() != candidate.len() || query.is_empty() {
        return None;
    }

    let mut dot = 0_f64;
    let mut query_norm = 0_f64;
    let mut candidate_norm = 0_f64;
    for (query_value, candidate_value) in query.iter().zip(candidate.iter()) {
        let left = f64::from(*query_value);
        let right = f64::from(*candidate_value);
        dot += left * right;
        query_norm += left * left;
        candidate_norm += right * right;
    }

    if query_norm <= f64::EPSILON || candidate_norm <= f64::EPSILON {
        return None;
    }

    Some(dot / (query_norm.sqrt() * candidate_norm.sqrt()))
}

fn normalize_query_terms(query: &str) -> Vec<String> {
    query
        .split(|character: char| !character.is_ascii_alphanumeric())
        .map(str::trim)
        .filter(|term| term.len() >= 2)
        .map(|term| term.to_ascii_lowercase())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect()
}

fn is_path_within_root(path: &Path, root: &Path) -> bool {
    let canonical_root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let canonical_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    canonical_path.starts_with(canonical_root)
}

fn remove_file_if_owned(path: &Path, app_data_dir: &Path) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }

    if !is_path_within_root(path, app_data_dir) {
        return Ok(());
    }

    std::fs::remove_file(path).map_err(|error| format!("failed to delete {}: {error}", path.display()))
}

fn find_matching_segment_for_chunk(
    connection: &Connection, document_id: &str, chunk_content: &str, query_terms: &[String],
) -> Result<(Option<i64>, Option<i64>), String> {
    let mut statement = connection
        .prepare(
            "SELECT start_ms, end_ms, text
             FROM document_segments
             WHERE document_id = ?1
             ORDER BY start_ms ASC, end_ms ASC",
        )
        .map_err(|error| format!("failed to prepare segment lookup for {document_id}: {error}"))?;

    let rows = statement
        .query_map(params![document_id], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?, row.get::<_, String>(2)?))
        })
        .map_err(|error| format!("failed to query segments for {document_id}: {error}"))?;

    let chunk_text = chunk_content.to_ascii_lowercase();
    let mut fallback_match: Option<(i64, i64, usize)> = None;
    for row in rows {
        let (start_ms, end_ms, text) =
            row.map_err(|error| format!("failed to decode segment for {document_id}: {error}"))?;
        let segment_text = text.trim().to_ascii_lowercase();
        if segment_text.is_empty() {
            continue;
        }

        if segment_text.len() >= 8 && chunk_text.contains(&segment_text) {
            return Ok((Some(start_ms), Some(end_ms)));
        }

        let overlap_score = query_terms
            .iter()
            .filter(|term| segment_text.contains(term.as_str()))
            .count();
        if overlap_score == 0 {
            continue;
        }

        match fallback_match {
            Some((_, _, best_score)) if best_score >= overlap_score => {}
            _ => {
                fallback_match = Some((start_ms, end_ms, overlap_score));
            }
        }
    }

    Ok(fallback_match.map_or((None, None), |(start_ms, end_ms, _)| (Some(start_ms), Some(end_ms))))
}

async fn process_document_ai(
    app: &tauri::AppHandle, transcript: &str, segments: &[models::TranscriptSegment], fallback_title: &str,
    progress_event: models::ProgressEvent, ollama_endpoint: &str,
) -> Result<(String, Option<String>, Vec<String>, Vec<models::EmbeddedChunk>), String> {
    emit_metadata_progress(
        app,
        progress_event,
        "running",
        "Starting Gemma transcript enrichment...",
        5.0,
    );

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|error| format!("failed to initialize Ollama HTTP client: {error}"))?;
    let (generated, generate_model) =
        generate_metadata_with_retry(app, &client, transcript, progress_event, ollama_endpoint).await?;

    let title = generated
        .title
        .filter(|item| !item.trim().is_empty())
        .unwrap_or_else(|| fallback_title.to_string());
    let summary = generated.summary.or_else(|| fallback_summary(transcript));
    let tags = generated.tags;

    emit_metadata_progress(
        app,
        progress_event,
        "running",
        "Chunking transcript for embedding generation...",
        72.0,
    );
    let chunks = parsers::build_embedding_chunks(segments, transcript, models::EMBEDDING_CHUNK_TARGET_WORDS);
    if chunks.is_empty() {
        return Err("could not create transcript chunks for embeddings".to_string());
    }

    emit_metadata_progress(
        app,
        progress_event,
        "running",
        "Generating semantic embeddings locally...",
        82.0,
    );
    let embedding_state = app.state::<embedding::EmbeddingState>();
    let vectors = embedding_state.embed_chunks(&chunks)?;

    if vectors.len() != chunks.len() {
        return Err(format!(
            "local embedding model returned {} vectors for {} chunks",
            vectors.len(),
            chunks.len()
        ));
    }

    let embedded_chunks = chunks
        .into_iter()
        .zip(vectors)
        .enumerate()
        .map(|(index, (content, embedding))| models::EmbeddedChunk { chunk_index: index as i64, content, embedding })
        .collect::<Vec<_>>();

    emit_metadata_progress(
        app,
        progress_event,
        "completed",
        format!("Gemma enrichment complete with {generate_model}. Embeddings ready."),
        100.0,
    );

    Ok((title, summary, tags, embedded_chunks))
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn set_window_title(window: tauri::WebviewWindow, title: String) -> Result<(), String> {
    let next_title = title.trim();
    if next_title.is_empty() {
        return Err("title must not be empty".to_string());
    }

    window
        .set_title(next_title)
        .map_err(|error| format!("failed to set window title: {error}"))
}

#[tauri::command]
pub fn get_app_version() -> String {
    option_env!("AUDIOX_APP_VERSION")
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(env!("CARGO_PKG_VERSION"))
        .to_string()
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn get_app_settings(app: tauri::AppHandle) -> Result<models::AppSettings, String> {
    let (app_data_dir, database_path) = managed_paths(&app);
    storage::ensure_directory_layout(&app_data_dir)?;
    storage::initialize_database(&database_path)?;
    load_runtime_settings(&database_path)
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn save_app_settings(
    app: tauri::AppHandle, whisper_model: Option<String>, whisper_language: Option<String>,
    whisper_threads: Option<usize>, ollama_endpoint: Option<String>,
) -> Result<models::AppSettings, String> {
    let (app_data_dir, database_path) = managed_paths(&app);
    storage::ensure_directory_layout(&app_data_dir)?;
    storage::initialize_database(&database_path)?;

    let current_settings = load_runtime_settings(&database_path)?;
    let next_whisper_model = whisper_model
        .as_deref()
        .map(parsers::validate_whisper_model_name)
        .transpose()?
        .unwrap_or_else(|| current_settings.whisper_model.clone());
    let next_whisper_language = whisper_language
        .as_deref()
        .map(parsers::validate_whisper_language)
        .transpose()?
        .unwrap_or_else(|| current_settings.whisper_language.clone());
    let next_whisper_threads = match whisper_threads {
        Some(value) => {
            if !(models::WHISPER_MIN_THREADS..=models::WHISPER_MAX_THREADS).contains(&value) {
                return Err(format!(
                    "whisper_threads must be between {} and {}",
                    models::WHISPER_MIN_THREADS,
                    models::WHISPER_MAX_THREADS
                ));
            }
            value
        }
        None => current_settings.whisper_threads,
    };
    let next_ollama_endpoint = ollama_endpoint
        .as_deref()
        .map(parsers::normalize_ollama_endpoint)
        .transpose()?
        .unwrap_or_else(|| current_settings.ollama_endpoint.clone());

    storage::write_setting(
        &database_path,
        models::SETTING_KEY_WHISPER_MODEL,
        next_whisper_model.as_str(),
    )?;
    storage::write_setting(
        &database_path,
        models::SETTING_KEY_WHISPER_LANGUAGE,
        next_whisper_language.as_str(),
    )?;
    let next_whisper_threads_value = next_whisper_threads.to_string();
    storage::write_setting(
        &database_path,
        models::SETTING_KEY_WHISPER_THREADS,
        next_whisper_threads_value.as_str(),
    )?;
    storage::write_setting(
        &database_path,
        models::SETTING_KEY_OLLAMA_ENDPOINT,
        next_ollama_endpoint.as_str(),
    )?;

    Ok(models::AppSettings {
        whisper_model: next_whisper_model,
        whisper_language: next_whisper_language,
        whisper_threads: next_whisper_threads,
        ollama_endpoint: next_ollama_endpoint,
    })
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn list_whisper_models(app: tauri::AppHandle) -> Result<models::WhisperModelInventory, String> {
    let (app_data_dir, database_path) = managed_paths(&app);
    storage::ensure_directory_layout(&app_data_dir)?;
    storage::initialize_database(&database_path)?;
    let settings = load_runtime_settings(&database_path)?;
    collect_whisper_model_inventory(&app_data_dir, &settings.whisper_model)
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn delete_whisper_model(app: tauri::AppHandle, model: String) -> Result<models::WhisperModelInventory, String> {
    let (app_data_dir, database_path) = managed_paths(&app);
    storage::ensure_directory_layout(&app_data_dir)?;
    storage::initialize_database(&database_path)?;

    let model_name = parsers::validate_whisper_model_name(&model)?;
    let model_path = app_data_dir
        .join("models")
        .join(parsers::whisper_model_file_name(model_name.as_str()));
    if !model_path.is_file() {
        return Err(format!(
            "whisper model '{}' is not installed at {}",
            model_name,
            model_path.display()
        ));
    }

    fs::remove_file(&model_path)
        .map_err(|error| format!("failed to delete whisper model {}: {error}", model_path.display()))?;

    let current_settings = load_runtime_settings(&database_path)?;
    let mut selected_model = current_settings.whisper_model;
    if selected_model == model_name {
        let inventory = collect_whisper_model_inventory(&app_data_dir, selected_model.as_str())?;
        if inventory
            .installed_models
            .iter()
            .any(|entry| entry.model_name == models::WHISPER_DEFAULTS.model_name)
        {
            selected_model = models::WHISPER_DEFAULTS.model_name.to_string();
        } else if let Some(first) = inventory.installed_models.first() {
            selected_model = first.model_name.clone();
        } else {
            selected_model = models::WHISPER_DEFAULTS.model_name.to_string();
        }
        storage::write_setting(
            &database_path,
            models::SETTING_KEY_WHISPER_MODEL,
            selected_model.as_str(),
        )?;
    }

    collect_whisper_model_inventory(&app_data_dir, selected_model.as_str())
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub async fn check_ollama_connection(app: tauri::AppHandle) -> Result<models::OllamaConnectionStatus, String> {
    let (app_data_dir, database_path) = managed_paths(&app);
    storage::ensure_directory_layout(&app_data_dir)?;
    storage::initialize_database(&database_path)?;
    let settings = load_runtime_settings(&database_path)?;
    let endpoint = settings.ollama_endpoint.clone();

    match bootstrap::fetch_ollama_model_names(endpoint.as_str()).await {
        Ok(installed_models) => {
            let missing_models = parsers::missing_required_ollama_models(&installed_models);
            let message = if missing_models.is_empty() {
                "Ollama is reachable and required models are installed.".to_string()
            } else {
                format!(
                    "Ollama is reachable, but required models are missing: {}",
                    missing_models.join(", ")
                )
            };
            Ok(models::OllamaConnectionStatus { endpoint, reachable: true, installed_models, missing_models, message })
        }
        Err(error) => Ok(models::OllamaConnectionStatus {
            endpoint,
            reachable: false,
            installed_models: Vec::new(),
            missing_models: models::REQUIRED_OLLAMA_MODELS
                .iter()
                .map(|item| (*item).to_string())
                .collect(),
            message: error,
        }),
    }
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub async fn download_whisper_model(app: tauri::AppHandle, model: Option<String>) -> Result<String, String> {
    let (app_data_dir, database_path) = managed_paths(&app);
    storage::ensure_directory_layout(&app_data_dir)?;

    let model_name =
        parsers::validate_whisper_model_name(model.as_deref().unwrap_or(models::WHISPER_DEFAULTS.model_name))?;
    let model_path = bootstrap::download_whisper_model_file(&app, &app_data_dir, &model_name).await?;
    storage::initialize_database(&database_path)?;
    let setup_status = bootstrap::check_setup_state(&app_data_dir).await?;
    log::info!(
        "downloaded whisper model {} to {} (all_required_ready={})",
        model_name,
        model_path.display(),
        setup_status.all_required_ready
    );
    Ok(model_path.display().to_string())
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub async fn download_embedding_model(app: tauri::AppHandle) -> Result<(), String> {
    let (app_data_dir, database_path) = managed_paths(&app);
    storage::ensure_directory_layout(&app_data_dir)?;
    storage::initialize_database(&database_path)?;

    emit_embedding_setup_progress(&app, "running", "Preparing local embedding model download...", 8.0);
    let embedding_state = app.state::<embedding::EmbeddingState>();
    embedding_state.ensure_ready()?;
    emit_embedding_setup_progress(&app, "completed", "Local embedding model is ready.", 100.0);

    let setup_status = bootstrap::check_setup_state(&app_data_dir).await?;
    log::info!(
        "local embedding model ready at {} (all_required_ready={})",
        embedding_state.cache_dir().display(),
        setup_status.all_required_ready
    );
    Ok(())
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub async fn pull_ollama_model(app: tauri::AppHandle, model: String) -> Result<(), String> {
    let model_name = model.trim().to_string();
    if model_name.is_empty() {
        return Err("model_name must not be empty".to_string());
    }

    let (_, database_path) = managed_paths(&app);
    storage::initialize_database(&database_path)?;
    let settings = load_runtime_settings(&database_path)?;
    let pull_url = models::OllamaUrl::Pull.url(settings.ollama_endpoint.as_str());

    bootstrap::emit_ollama_progress(
        &app,
        &model_name,
        "running",
        format!("Starting pull for {model_name}"),
        0,
        0,
    );

    let client = reqwest::Client::builder()
        .build()
        .map_err(|error| format!("failed to initialize Ollama client: {error}"))?;
    let mut response = client
        .post(&pull_url)
        .json(&serde_json::json!({ "name": model_name, "stream": true }))
        .send()
        .await
        .map_err(|error| format!("failed to call Ollama pull API at {pull_url}: {error}"))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        let message = format!("Ollama pull failed with status {status}: {body}");
        bootstrap::emit_ollama_progress(&app, &model_name, "error", &message, 0, 0);
        return Err(message);
    }

    let mut buffer = String::new();
    let mut received_done = false;
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| format!("failed while receiving Ollama pull progress: {error}"))?
    {
        buffer.push_str(&String::from_utf8_lossy(&chunk));
        while let Some(newline_index) = buffer.find('\n') {
            let line = buffer[..newline_index].trim().to_string();
            buffer.drain(..=newline_index);
            if line.is_empty() {
                continue;
            }

            let (status, completed, total, done) = parsers::parse_ollama_progress_line(&line)?;
            let progress_status = if done { "completed" } else { "running" };
            bootstrap::emit_ollama_progress(&app, &model_name, progress_status, status, completed, total);
            if done {
                received_done = true;
            }
        }
    }

    let trailing = buffer.trim();
    if !trailing.is_empty() {
        let (status, completed, total, done) = parsers::parse_ollama_progress_line(trailing)?;
        let progress_status = if done { "completed" } else { "running" };
        bootstrap::emit_ollama_progress(&app, &model_name, progress_status, status, completed, total);
        if done {
            received_done = true;
        }
    }

    if !received_done {
        bootstrap::emit_ollama_progress(
            &app,
            &model_name,
            "completed",
            format!("Model {model_name} pull finished."),
            1,
            1,
        );
    }

    let (app_data_dir, _) = managed_paths(&app);
    let setup_status = bootstrap::check_setup_state(&app_data_dir).await?;
    log::info!(
        "pulled ollama model {} (missing_models_after_pull={})",
        model_name,
        setup_status.missing_ollama_models.join(",")
    );

    Ok(())
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn list_documents(
    app: tauri::AppHandle, sort: Option<String>, filter_tags: Option<Vec<String>>,
) -> Result<Vec<models::DocumentSummary>, String> {
    let (app_data_dir, database_path) = managed_paths(&app);
    storage::ensure_directory_layout(&app_data_dir)?;
    storage::initialize_database(&database_path)?;

    let connection = Connection::open(&database_path)
        .map_err(|error| format!("failed to open database {}: {error}", database_path.display()))?;
    let mut statement = connection
        .prepare(
            "SELECT id, title, summary, keywords, duration_seconds, created_at, updated_at
             FROM documents",
        )
        .map_err(|error| format!("failed to prepare list_documents query: {error}"))?;

    let rows = statement
        .query_map([], |row| {
            Ok(models::DocumentSummary {
                id: row.get(0)?,
                title: row.get(1)?,
                summary: row.get(2)?,
                tags: parsers::parse_keywords_csv(row.get::<_, Option<String>>(3)?.as_deref()),
                duration_seconds: row.get(4)?,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
            })
        })
        .map_err(|error| format!("failed to query documents: {error}"))?;

    let mut documents = Vec::new();
    for row in rows {
        documents.push(row.map_err(|error| format!("failed to decode document row: {error}"))?);
    }

    let requested_tags = filter_tags.unwrap_or_default();
    let tag_filter = parsers::sanitize_tags(&requested_tags);
    if !tag_filter.is_empty() {
        documents.retain(|document| matches_all_tags(&document.tags, &tag_filter));
    }

    apply_document_sort(&mut documents, models::DocumentSort::parse(sort.as_deref()));
    Ok(documents)
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn get_document(app: tauri::AppHandle, id: String) -> Result<models::DocumentDetail, String> {
    let document_id = id.trim();
    if document_id.is_empty() {
        return Err("id must not be empty".to_string());
    }

    let (app_data_dir, database_path) = managed_paths(&app);
    storage::ensure_directory_layout(&app_data_dir)?;
    storage::initialize_database(&database_path)?;

    let connection = Connection::open(&database_path)
        .map_err(|error| format!("failed to open database {}: {error}", database_path.display()))?;

    let mut document = connection
        .query_row(
            "SELECT id, title, summary, keywords, COALESCE(transcript, ''), audio_path, subtitle_srt_path, subtitle_vtt_path,
                    duration_seconds, created_at, updated_at
             FROM documents
             WHERE id = ?1",
            params![document_id],
            |row| {
                Ok(models::DocumentDetail {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    summary: row.get(2)?,
                    tags: parsers::parse_keywords_csv(row.get::<_, Option<String>>(3)?.as_deref()),
                    transcript: row.get(4)?,
                    audio_path: row.get(5)?,
                    subtitle_srt_path: row.get(6)?,
                    subtitle_vtt_path: row.get(7)?,
                    duration_seconds: row.get(8)?,
                    created_at: row.get(9)?,
                    updated_at: row.get(10)?,
                    segments: Vec::new(),
                })
            },
        )
        .optional()
        .map_err(|error| format!("failed to query document {document_id}: {error}"))?
        .ok_or_else(|| format!("document {document_id} was not found"))?;

    let mut segment_statement = connection
        .prepare(
            "SELECT start_ms, end_ms, text
             FROM document_segments
             WHERE document_id = ?1
             ORDER BY start_ms ASC, end_ms ASC",
        )
        .map_err(|error| format!("failed to prepare segments query: {error}"))?;
    let segment_rows = segment_statement
        .query_map(params![document_id], |row| {
            Ok(models::TranscriptSegment { start_ms: row.get(0)?, end_ms: row.get(1)?, text: row.get(2)? })
        })
        .map_err(|error| format!("failed to load document segments for {document_id}: {error}"))?;

    let mut segments = Vec::new();
    for row in segment_rows {
        segments.push(row.map_err(|error| format!("failed to decode segment row for {document_id}: {error}"))?);
    }
    document.segments = segments;
    Ok(document)
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn update_document(
    app: tauri::AppHandle, id: String, title: Option<String>, tags: Option<Vec<String>>,
) -> Result<models::DocumentDetail, String> {
    let document_id = id.trim().to_string();
    if document_id.is_empty() {
        return Err("id must not be empty".to_string());
    }

    let (app_data_dir, database_path) = managed_paths(&app);
    storage::ensure_directory_layout(&app_data_dir)?;
    storage::initialize_database(&database_path)?;

    let connection = Connection::open(&database_path)
        .map_err(|error| format!("failed to open database {}: {error}", database_path.display()))?;

    let existing = connection
        .query_row(
            "SELECT title, keywords FROM documents WHERE id = ?1",
            params![document_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
        )
        .optional()
        .map_err(|error| format!("failed to query document {}: {error}", document_id))?
        .ok_or_else(|| format!("document {} was not found", document_id))?;

    let next_title = match title {
        Some(value) => {
            let trimmed = value.trim().to_string();
            if trimmed.is_empty() {
                return Err("title must not be empty".to_string());
            }
            trimmed
        }
        None => existing.0,
    };

    let next_keywords = match tags {
        Some(values) => parsers::serialize_keywords_csv(&values),
        None => existing.1,
    };

    connection
        .execute(
            "UPDATE documents
             SET title = ?2, keywords = ?3, updated_at = ?4
             WHERE id = ?1",
            params![document_id, next_title, next_keywords, Utc::now().to_rfc3339()],
        )
        .map_err(|error| format!("failed to update document {}: {error}", document_id))?;

    get_document(app, document_id)
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub async fn enrich_document_metadata(app: tauri::AppHandle, id: String) -> Result<models::DocumentDetail, String> {
    let document_id = id.trim().to_string();
    if document_id.is_empty() {
        return Err("id must not be empty".to_string());
    }

    let current_document = get_document(app.clone(), document_id.clone())?;
    let transcript = current_document.transcript.trim().to_string();
    if transcript.is_empty() {
        return Err(format!(
            "document {document_id} has an empty transcript and cannot be enriched"
        ));
    }

    let fallback_title = current_document
        .title
        .trim()
        .to_string()
        .chars()
        .take(120)
        .collect::<String>();
    let fallback_title = if fallback_title.trim().is_empty() {
        format!("Document {}", current_document.id.chars().take(8).collect::<String>())
    } else {
        fallback_title
    };

    let (app_data_dir, database_path) = managed_paths(&app);
    storage::ensure_directory_layout(&app_data_dir)?;
    storage::initialize_database(&database_path)?;
    let runtime_settings = load_runtime_settings(&database_path)?;

    let (title, summary, tags, chunks) = process_document_ai(
        &app,
        &transcript,
        &current_document.segments,
        &fallback_title,
        models::ProgressEvent::DocumentMetadata,
        runtime_settings.ollama_endpoint.as_str(),
    )
    .await?;
    let keywords_csv = parsers::serialize_keywords_csv(&tags);

    let connection = Connection::open(&database_path)
        .map_err(|error| format!("failed to open database {}: {error}", database_path.display()))?;
    let transaction = connection
        .unchecked_transaction()
        .map_err(|error| format!("failed to start metadata transaction for {document_id}: {error}"))?;

    let now = Utc::now().to_rfc3339();
    let updated_rows = transaction
        .execute(
            "UPDATE documents
             SET title = ?2, summary = ?3, keywords = ?4, updated_at = ?5
             WHERE id = ?1",
            params![&document_id, title, summary, keywords_csv, &now],
        )
        .map_err(|error| format!("failed to update document metadata for {document_id}: {error}"))?;
    if updated_rows == 0 {
        return Err(format!("document {document_id} was not found"));
    }

    transaction
        .execute("DELETE FROM chunks WHERE document_id = ?1", params![&document_id])
        .map_err(|error| format!("failed to replace chunks for {document_id}: {error}"))?;

    let mut chunk_statement = transaction
        .prepare(
            "INSERT INTO chunks (document_id, chunk_index, content, embedding, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )
        .map_err(|error| format!("failed to prepare chunk update statement for {document_id}: {error}"))?;

    for chunk in chunks {
        chunk_statement
            .execute(params![
                &document_id,
                chunk.chunk_index,
                chunk.content,
                embedding_to_blob(&chunk.embedding),
                &now
            ])
            .map_err(|error| {
                format!(
                    "failed to persist chunk {} for {document_id}: {error}",
                    chunk.chunk_index
                )
            })?;
    }

    drop(chunk_statement);
    transaction
        .commit()
        .map_err(|error| format!("failed to commit metadata enrichment for {document_id}: {error}"))?;

    get_document(app, document_id)
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn delete_document(app: tauri::AppHandle, id: String) -> Result<(), String> {
    let document_id = id.trim().to_string();
    if document_id.is_empty() {
        return Err("id must not be empty".to_string());
    }

    let (app_data_dir, database_path) = managed_paths(&app);
    storage::ensure_directory_layout(&app_data_dir)?;
    storage::initialize_database(&database_path)?;

    let connection = Connection::open(&database_path)
        .map_err(|error| format!("failed to open database {}: {error}", database_path.display()))?;
    let transaction = connection
        .unchecked_transaction()
        .map_err(|error| format!("failed to start deletion transaction for {document_id}: {error}"))?;

    let (audio_path, subtitle_srt_path, subtitle_vtt_path) = transaction
        .query_row(
            "SELECT audio_path, subtitle_srt_path, subtitle_vtt_path
             FROM documents
             WHERE id = ?1",
            params![document_id],
            |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            },
        )
        .optional()
        .map_err(|error| format!("failed to load document {document_id} for deletion: {error}"))?
        .ok_or_else(|| format!("document {document_id} was not found"))?;

    transaction
        .execute("DELETE FROM documents WHERE id = ?1", params![document_id])
        .map_err(|error| format!("failed to delete document {document_id}: {error}"))?;
    transaction
        .commit()
        .map_err(|error| format!("failed to commit deletion for {document_id}: {error}"))?;

    let mut cleanup_paths = Vec::new();
    for value in [audio_path, subtitle_srt_path, subtitle_vtt_path].into_iter().flatten() {
        cleanup_paths.push(storage::resolve_storage_path(&app_data_dir, &value));
    }

    let audio_dir = app_data_dir.join("audio");
    let source_prefix = format!("{document_id}-");
    if let Ok(entries) = std::fs::read_dir(&audio_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(file_name) = path.file_name().and_then(|item| item.to_str()) else {
                continue;
            };
            if file_name.starts_with(&source_prefix) {
                cleanup_paths.push(path);
            }
        }
    }

    for path in cleanup_paths {
        if let Err(error) = remove_file_if_owned(&path, &app_data_dir) {
            log::warn!(
                "document {} deleted but failed to remove file {}: {}",
                document_id,
                path.display(),
                error
            );
        }
    }

    Ok(())
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub async fn search(
    app: tauri::AppHandle, query: String, limit: Option<usize>,
) -> Result<Vec<models::SearchResult>, String> {
    let trimmed_query = query.trim().to_string();
    if trimmed_query.is_empty() {
        return Ok(Vec::new());
    }

    let (app_data_dir, database_path) = managed_paths(&app);
    storage::ensure_directory_layout(&app_data_dir)?;
    storage::initialize_database(&database_path)?;

    let requested_limit = limit
        .unwrap_or_else(|| models::SearchLimit::Default.into())
        .clamp(1, models::SearchLimit::Max.into());
    let embedding_state = app.state::<embedding::EmbeddingState>();
    let query_embedding = embedding_state.embed_query(&trimmed_query)?;

    let connection = Connection::open(&database_path)
        .map_err(|error| format!("failed to open database {}: {error}", database_path.display()))?;

    let mut statement = connection
        .prepare(
            "SELECT
               chunks.document_id,
               chunks.chunk_index,
               chunks.content,
               chunks.embedding,
               documents.title,
               documents.summary,
               documents.keywords
             FROM chunks
             JOIN documents ON documents.id = chunks.document_id",
        )
        .map_err(|error| format!("failed to prepare semantic search query: {error}"))?;

    #[derive(Clone)]
    struct RankedChunk {
        document_id: String,
        chunk_index: i64,
        chunk_content: String,
        document_title: String,
        document_summary: Option<String>,
        document_tags: Vec<String>,
        similarity: f64,
    }

    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Vec<u8>>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, Option<String>>(6)?,
            ))
        })
        .map_err(|error| format!("failed to execute semantic search query: {error}"))?;

    let mut ranked = Vec::new();
    for row in rows {
        let (document_id, chunk_index, chunk_content, embedding_blob, document_title, document_summary, keywords_csv) =
            row.map_err(|error| format!("failed to decode semantic search row: {error}"))?;

        let embedding = match embedding_from_blob(&embedding_blob) {
            Ok(value) => value,
            Err(error) => {
                log::warn!(
                    "skipping chunk {}:{} due to invalid embedding: {}",
                    document_id,
                    chunk_index,
                    error
                );
                continue;
            }
        };

        let Some(similarity) = cosine_similarity(&query_embedding, &embedding) else {
            continue;
        };
        ranked.push(RankedChunk {
            document_id,
            chunk_index,
            chunk_content,
            document_title,
            document_summary,
            document_tags: parsers::parse_keywords_csv(keywords_csv.as_deref()),
            similarity,
        });
    }

    ranked.sort_by(|left, right| {
        right
            .similarity
            .partial_cmp(&left.similarity)
            .unwrap_or(Ordering::Equal)
            .then(right.document_id.cmp(&left.document_id))
            .then(right.chunk_index.cmp(&left.chunk_index))
    });

    let query_terms = normalize_query_terms(&trimmed_query);
    let mut results = Vec::new();
    for candidate in ranked.into_iter().take(requested_limit) {
        let (segment_start_ms, segment_end_ms) = find_matching_segment_for_chunk(
            &connection,
            &candidate.document_id,
            &candidate.chunk_content,
            &query_terms,
        )?;

        results.push(models::SearchResult {
            document_id: candidate.document_id,
            document_title: candidate.document_title,
            document_summary: candidate.document_summary,
            document_tags: candidate.document_tags,
            chunk_index: candidate.chunk_index,
            chunk_content: candidate.chunk_content,
            similarity: candidate.similarity,
            segment_start_ms,
            segment_end_ms,
        });
    }

    Ok(results)
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn initialize_app(app: tauri::AppHandle) -> Result<models::AppBootstrapResult, String> {
    storage::bootstrap_from_app(&app)
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub async fn preflight(app: tauri::AppHandle) -> Result<models::PreflightResult, String> {
    let (app_data_dir, database_path) = managed_paths(&app);

    storage::ensure_directory_layout(&app_data_dir)?;

    let ollama_endpoint = match storage::initialize_database(&database_path)
        .and_then(|_| load_runtime_settings(&database_path))
        .map(|settings| settings.ollama_endpoint)
    {
        Ok(endpoint) => endpoint,
        Err(error) => {
            log::warn!(
                "failed to load persisted Ollama endpoint for preflight: {}; using {}",
                error,
                models::OLLAMA_DEFAULT_ENDPOINT
            );
            models::OLLAMA_DEFAULT_ENDPOINT.to_string()
        }
    };

    let mut result = models::PreflightResult::default();

    match bootstrap::ensure_runtime_binary(&app_data_dir, &models::WHISPER_BINARY_SPEC).await {
        Ok(message) => bootstrap::record_preflight_detail(
            &app,
            &mut result,
            models::WHISPER_BINARY_SPEC.check,
            models::CheckStatus::Pass,
            message,
        ),
        Err(error) => bootstrap::record_preflight_detail(
            &app,
            &mut result,
            models::WHISPER_BINARY_SPEC.check,
            models::CheckStatus::Fail,
            error,
        ),
    }

    match bootstrap::ensure_runtime_binary(&app_data_dir, &models::FFMPEG_BINARY_SPEC).await {
        Ok(message) => bootstrap::record_preflight_detail(
            &app,
            &mut result,
            models::FFMPEG_BINARY_SPEC.check,
            models::CheckStatus::Pass,
            message,
        ),
        Err(error) => bootstrap::record_preflight_detail(
            &app,
            &mut result,
            models::FFMPEG_BINARY_SPEC.check,
            models::CheckStatus::Fail,
            error,
        ),
    }

    match bootstrap::ensure_runtime_binary(&app_data_dir, &models::YT_DLP_BINARY_SPEC).await {
        Ok(message) => bootstrap::record_preflight_detail(
            &app,
            &mut result,
            models::YT_DLP_BINARY_SPEC.check,
            models::CheckStatus::Pass,
            message,
        ),
        Err(error) => bootstrap::record_preflight_detail(
            &app,
            &mut result,
            models::YT_DLP_BINARY_SPEC.check,
            models::CheckStatus::Warn,
            format!("{error} URL import remains disabled until yt-dlp is available."),
        ),
    }

    let whisper_model_missing = match storage::whisper_model_present(&app_data_dir.join("models")) {
        Ok(true) => {
            bootstrap::record_preflight_detail(
                &app,
                &mut result,
                models::PreflightCheck::WhisperModel,
                models::CheckStatus::Pass,
                "whisper model files are present.",
            );
            false
        }
        Ok(false) => {
            bootstrap::record_preflight_detail(
                &app,
                &mut result,
                models::PreflightCheck::WhisperModel,
                models::CheckStatus::Fail,
                "No whisper model found in appdata/models. Open setup to download ggml-base.en.bin.",
            );
            true
        }
        Err(error) => {
            bootstrap::record_preflight_detail(
                &app,
                &mut result,
                models::PreflightCheck::WhisperModel,
                models::CheckStatus::Fail,
                error,
            );
            false
        }
    };

    let embedding_model_missing = match storage::embedding_model_present(&app_data_dir.join("models").join("embed")) {
        Ok(true) => {
            bootstrap::record_preflight_detail(
                &app,
                &mut result,
                models::PreflightCheck::EmbeddingModel,
                models::CheckStatus::Pass,
                "Local embedding model files are present.",
            );
            false
        }
        Ok(false) => {
            bootstrap::record_preflight_detail(
                &app,
                &mut result,
                models::PreflightCheck::EmbeddingModel,
                models::CheckStatus::Warn,
                "Local embedding model is missing in appdata/models/embed. Open setup to download it.",
            );
            true
        }
        Err(error) => {
            bootstrap::record_preflight_detail(
                &app,
                &mut result,
                models::PreflightCheck::EmbeddingModel,
                models::CheckStatus::Warn,
                format!("{error} Semantic search model can be downloaded from setup."),
            );
            true
        }
    };

    match bootstrap::fetch_ollama_model_names(ollama_endpoint.as_str()).await {
        Ok(models) => {
            bootstrap::record_preflight_detail(
                &app,
                &mut result,
                models::PreflightCheck::OllamaServer,
                models::CheckStatus::Pass,
                "Ollama server is reachable.",
            );
            let missing_models = parsers::missing_required_ollama_models(&models);
            if missing_models.is_empty() {
                bootstrap::record_preflight_detail(
                    &app,
                    &mut result,
                    models::PreflightCheck::OllamaModels,
                    models::CheckStatus::Pass,
                    "Required Ollama generate models are available.",
                );
            } else {
                bootstrap::record_preflight_detail(
                    &app,
                    &mut result,
                    models::PreflightCheck::OllamaModels,
                    models::CheckStatus::Warn,
                    format!(
                        "Missing Ollama models for metadata generation: {}. Pull them with `ollama pull <model>`.",
                        missing_models.join(", ")
                    ),
                );
            }
        }
        Err(error) => {
            bootstrap::record_preflight_detail(
                &app,
                &mut result,
                models::PreflightCheck::OllamaServer,
                models::CheckStatus::Warn,
                format!(
                    "{error} Start Ollama with `ollama serve` to enable title/summary/tag generation. Endpoint: {}",
                    ollama_endpoint
                ),
            );
            bootstrap::record_preflight_detail(
                &app,
                &mut result,
                models::PreflightCheck::OllamaModels,
                models::CheckStatus::Warn,
                "Required Ollama models could not be verified because the server is unavailable.",
            );
        }
    }

    match storage::initialize_database(&database_path) {
        Ok(_) => bootstrap::record_preflight_detail(
            &app,
            &mut result,
            models::PreflightCheck::Database,
            models::CheckStatus::Pass,
            "SQLite database is accessible and migrations are current.",
        ),
        Err(error) => bootstrap::record_preflight_detail(
            &app,
            &mut result,
            models::PreflightCheck::Database,
            models::CheckStatus::Fail,
            error,
        ),
    }

    let setup_dependencies_ready = !whisper_model_missing && !embedding_model_missing;
    result.should_open_setup = !setup_dependencies_ready;
    result.all_required_passed = bootstrap::compute_all_required_passed(&result);
    storage::set_setup_completed(&database_path, setup_dependencies_ready)?;
    log::info!(
        "preflight finished: all_required_passed={}, setup_dependencies_ready={}, should_open_setup={}",
        result.all_required_passed,
        setup_dependencies_ready,
        result.should_open_setup
    );

    Ok(result)
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub async fn import_audio_file(app: tauri::AppHandle, source_path: String) -> Result<models::ImportedDocument, String> {
    let source_trimmed = source_path.trim();
    if source_trimmed.is_empty() {
        return Err("source_path must not be empty".to_string());
    }

    let source = std::path::PathBuf::from(source_trimmed);
    parsers::ensure_supported_import_path(&source)?;

    let (app_data_dir, database_path) = managed_paths(&app);
    storage::ensure_directory_layout(&app_data_dir)?;
    storage::initialize_database(&database_path)?;
    let runtime_settings = load_runtime_settings(&database_path)?;

    let ffmpeg_program = bootstrap::resolve_runtime_binary_program(&app_data_dir, &models::FFMPEG_BINARY_SPEC).await?;
    let whisper_program =
        bootstrap::resolve_runtime_binary_program(&app_data_dir, &models::WHISPER_BINARY_SPEC).await?;
    let model_path =
        storage::resolve_whisper_model_path_for(&app_data_dir, Some(runtime_settings.whisper_model.as_str()))?;

    let document_id = Uuid::new_v4().to_string();
    let extension = parsers::extension_for_path(&source)
        .ok_or_else(|| format!("failed to determine extension for {}", source.display()))?;
    let copied_source_path = app_data_dir
        .join("audio")
        .join(format!("{document_id}-source.{extension}"));
    std::fs::copy(&source, &copied_source_path)
        .map_err(|error| format!("failed to copy source audio into app data: {error}"))?;

    let converted_wav_path = app_data_dir.join("audio").join(format!("{document_id}.wav"));
    bootstrap::run_ffmpeg_conversion(&app, &ffmpeg_program, &copied_source_path, &converted_wav_path).await?;

    let subtitle_base = app_data_dir.join("subtitles").join(&document_id);
    let segments = run_whisper_transcription_with_retry(
        &app,
        &whisper_program,
        &model_path,
        &converted_wav_path,
        &subtitle_base,
        runtime_settings.whisper_language.as_str(),
        runtime_settings.whisper_threads,
    )
    .await?;
    if segments.is_empty() {
        return Err("whisper transcription did not return any transcript segments".to_string());
    }

    let subtitle_srt_path = subtitle_base.with_extension("srt");
    let subtitle_vtt_path = subtitle_base.with_extension("vtt");
    if !subtitle_srt_path.is_file() {
        return Err(format!(
            "whisper did not generate expected subtitle file {}",
            subtitle_srt_path.display()
        ));
    }
    if !subtitle_vtt_path.is_file() {
        return Err(format!(
            "whisper did not generate expected subtitle file {}",
            subtitle_vtt_path.display()
        ));
    }

    let transcript = parsers::build_transcript_text(&segments);
    let duration_seconds = parsers::max_duration_seconds(&segments);
    let fallback_title = source
        .file_stem()
        .and_then(|stem| stem.to_str())
        .map(str::trim)
        .filter(|stem| !stem.is_empty())
        .unwrap_or("Imported audio")
        .to_string();
    let (title, summary, tags, chunks) = process_document_ai(
        &app,
        &transcript,
        &segments,
        &fallback_title,
        models::ProgressEvent::ImportMetadata,
        runtime_settings.ollama_endpoint.as_str(),
    )
    .await?;
    let keywords_csv = parsers::serialize_keywords_csv(&tags);

    let audio_path = storage::path_for_storage(&converted_wav_path, &app_data_dir);
    let subtitle_srt = storage::path_for_storage(&subtitle_srt_path, &app_data_dir);
    let subtitle_vtt = storage::path_for_storage(&subtitle_vtt_path, &app_data_dir);
    let source_uri = source.to_string_lossy().to_string();

    storage::persist_document(
        &database_path,
        &storage::PersistDocumentInput {
            document_id: &document_id,
            source_type: "file_import",
            title: &title,
            summary: summary.as_deref(),
            keywords_csv: keywords_csv.as_deref(),
            source_uri: &source_uri,
            transcript: &transcript,
            audio_path: &audio_path,
            subtitle_srt_path: &subtitle_srt,
            subtitle_vtt_path: &subtitle_vtt,
            duration_seconds,
            segments: &segments,
            chunks: &chunks,
        },
    )?;

    let created_at = Utc::now().to_rfc3339();
    Ok(models::ImportedDocument {
        id: document_id,
        title,
        summary,
        tags,
        transcript,
        audio_path,
        subtitle_srt_path: subtitle_srt,
        subtitle_vtt_path: subtitle_vtt,
        duration_seconds,
        created_at,
        segments,
    })
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub async fn import_recorded_audio(
    app: tauri::AppHandle, source_path: String,
) -> Result<models::ImportedDocument, String> {
    let source_trimmed = source_path.trim();
    if source_trimmed.is_empty() {
        return Err("source_path must not be empty".to_string());
    }
    let source = std::path::PathBuf::from(source_trimmed);
    parsers::ensure_supported_import_path(&source)?;

    let (app_data_dir, database_path) = managed_paths(&app);
    storage::ensure_directory_layout(&app_data_dir)?;
    storage::initialize_database(&database_path)?;
    let runtime_settings = load_runtime_settings(&database_path)?;

    let ffmpeg_program = bootstrap::resolve_runtime_binary_program(&app_data_dir, &models::FFMPEG_BINARY_SPEC).await?;
    let whisper_program =
        bootstrap::resolve_runtime_binary_program(&app_data_dir, &models::WHISPER_BINARY_SPEC).await?;
    let model_path =
        storage::resolve_whisper_model_path_for(&app_data_dir, Some(runtime_settings.whisper_model.as_str()))?;

    let document_id = Uuid::new_v4().to_string();
    let recording_extension = parsers::extension_for_path(&source).unwrap_or_else(|| "wav".to_string());
    let converted_wav_path = app_data_dir.join("audio").join(format!("{document_id}.wav"));
    bootstrap::run_ffmpeg_conversion(&app, &ffmpeg_program, &source, &converted_wav_path).await?;
    if source != converted_wav_path {
        let _ = remove_file_if_owned(&source, &app_data_dir);
    }

    let subtitle_base = app_data_dir.join("subtitles").join(&document_id);
    let segments = run_whisper_transcription_with_retry(
        &app,
        &whisper_program,
        &model_path,
        &converted_wav_path,
        &subtitle_base,
        runtime_settings.whisper_language.as_str(),
        runtime_settings.whisper_threads,
    )
    .await?;
    if segments.is_empty() {
        return Err("whisper transcription did not return any transcript segments".to_string());
    }

    let subtitle_srt_path = subtitle_base.with_extension("srt");
    let subtitle_vtt_path = subtitle_base.with_extension("vtt");
    if !subtitle_srt_path.is_file() {
        return Err(format!(
            "whisper did not generate expected subtitle file {}",
            subtitle_srt_path.display()
        ));
    }
    if !subtitle_vtt_path.is_file() {
        return Err(format!(
            "whisper did not generate expected subtitle file {}",
            subtitle_vtt_path.display()
        ));
    }

    let transcript = parsers::build_transcript_text(&segments);
    let duration_seconds = parsers::max_duration_seconds(&segments);
    let fallback_title = format!("Recording {}", Utc::now().format("%Y-%m-%d %H:%M UTC"));
    let (title, summary, tags, chunks) = process_document_ai(
        &app,
        &transcript,
        &segments,
        &fallback_title,
        models::ProgressEvent::ImportMetadata,
        runtime_settings.ollama_endpoint.as_str(),
    )
    .await?;
    let keywords_csv = parsers::serialize_keywords_csv(&tags);

    let audio_path = storage::path_for_storage(&converted_wav_path, &app_data_dir);
    let subtitle_srt = storage::path_for_storage(&subtitle_srt_path, &app_data_dir);
    let subtitle_vtt = storage::path_for_storage(&subtitle_vtt_path, &app_data_dir);
    let source_uri = format!("microphone://{recording_extension}");

    storage::persist_document(
        &database_path,
        &storage::PersistDocumentInput {
            document_id: &document_id,
            source_type: "microphone_recording",
            title: &title,
            summary: summary.as_deref(),
            keywords_csv: keywords_csv.as_deref(),
            source_uri: &source_uri,
            transcript: &transcript,
            audio_path: &audio_path,
            subtitle_srt_path: &subtitle_srt,
            subtitle_vtt_path: &subtitle_vtt,
            duration_seconds,
            segments: &segments,
            chunks: &chunks,
        },
    )?;

    let created_at = Utc::now().to_rfc3339();
    Ok(models::ImportedDocument {
        id: document_id,
        title,
        summary,
        tags,
        transcript,
        audio_path,
        subtitle_srt_path: subtitle_srt,
        subtitle_vtt_path: subtitle_vtt,
        duration_seconds,
        created_at,
        segments,
    })
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub async fn check_setup(app: tauri::AppHandle) -> Result<models::SetupStatus, String> {
    let (app_data_dir, _) = managed_paths(&app);

    let setup = bootstrap::check_setup_state(&app_data_dir).await?;
    log::info!(
        "setup status: whisper_ready={}, embedding_ready={}, ollama_server_ready={}, missing_models={}, completed={}",
        setup.whisper_model_ready,
        setup.embedding_model_ready,
        setup.ollama_server_ready,
        setup.missing_ollama_models.join(","),
        setup.setup_completed
    );
    Ok(setup)
}
