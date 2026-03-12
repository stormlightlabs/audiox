use super::{bootstrap, models, parsers, storage};
use rusqlite::{params, Connection, OptionalExtension};
use tauri::Manager;
use uuid::Uuid;

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub async fn download_whisper_model(app: tauri::AppHandle, model: Option<String>) -> Result<String, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("failed to resolve app data directory: {error}"))?;
    storage::ensure_directory_layout(&app_data_dir)?;

    let model_name =
        parsers::validate_whisper_model_name(model.as_deref().unwrap_or(models::DEFAULT_WHISPER_MODEL_NAME))?;
    let model_path = bootstrap::download_whisper_model_file(&app, &app_data_dir, &model_name).await?;
    let database_path = storage::database_path_from_app_data(&app_data_dir);
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
pub async fn pull_ollama_model(app: tauri::AppHandle, model: String) -> Result<(), String> {
    let model_name = model.trim().to_string();
    if model_name.is_empty() {
        return Err("model_name must not be empty".to_string());
    }

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
        .post(models::OLLAMA_PULL_URL)
        .json(&serde_json::json!({ "name": model_name, "stream": true }))
        .send()
        .await
        .map_err(|error| format!("failed to call Ollama pull API at {}: {error}", models::OLLAMA_PULL_URL))?;

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

    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("failed to resolve app data directory: {error}"))?;
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
pub fn list_documents(app: tauri::AppHandle) -> Result<Vec<models::DocumentSummary>, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("failed to resolve app data directory: {error}"))?;
    storage::ensure_directory_layout(&app_data_dir)?;

    let database_path = storage::database_path_from_app_data(&app_data_dir);
    storage::initialize_database(&database_path)?;

    let connection = Connection::open(&database_path)
        .map_err(|error| format!("failed to open database {}: {error}", database_path.display()))?;
    let mut statement = connection
        .prepare(
            "SELECT id, title, summary, duration_seconds, created_at, updated_at
             FROM documents
             ORDER BY datetime(created_at) DESC, created_at DESC",
        )
        .map_err(|error| format!("failed to prepare list_documents query: {error}"))?;

    let rows = statement
        .query_map([], |row| {
            Ok(models::DocumentSummary {
                id: row.get(0)?,
                title: row.get(1)?,
                summary: row.get(2)?,
                duration_seconds: row.get(3)?,
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
            })
        })
        .map_err(|error| format!("failed to query documents: {error}"))?;

    let mut documents = Vec::new();
    for row in rows {
        documents.push(row.map_err(|error| format!("failed to decode document row: {error}"))?);
    }
    Ok(documents)
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn get_document(app: tauri::AppHandle, id: String) -> Result<models::DocumentDetail, String> {
    let document_id = id.trim();
    if document_id.is_empty() {
        return Err("id must not be empty".to_string());
    }

    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("failed to resolve app data directory: {error}"))?;
    storage::ensure_directory_layout(&app_data_dir)?;

    let database_path = storage::database_path_from_app_data(&app_data_dir);
    storage::initialize_database(&database_path)?;

    let connection = Connection::open(&database_path)
        .map_err(|error| format!("failed to open database {}: {error}", database_path.display()))?;

    let mut document = connection
        .query_row(
            "SELECT id, title, summary, COALESCE(transcript, ''), audio_path, subtitle_srt_path, subtitle_vtt_path,
                    duration_seconds, created_at, updated_at
             FROM documents
             WHERE id = ?1",
            params![document_id],
            |row| {
                Ok(models::DocumentDetail {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    summary: row.get(2)?,
                    transcript: row.get(3)?,
                    audio_path: row.get(4)?,
                    subtitle_srt_path: row.get(5)?,
                    subtitle_vtt_path: row.get(6)?,
                    duration_seconds: row.get(7)?,
                    created_at: row.get(8)?,
                    updated_at: row.get(9)?,
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
pub fn initialize_app(app: tauri::AppHandle) -> Result<models::AppBootstrapResult, String> {
    storage::bootstrap_from_app(&app)
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub async fn preflight(app: tauri::AppHandle) -> Result<models::PreflightResult, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("failed to resolve app data directory: {error}"))?;

    storage::ensure_directory_layout(&app_data_dir)?;

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

    let mut ollama_models_missing = false;
    match bootstrap::fetch_ollama_model_names().await {
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
                    "Required Ollama models are available.",
                );
            } else {
                ollama_models_missing = true;
                bootstrap::record_preflight_detail(
                    &app,
                    &mut result,
                    models::PreflightCheck::OllamaModels,
                    models::CheckStatus::Fail,
                    format!(
                        "Missing Ollama models: {}. Pull them with `ollama pull <model>`.",
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
                models::CheckStatus::Fail,
                format!("{error} Start Ollama with `ollama serve`."),
            );
            bootstrap::record_preflight_detail(
                &app,
                &mut result,
                models::PreflightCheck::OllamaModels,
                models::CheckStatus::Fail,
                "Required Ollama models could not be verified because the server is unavailable.",
            );
        }
    }

    let database_path = storage::database_path_from_app_data(&app_data_dir);
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

    let setup_dependencies_ready = !whisper_model_missing && !ollama_models_missing;
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

    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("failed to resolve app data directory: {error}"))?;
    storage::ensure_directory_layout(&app_data_dir)?;
    let database_path = storage::database_path_from_app_data(&app_data_dir);
    storage::initialize_database(&database_path)?;

    let ffmpeg_program = bootstrap::resolve_runtime_binary_program(&app_data_dir, &models::FFMPEG_BINARY_SPEC).await?;
    let whisper_program =
        bootstrap::resolve_runtime_binary_program(&app_data_dir, &models::WHISPER_BINARY_SPEC).await?;
    let model_path = storage::resolve_whisper_model_path(&app_data_dir)?;

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
    let segments =
        bootstrap::run_whisper_transcription(&app, &whisper_program, &model_path, &converted_wav_path, &subtitle_base)
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
    let title = source
        .file_stem()
        .and_then(|stem| stem.to_str())
        .map(str::trim)
        .filter(|stem| !stem.is_empty())
        .unwrap_or("Imported audio")
        .to_string();

    let audio_path = storage::path_for_storage(&converted_wav_path, &app_data_dir);
    let subtitle_srt = storage::path_for_storage(&subtitle_srt_path, &app_data_dir);
    let subtitle_vtt = storage::path_for_storage(&subtitle_vtt_path, &app_data_dir);
    let source_uri = source.to_string_lossy().to_string();

    storage::persist_document(
        &database_path,
        &storage::PersistDocumentInput {
            document_id: &document_id,
            title: &title,
            source_uri: &source_uri,
            transcript: &transcript,
            audio_path: &audio_path,
            subtitle_srt_path: &subtitle_srt,
            subtitle_vtt_path: &subtitle_vtt,
            duration_seconds,
            segments: &segments,
        },
    )?;

    let created_at = chrono::Utc::now().to_rfc3339();
    Ok(models::ImportedDocument {
        id: document_id,
        title,
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
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("failed to resolve app data directory: {error}"))?;

    let setup = bootstrap::check_setup_state(&app_data_dir).await?;
    log::info!(
        "setup status: whisper_ready={}, ollama_server_ready={}, missing_models={}, completed={}",
        setup.whisper_model_ready,
        setup.ollama_server_ready,
        setup.missing_ollama_models.join(","),
        setup.setup_completed
    );
    Ok(setup)
}
