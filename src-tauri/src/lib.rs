use chrono::Utc;
use regex::Regex;
use reqwest::StatusCode;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::ffi::OsStr;
use std::fs;
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tauri::{Emitter, Manager};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use uuid::Uuid;

const REQUIRED_DIRECTORIES: [&str; 6] = ["models", "audio", "video", "subtitles", "db", "bin"];
const REQUIRED_OLLAMA_MODELS: [&str; 2] = ["nomic-embed-text", "gemma3:4b"];
const SCHEMA_VERSION: i64 = 2;
const PREFLIGHT_EVENT: &str = "preflight://check";
const SETUP_WHISPER_PROGRESS_EVENT: &str = "setup://whisper-progress";
const SETUP_OLLAMA_PROGRESS_EVENT: &str = "setup://ollama-progress";
const IMPORT_CONVERSION_PROGRESS_EVENT: &str = "import://conversion-progress";
const IMPORT_TRANSCRIPTION_PROGRESS_EVENT: &str = "import://transcription-progress";
const OLLAMA_TAGS_URL: &str = "http://localhost:11434/api/tags";
const OLLAMA_PULL_URL: &str = "http://localhost:11434/api/pull";
const DEFAULT_WHISPER_MODEL_NAME: &str = "base.en";
const DEFAULT_WHISPER_THREADS: usize = 4;
const COMMAND_TIMEOUT_SECONDS: u64 = 8;
const DOWNLOAD_TIMEOUT_SECONDS: u64 = 120;
const ALLOWED_IMPORT_EXTENSIONS: [&str; 7] = ["mp3", "m4a", "wav", "flac", "ogg", "opus", "webm"];

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AppBootstrapResult {
    app_data_dir: String,
    database_path: String,
    created_directories: Vec<String>,
    schema_version: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
enum CheckStatus {
    Pass,
    Fail,
    Warn,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum PreflightCheck {
    WhisperCli,
    Ffmpeg,
    YtDlp,
    WhisperModel,
    OllamaServer,
    OllamaModels,
    Database,
}

#[derive(Clone, Debug, Serialize)]
struct PreflightCheckDetail {
    check: PreflightCheck,
    status: CheckStatus,
    message: String,
}

#[derive(Clone, Debug, Serialize)]
struct PreflightResult {
    whisper_cli: CheckStatus,
    ffmpeg: CheckStatus,
    yt_dlp: CheckStatus,
    whisper_model: CheckStatus,
    ollama_server: CheckStatus,
    ollama_models: CheckStatus,
    database: CheckStatus,
    should_open_setup: bool,
    all_required_passed: bool,
    details: Vec<PreflightCheckDetail>,
}

#[derive(Clone, Debug, Serialize)]
struct SetupStatus {
    whisper_model_ready: bool,
    ollama_server_ready: bool,
    missing_ollama_models: Vec<String>,
    setup_completed: bool,
    all_required_ready: bool,
    guidance: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct WhisperDownloadProgress {
    model_name: String,
    status: String,
    message: String,
    downloaded_bytes: u64,
    total_bytes: Option<u64>,
    percent: f64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct OllamaPullProgress {
    model_name: String,
    status: String,
    message: String,
    completed: u64,
    total: u64,
    percent: f64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ConversionProgress {
    status: String,
    message: String,
    out_time_ms: i64,
    total_duration_ms: Option<i64>,
    percent: f64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TranscriptionProgress {
    status: String,
    message: String,
    percent: f64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TranscriptSegment {
    start_ms: i64,
    end_ms: i64,
    text: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ImportedDocument {
    id: String,
    title: String,
    transcript: String,
    audio_path: String,
    subtitle_srt_path: String,
    subtitle_vtt_path: String,
    duration_seconds: i64,
    created_at: String,
    segments: Vec<TranscriptSegment>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DocumentSummary {
    id: String,
    title: String,
    summary: Option<String>,
    duration_seconds: Option<i64>,
    created_at: String,
    updated_at: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DocumentDetail {
    id: String,
    title: String,
    summary: Option<String>,
    transcript: String,
    audio_path: Option<String>,
    subtitle_srt_path: Option<String>,
    subtitle_vtt_path: Option<String>,
    duration_seconds: Option<i64>,
    created_at: String,
    updated_at: String,
    segments: Vec<TranscriptSegment>,
}

impl Default for PreflightResult {
    fn default() -> Self {
        Self {
            whisper_cli: CheckStatus::Fail,
            ffmpeg: CheckStatus::Fail,
            yt_dlp: CheckStatus::Warn,
            whisper_model: CheckStatus::Fail,
            ollama_server: CheckStatus::Fail,
            ollama_models: CheckStatus::Fail,
            database: CheckStatus::Fail,
            should_open_setup: false,
            all_required_passed: false,
            details: Vec::new(),
        }
    }
}

#[derive(Clone, Copy)]
struct RuntimeBinarySpec {
    check: PreflightCheck,
    tool_id: &'static str,
    display_name: &'static str,
    version: &'static str,
    executable_stem: &'static str,
    version_args: &'static [&'static str],
    path_candidates: &'static [&'static str],
    sidecar_candidates: &'static [&'static str],
    download_url_env: &'static str,
    download_sha256_env: &'static str,
    allow_runtime_download: bool,
}

const WHISPER_BINARY_SPEC: RuntimeBinarySpec = RuntimeBinarySpec {
    check: PreflightCheck::WhisperCli,
    tool_id: "whisper-cli",
    display_name: "whisper-cli",
    version: "runtime",
    executable_stem: "whisper-cli",
    version_args: &["--version"],
    path_candidates: &["whisper-cli"],
    sidecar_candidates: &["binaries/whisper-cli", "whisper-cli"],
    download_url_env: "AUDIOX_WHISPER_URL",
    download_sha256_env: "AUDIOX_WHISPER_SHA256",
    allow_runtime_download: false,
};

const FFMPEG_BINARY_SPEC: RuntimeBinarySpec = RuntimeBinarySpec {
    check: PreflightCheck::Ffmpeg,
    tool_id: "ffmpeg",
    display_name: "ffmpeg",
    version: "runtime",
    executable_stem: "ffmpeg",
    version_args: &["-version"],
    path_candidates: &["ffmpeg"],
    sidecar_candidates: &["binaries/ffmpeg", "ffmpeg"],
    download_url_env: "AUDIOX_FFMPEG_URL",
    download_sha256_env: "AUDIOX_FFMPEG_SHA256",
    allow_runtime_download: false,
};

const YT_DLP_BINARY_SPEC: RuntimeBinarySpec = RuntimeBinarySpec {
    check: PreflightCheck::YtDlp,
    tool_id: "yt-dlp",
    display_name: "yt-dlp",
    version: "runtime",
    executable_stem: "yt-dlp",
    version_args: &["--version"],
    path_candidates: &["yt-dlp", "yt_dlp"],
    sidecar_candidates: &["binaries/yt-dlp", "yt-dlp"],
    download_url_env: "AUDIOX_YTDLP_URL",
    download_sha256_env: "AUDIOX_YTDLP_SHA256",
    allow_runtime_download: false,
};

fn ensure_directory_layout(app_data_dir: &Path) -> Result<Vec<String>, String> {
    fs::create_dir_all(app_data_dir).map_err(|error| {
        format!(
            "failed to create app data directory at {}: {error}",
            app_data_dir.display()
        )
    })?;

    let mut created_directories = Vec::new();
    for directory_name in REQUIRED_DIRECTORIES {
        let directory_path = app_data_dir.join(directory_name);
        if directory_path.exists() {
            if !directory_path.is_dir() {
                return Err(format!(
                    "path exists but is not a directory: {}",
                    directory_path.display()
                ));
            }
            continue;
        }

        fs::create_dir_all(&directory_path).map_err(|error| {
            format!(
                "failed to create required directory {}: {error}",
                directory_path.display()
            )
        })?;
        created_directories.push(directory_name.to_string());
    }

    Ok(created_directories)
}

fn upsert_setting(connection: &Connection, key: &str, value: &str) -> Result<(), String> {
    let key_pattern =
        Regex::new(r"^[a-z0-9_]+$").map_err(|error| format!("failed to compile key validation regex: {error}"))?;

    if !key_pattern.is_match(key) {
        return Err(format!("setting key '{key}' is invalid"));
    }

    connection
        .execute(
            "INSERT INTO settings (key, value, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            params![key, value, Utc::now().to_rfc3339()],
        )
        .map_err(|error| format!("failed to write setting '{key}': {error}"))?;

    Ok(())
}

fn read_setting(connection: &Connection, key: &str) -> Result<Option<String>, String> {
    connection
        .query_row("SELECT value FROM settings WHERE key = ?1", params![key], |row| {
            row.get::<_, String>(0)
        })
        .optional()
        .map_err(|error| format!("failed to read setting '{key}': {error}"))
}

fn parse_setting_bool(value: Option<String>) -> bool {
    value
        .map(|item| item.trim().eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

fn has_column(connection: &Connection, table: &str, column: &str) -> Result<bool, String> {
    let mut statement = connection
        .prepare(&format!("PRAGMA table_info({table})"))
        .map_err(|error| format!("failed to inspect table '{table}': {error}"))?;

    let mut rows = statement
        .query([])
        .map_err(|error| format!("failed to query table info for '{table}': {error}"))?;
    while let Some(row) = rows
        .next()
        .map_err(|error| format!("failed while reading table info for '{table}': {error}"))?
    {
        let name: String = row
            .get(1)
            .map_err(|error| format!("failed to read column metadata for '{table}': {error}"))?;
        if name == column {
            return Ok(true);
        }
    }

    Ok(false)
}

fn ensure_documents_table_columns(connection: &Connection) -> Result<(), String> {
    let required_columns = [
        ("audio_path", "TEXT"),
        ("subtitle_srt_path", "TEXT"),
        ("subtitle_vtt_path", "TEXT"),
    ];

    for (column_name, definition) in required_columns {
        if has_column(connection, "documents", column_name)? {
            continue;
        }

        connection
            .execute(
                &format!("ALTER TABLE documents ADD COLUMN {column_name} {definition}"),
                [],
            )
            .map_err(|error| format!("failed to add documents.{column_name}: {error}"))?;
    }

    Ok(())
}

fn database_path_from_app_data(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("db").join("audiox.db")
}

fn set_setup_completed(database_path: &Path, completed: bool) -> Result<(), String> {
    let connection = Connection::open(database_path)
        .map_err(|error| format!("failed to open database {}: {error}", database_path.display()))?;

    let value = if completed { "true" } else { "false" };
    upsert_setting(&connection, "setup_completed", value)?;
    if completed {
        upsert_setting(&connection, "setup_completed_at", &Utc::now().to_rfc3339())?;
    }
    Ok(())
}

fn initialize_database(database_path: &Path) -> Result<(), String> {
    let connection = Connection::open(database_path)
        .map_err(|error| format!("failed to open database {}: {error}", database_path.display()))?;

    connection
        .execute_batch(
            "
          PRAGMA journal_mode = WAL;
          CREATE TABLE IF NOT EXISTS schema_meta (
            key TEXT PRIMARY KEY NOT NULL,
            value TEXT NOT NULL
          );

          CREATE TABLE IF NOT EXISTS settings (
            key TEXT PRIMARY KEY NOT NULL,
            value TEXT NOT NULL,
            updated_at TEXT NOT NULL
          );

          CREATE TABLE IF NOT EXISTS documents (
            id TEXT PRIMARY KEY NOT NULL,
            source_type TEXT NOT NULL,
            source_uri TEXT,
            title TEXT NOT NULL DEFAULT '',
            summary TEXT,
            transcript TEXT,
            audio_path TEXT,
            subtitle_srt_path TEXT,
            subtitle_vtt_path TEXT,
            duration_seconds INTEGER,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
          );

          CREATE TABLE IF NOT EXISTS document_segments (
            id TEXT PRIMARY KEY NOT NULL,
            document_id TEXT NOT NULL,
            start_ms INTEGER NOT NULL,
            end_ms INTEGER NOT NULL,
            text TEXT NOT NULL,
            created_at TEXT NOT NULL,
            FOREIGN KEY(document_id) REFERENCES documents(id) ON DELETE CASCADE
          );
        ",
        )
        .map_err(|error| format!("failed to initialize schema: {error}"))?;

    ensure_documents_table_columns(&connection)?;

    connection
        .execute(
            "INSERT INTO schema_meta (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params!["schema_version", SCHEMA_VERSION.to_string()],
        )
        .map_err(|error| format!("failed to persist schema version: {error}"))?;

    let installation_id = connection
        .query_row("SELECT value FROM settings WHERE key = 'installation_id'", [], |row| {
            row.get::<_, String>(0)
        })
        .optional()
        .map_err(|error| format!("failed to read installation id: {error}"))?;

    if installation_id.is_none() {
        upsert_setting(&connection, "installation_id", &Uuid::new_v4().to_string())?;
    }
    upsert_setting(&connection, "last_bootstrap_at", &Utc::now().to_rfc3339())?;

    Ok(())
}

fn bootstrap_at(app_data_dir: &Path) -> Result<AppBootstrapResult, String> {
    let created_directories = ensure_directory_layout(app_data_dir)?;
    let database_path = database_path_from_app_data(app_data_dir);
    initialize_database(&database_path)?;

    Ok(AppBootstrapResult {
        app_data_dir: app_data_dir.display().to_string(),
        database_path: database_path.display().to_string(),
        created_directories,
        schema_version: SCHEMA_VERSION,
    })
}

fn bootstrap_from_app(app: &tauri::AppHandle) -> Result<AppBootstrapResult, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("failed to resolve app data directory: {error}"))?;
    bootstrap_at(&app_data_dir)
}

fn summarize_command_output(stderr: &[u8], stdout: &[u8]) -> String {
    let stderr_message = String::from_utf8_lossy(stderr).trim().to_string();
    if !stderr_message.is_empty() {
        return stderr_message;
    }

    let stdout_message = String::from_utf8_lossy(stdout).trim().to_string();
    if !stdout_message.is_empty() {
        return stdout_message;
    }

    "command exited with a non-zero status".to_string()
}

fn detail_status_mut(result: &mut PreflightResult, check: PreflightCheck) -> &mut CheckStatus {
    match check {
        PreflightCheck::WhisperCli => &mut result.whisper_cli,
        PreflightCheck::Ffmpeg => &mut result.ffmpeg,
        PreflightCheck::YtDlp => &mut result.yt_dlp,
        PreflightCheck::WhisperModel => &mut result.whisper_model,
        PreflightCheck::OllamaServer => &mut result.ollama_server,
        PreflightCheck::OllamaModels => &mut result.ollama_models,
        PreflightCheck::Database => &mut result.database,
    }
}

fn emit_preflight_detail(app: &tauri::AppHandle, detail: &PreflightCheckDetail) {
    let _ = app.emit(PREFLIGHT_EVENT, detail.clone());
}

fn record_preflight_detail(
    app: &tauri::AppHandle, result: &mut PreflightResult, check: PreflightCheck, status: CheckStatus,
    message: impl Into<String>,
) {
    let detail = PreflightCheckDetail { check, status, message: message.into() };
    *detail_status_mut(result, check) = status;
    result.details.push(detail.clone());
    emit_preflight_detail(app, &detail);
}

fn managed_binary_filename(spec: &RuntimeBinarySpec) -> String {
    #[cfg(target_os = "windows")]
    {
        format!("{}.exe", spec.executable_stem)
    }

    #[cfg(not(target_os = "windows"))]
    {
        spec.executable_stem.to_string()
    }
}

fn managed_binary_path(app_data_dir: &Path, spec: &RuntimeBinarySpec) -> PathBuf {
    app_data_dir
        .join("bin")
        .join(spec.tool_id)
        .join(spec.version)
        .join(managed_binary_filename(spec))
}

#[cfg(target_os = "windows")]
fn normalize_executable_path(path: PathBuf) -> PathBuf {
    if path.extension().and_then(|item| item.to_str()) == Some("exe") {
        return path;
    }

    let mut with_extension = path;
    with_extension.as_mut_os_string().push(".exe");
    with_extension
}

#[cfg(not(target_os = "windows"))]
fn normalize_executable_path(path: PathBuf) -> PathBuf {
    if path.extension().and_then(|item| item.to_str()) == Some("exe") {
        let mut without_extension = path;
        without_extension.set_extension("");
        return without_extension;
    }
    path
}

fn read_sidecar_candidates_from_directory(dir: &Path, executable_stem: &str) -> Vec<PathBuf> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return Vec::new(),
    };

    let mut matches = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let Some(file_name) = path.file_name().and_then(|item| item.to_str()) else {
            continue;
        };

        if file_name == executable_stem
            || file_name == format!("{executable_stem}.exe")
            || file_name.starts_with(&format!("{executable_stem}-"))
            || file_name.starts_with(&format!("{executable_stem}_"))
        {
            matches.push(path);
        }
    }

    matches
}

fn sidecar_candidate_paths(spec: &RuntimeBinarySpec) -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    if let Ok(current_exe) = std::env::current_exe() {
        if let Some(exe_dir) = current_exe.parent() {
            for candidate in spec.sidecar_candidates {
                candidates.push(normalize_executable_path(exe_dir.join(candidate)));
            }
            candidates.extend(read_sidecar_candidates_from_directory(exe_dir, spec.executable_stem));
            let sidecar_dir = exe_dir.join("binaries");
            candidates.extend(read_sidecar_candidates_from_directory(&sidecar_dir, spec.executable_stem));
        }
    }

    let manifest_sidecar_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("binaries");
    for candidate in spec.sidecar_candidates {
        candidates.push(normalize_executable_path(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(candidate)));
    }
    candidates.extend(read_sidecar_candidates_from_directory(
        &manifest_sidecar_dir,
        spec.executable_stem,
    ));

    let mut seen = HashSet::new();
    candidates.retain(|path| seen.insert(path.to_string_lossy().to_string()));
    candidates
}

async fn execute_version_command<S: AsRef<OsStr>>(
    program: S, program_label: &str, args: &[&str],
) -> Result<(), String> {
    let mut command = tokio::process::Command::new(program);
    command.args(args);
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());

    let output = tokio::time::timeout(Duration::from_secs(COMMAND_TIMEOUT_SECONDS), command.output())
        .await
        .map_err(|_| format!("timed out while checking {program_label}"))?
        .map_err(|error| format!("failed to spawn {program_label}: {error}"))?;

    if output.status.success() {
        return Ok(());
    }

    Err(format!(
        "{program_label} failed version check: {}",
        summarize_command_output(&output.stderr, &output.stdout)
    ))
}

fn env_var(name: &str) -> Result<Option<String>, String> {
    match std::env::var(name) {
        Ok(value) => {
            let trimmed = value.trim().to_string();
            if trimmed.is_empty() {
                Ok(None)
            } else {
                Ok(Some(trimmed))
            }
        }
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(std::env::VarError::NotUnicode(_)) => Err(format!(
            "environment variable {name} contains invalid unicode and cannot be used"
        )),
    }
}

fn normalize_sha256(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn is_valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.chars().all(|character| character.is_ascii_hexdigit())
}

fn configured_download_url(spec: &RuntimeBinarySpec) -> Result<Option<String>, String> {
    env_var(spec.download_url_env)
}

fn configured_download_sha256(spec: &RuntimeBinarySpec) -> Result<Option<String>, String> {
    let value = env_var(spec.download_sha256_env)?;
    match value {
        Some(sha256) => {
            let normalized = normalize_sha256(&sha256);
            if !is_valid_sha256(&normalized) {
                return Err(format!(
                    "environment variable {} must be a 64-character SHA256 hex digest",
                    spec.download_sha256_env
                ));
            }
            Ok(Some(normalized))
        }
        None => Ok(None),
    }
}

fn sha256_hex_for_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn sha256_hex_for_file(path: &Path) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|error| format!("failed to read {} for checksum: {error}", path.display()))?;
    Ok(sha256_hex_for_bytes(&bytes))
}

#[cfg(unix)]
fn make_executable(path: &Path) -> Result<(), String> {
    let mut permissions = fs::metadata(path)
        .map_err(|error| format!("failed to read permissions for {}: {error}", path.display()))?
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions)
        .map_err(|error| format!("failed to set executable permissions on {}: {error}", path.display()))
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> Result<(), String> {
    Ok(())
}

async fn download_binary(url: &str, expected_sha256: &str, destination: &Path) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(DOWNLOAD_TIMEOUT_SECONDS))
        .build()
        .map_err(|error| format!("failed to initialize download client: {error}"))?;

    let response = client
        .get(url)
        .send()
        .await
        .map_err(|error| format!("failed to download {url}: {error}"))?;

    if response.status() != StatusCode::OK {
        return Err(format!(
            "download failed for {url} with HTTP status {}",
            response.status()
        ));
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|error| format!("failed to read download payload from {url}: {error}"))?;

    let actual_sha256 = sha256_hex_for_bytes(&bytes);
    if actual_sha256 != expected_sha256 {
        return Err(format!(
            "checksum mismatch for {url}; expected {expected_sha256} but got {actual_sha256}"
        ));
    }

    let parent_dir = destination
        .parent()
        .ok_or_else(|| format!("destination path {} has no parent directory", destination.display()))?;
    fs::create_dir_all(parent_dir).map_err(|error| format!("failed to create {}: {error}", parent_dir.display()))?;

    let file_name = destination
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| format!("destination filename is invalid unicode: {}", destination.display()))?;
    let temporary_path = destination.with_file_name(format!("{file_name}.download"));

    fs::write(&temporary_path, &bytes)
        .map_err(|error| format!("failed to write {}: {error}", temporary_path.display()))?;
    make_executable(&temporary_path)?;

    if destination.exists() {
        fs::remove_file(destination)
            .map_err(|error| format!("failed to replace existing {}: {error}", destination.display()))?;
    }

    fs::rename(&temporary_path, destination)
        .map_err(|error| format!("failed to install {}: {error}", destination.display()))?;

    Ok(())
}

async fn try_system_binary(spec: &RuntimeBinarySpec) -> Option<String> {
    for candidate in spec.path_candidates {
        if execute_version_command(candidate, candidate, spec.version_args)
            .await
            .is_ok()
        {
            return Some((*candidate).to_string());
        }
    }
    None
}

fn download_guidance(spec: &RuntimeBinarySpec) -> String {
    format!(
        "{} is missing. Install '{}' on PATH or configure {} and {} to allow runtime download.",
        spec.display_name, spec.executable_stem, spec.download_url_env, spec.download_sha256_env
    )
}

fn non_download_guidance(spec: &RuntimeBinarySpec) -> String {
    format!(
        "{} is unavailable. Reinstall Audio X to restore bundled dependencies. For local development, run `bash setup.sh` and ensure '{}' is installed on PATH.",
        spec.display_name, spec.executable_stem
    )
}

async fn try_sidecar_binary(spec: &RuntimeBinarySpec) -> Result<Option<PathBuf>, String> {
    let mut errors = Vec::new();
    for candidate in sidecar_candidate_paths(spec) {
        if !candidate.is_file() {
            continue;
        }

        let display = candidate.display().to_string();
        match execute_version_command(&candidate, &display, spec.version_args).await {
            Ok(()) => {
                return Ok(Some(candidate));
            }
            Err(error) => errors.push(format!("{display}: {error}")),
        }
    }

    if !errors.is_empty() {
        log::warn!(
            "found {} sidecar candidates but none were executable: {}",
            spec.display_name,
            errors.join(" | ")
        );
    }

    Ok(None)
}

#[derive(Clone, Debug)]
struct ResolvedBinary {
    program: String,
    message: String,
}

async fn resolve_runtime_binary(app_data_dir: &Path, spec: &RuntimeBinarySpec) -> Result<ResolvedBinary, String> {
    if let Some(sidecar_path) = try_sidecar_binary(spec).await? {
        return Ok(ResolvedBinary {
            program: sidecar_path.display().to_string(),
            message: format!("{} sidecar is available at {}.", spec.display_name, sidecar_path.display()),
        });
    }

    let binary_path = managed_binary_path(app_data_dir, spec);

    if binary_path.is_file() {
        let label = binary_path.display().to_string();
        execute_version_command(&binary_path, &label, spec.version_args).await?;
        return Ok(ResolvedBinary {
            program: binary_path.display().to_string(),
            message: format!("{} is available at {}.", spec.display_name, binary_path.display()),
        });
    }

    if let Some(system_binary) = try_system_binary(spec).await {
        return Ok(ResolvedBinary {
            program: system_binary.clone(),
            message: format!("{} is available on PATH as '{system_binary}'.", spec.display_name),
        });
    }

    if !spec.allow_runtime_download {
        return Err(non_download_guidance(spec));
    }

    let download_url = configured_download_url(spec)?;
    let download_sha256 = configured_download_sha256(spec)?;

    let url = download_url.ok_or_else(|| download_guidance(spec))?;
    let sha256 = download_sha256.ok_or_else(|| {
        format!(
            "{} is missing; {} must be set when downloading from {}.",
            spec.display_name, spec.download_sha256_env, spec.download_url_env
        )
    })?;

    download_binary(&url, &sha256, &binary_path).await?;

    let label = binary_path.display().to_string();
    execute_version_command(&binary_path, &label, spec.version_args).await?;

    let installed_sha256 = sha256_hex_for_file(&binary_path)?;
    Ok(ResolvedBinary {
        program: binary_path.display().to_string(),
        message: format!(
            "Downloaded {} to {} (sha256: {}).",
            spec.display_name,
            binary_path.display(),
            installed_sha256
        ),
    })
}

async fn resolve_runtime_binary_program(app_data_dir: &Path, spec: &RuntimeBinarySpec) -> Result<String, String> {
    let resolved = resolve_runtime_binary(app_data_dir, spec).await?;
    Ok(resolved.program)
}

async fn ensure_runtime_binary(app_data_dir: &Path, spec: &RuntimeBinarySpec) -> Result<String, String> {
    let resolved = resolve_runtime_binary(app_data_dir, spec).await?;
    Ok(resolved.message)
}

fn whisper_model_present(models_dir: &Path) -> Result<bool, String> {
    if !models_dir.exists() {
        return Ok(false);
    }

    let entries = fs::read_dir(models_dir)
        .map_err(|error| format!("failed to list models directory {}: {error}", models_dir.display()))?;

    for entry in entries {
        let entry =
            entry.map_err(|error| format!("failed to inspect models directory {}: {error}", models_dir.display()))?;
        let path = entry.path();
        let extension = path.extension().and_then(|item| item.to_str()).unwrap_or("");
        if path.is_file() && extension.eq_ignore_ascii_case("bin") {
            return Ok(true);
        }
    }

    Ok(false)
}

fn parse_ollama_model_names(payload: &Value) -> Vec<String> {
    payload
        .get("models")
        .and_then(Value::as_array)
        .into_iter()
        .flat_map(|models| models.iter())
        .filter_map(|model| {
            model
                .get("name")
                .and_then(Value::as_str)
                .or_else(|| model.get("model").and_then(Value::as_str))
        })
        .map(ToString::to_string)
        .collect()
}

fn model_name_matches(candidate: &str, required: &str) -> bool {
    let candidate = candidate.trim().to_ascii_lowercase();
    let required = required.trim().to_ascii_lowercase();

    if candidate == required {
        return true;
    }

    let (candidate_family, candidate_tag) = candidate
        .split_once(':')
        .map_or((candidate.as_str(), None), |(family, tag)| (family, Some(tag)));
    let (required_family, required_tag) = required
        .split_once(':')
        .map_or((required.as_str(), None), |(family, tag)| (family, Some(tag)));

    if candidate_family != required_family {
        return false;
    }

    match (candidate_tag, required_tag) {
        (None, _) | (_, None) => true,
        (Some(candidate_tag), Some(required_tag)) => {
            candidate_tag == required_tag || candidate_tag == "latest" || required_tag == "latest"
        }
    }
}

fn missing_required_ollama_models(models: &[String]) -> Vec<String> {
    REQUIRED_OLLAMA_MODELS
        .iter()
        .filter(|required| !models.iter().any(|candidate| model_name_matches(candidate, required)))
        .map(|required| required.to_string())
        .collect()
}

async fn fetch_ollama_model_names() -> Result<Vec<String>, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .map_err(|error| format!("failed to initialize HTTP client: {error}"))?;

    let response = client
        .get(OLLAMA_TAGS_URL)
        .send()
        .await
        .map_err(|error| format!("failed to reach Ollama at {OLLAMA_TAGS_URL}: {error}"))?;

    if !response.status().is_success() {
        return Err(format!("Ollama responded with unexpected status {}", response.status()));
    }

    let tags_payload = response
        .json::<Value>()
        .await
        .map_err(|error| format!("failed to parse Ollama tags response: {error}"))?;

    Ok(parse_ollama_model_names(&tags_payload))
}

fn validate_whisper_model_name(model_name: &str) -> Result<String, String> {
    let trimmed = model_name.trim();
    let valid_pattern =
        Regex::new(r"^[a-z0-9._-]+$").map_err(|error| format!("failed to compile model validation regex: {error}"))?;
    if !valid_pattern.is_match(trimmed) {
        return Err(format!(
            "invalid whisper model name '{model_name}'; expected letters, numbers, dots, underscores, or dashes"
        ));
    }
    Ok(trimmed.to_string())
}

fn whisper_model_file_name(model_name: &str) -> String {
    format!("ggml-{model_name}.bin")
}

fn whisper_model_download_url(model_name: &str) -> String {
    format!(
        "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/{}",
        whisper_model_file_name(model_name)
    )
}

fn calculate_percent(completed: u64, total: u64) -> f64 {
    if total == 0 {
        return 0.0;
    }

    let percent = (completed as f64 / total as f64) * 100.0;
    percent.clamp(0.0, 100.0)
}

fn emit_whisper_progress(
    app: &tauri::AppHandle, model_name: &str, status: &str, message: impl Into<String>, downloaded_bytes: u64,
    total_bytes: Option<u64>,
) {
    let percent = total_bytes
        .map(|total| calculate_percent(downloaded_bytes, total))
        .unwrap_or(0.0);
    let payload = WhisperDownloadProgress {
        model_name: model_name.to_string(),
        status: status.to_string(),
        message: message.into(),
        downloaded_bytes,
        total_bytes,
        percent,
    };
    let _ = app.emit(SETUP_WHISPER_PROGRESS_EVENT, payload);
}

fn emit_ollama_progress(
    app: &tauri::AppHandle, model_name: &str, status: &str, message: impl Into<String>, completed: u64, total: u64,
) {
    let payload = OllamaPullProgress {
        model_name: model_name.to_string(),
        status: status.to_string(),
        message: message.into(),
        completed,
        total,
        percent: calculate_percent(completed, total),
    };
    let _ = app.emit(SETUP_OLLAMA_PROGRESS_EVENT, payload);
}

async fn download_whisper_model_file(
    app: &tauri::AppHandle, app_data_dir: &Path, model_name: &str,
) -> Result<PathBuf, String> {
    let model_file_name = whisper_model_file_name(model_name);
    let destination = app_data_dir.join("models").join(&model_file_name);
    if destination.is_file() {
        emit_whisper_progress(
            app,
            model_name,
            "completed",
            format!("{model_file_name} already exists."),
            1,
            Some(1),
        );
        return Ok(destination);
    }

    let url = whisper_model_download_url(model_name);
    emit_whisper_progress(
        app,
        model_name,
        "running",
        format!("Starting download from {url}"),
        0,
        None,
    );

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(DOWNLOAD_TIMEOUT_SECONDS))
        .build()
        .map_err(|error| format!("failed to initialize whisper model download client: {error}"))?;

    let mut response = client
        .get(&url)
        .send()
        .await
        .map_err(|error| format!("failed to download whisper model from {url}: {error}"))?;

    if response.status() != StatusCode::OK {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!(
            "whisper model download failed with HTTP status {status}: {body}"
        ));
    }

    let parent = destination
        .parent()
        .ok_or_else(|| format!("destination path {} has no parent directory", destination.display()))?;
    fs::create_dir_all(parent).map_err(|error| format!("failed to create {}: {error}", parent.display()))?;

    let temporary_path = destination.with_extension("bin.download");
    let mut file = fs::File::create(&temporary_path).map_err(|error| {
        format!(
            "failed to create temporary model file {}: {error}",
            temporary_path.display()
        )
    })?;

    let total_bytes = response.content_length();
    let mut downloaded_bytes = 0_u64;
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| format!("failed while downloading whisper model: {error}"))?
    {
        file.write_all(&chunk)
            .map_err(|error| format!("failed writing model data to {}: {error}", temporary_path.display()))?;
        downloaded_bytes += u64::try_from(chunk.len()).map_err(|error| format!("download chunk too large: {error}"))?;
        emit_whisper_progress(
            app,
            model_name,
            "running",
            format!("Downloading {model_file_name}..."),
            downloaded_bytes,
            total_bytes,
        );
    }

    file.flush().map_err(|error| {
        format!(
            "failed to flush temporary model file {}: {error}",
            temporary_path.display()
        )
    })?;

    if destination.exists() {
        fs::remove_file(&destination).map_err(|error| {
            format!(
                "failed to replace existing whisper model file {}: {error}",
                destination.display()
            )
        })?;
    }
    fs::rename(&temporary_path, &destination)
        .map_err(|error| format!("failed to install whisper model at {}: {error}", destination.display()))?;

    emit_whisper_progress(
        app,
        model_name,
        "completed",
        format!("Downloaded {model_file_name}."),
        downloaded_bytes,
        total_bytes.or(Some(downloaded_bytes)),
    );
    Ok(destination)
}

fn compute_setup_guidance(
    whisper_model_ready: bool, ollama_server_ready: bool, missing_ollama_models: &[String], ollama_error: Option<&str>,
) -> Vec<String> {
    let mut guidance = Vec::new();
    if !whisper_model_ready {
        guidance.push(format!(
            "Download {} into appdata/models to enable transcription.",
            whisper_model_file_name(DEFAULT_WHISPER_MODEL_NAME)
        ));
    }
    if !ollama_server_ready {
        let suffix = ollama_error.unwrap_or("Ollama did not respond.");
        guidance.push(format!(
            "{suffix} Install Ollama from https://ollama.com and start it with `ollama serve`."
        ));
    } else if !missing_ollama_models.is_empty() {
        guidance.push(format!(
            "Pull missing Ollama models: {}.",
            missing_ollama_models.join(", ")
        ));
    }

    guidance
}

async fn check_setup_state(app_data_dir: &Path) -> Result<SetupStatus, String> {
    ensure_directory_layout(app_data_dir)?;
    let database_path = database_path_from_app_data(app_data_dir);
    initialize_database(&database_path)?;

    let whisper_model_ready = whisper_model_present(&app_data_dir.join("models"))?;
    let (ollama_server_ready, missing_ollama_models, ollama_error) = match fetch_ollama_model_names().await {
        Ok(models) => {
            let missing = missing_required_ollama_models(&models);
            (true, missing, None)
        }
        Err(error) => (
            false,
            REQUIRED_OLLAMA_MODELS.iter().map(|item| (*item).to_string()).collect(),
            Some(error),
        ),
    };

    let all_required_ready = whisper_model_ready && ollama_server_ready && missing_ollama_models.is_empty();
    set_setup_completed(&database_path, all_required_ready)?;

    let connection = Connection::open(&database_path)
        .map_err(|error| format!("failed to open database {}: {error}", database_path.display()))?;
    let setup_completed = parse_setting_bool(read_setting(&connection, "setup_completed")?);

    Ok(SetupStatus {
        whisper_model_ready,
        ollama_server_ready,
        missing_ollama_models: missing_ollama_models.clone(),
        setup_completed,
        all_required_ready,
        guidance: compute_setup_guidance(
            whisper_model_ready,
            ollama_server_ready,
            &missing_ollama_models,
            ollama_error.as_deref(),
        ),
    })
}

fn parse_ollama_progress_line(line: &str) -> Result<(String, u64, u64, bool), String> {
    let payload = serde_json::from_str::<Value>(line)
        .map_err(|error| format!("invalid Ollama progress payload '{line}': {error}"))?;
    let status = payload
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("pulling")
        .to_string();
    let completed = payload.get("completed").and_then(Value::as_u64).unwrap_or(0);
    let total = payload.get("total").and_then(Value::as_u64).unwrap_or(0);
    let done = payload.get("done").and_then(Value::as_bool).unwrap_or(false);
    Ok((status, completed, total, done))
}

fn compute_all_required_passed(result: &PreflightResult) -> bool {
    ![
        result.whisper_cli,
        result.ffmpeg,
        result.whisper_model,
        result.ollama_server,
        result.ollama_models,
        result.database,
    ]
    .contains(&CheckStatus::Fail)
}

fn extension_for_path(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.trim().to_ascii_lowercase())
}

fn ensure_supported_import_path(source_path: &Path) -> Result<(), String> {
    if !source_path.is_file() {
        return Err(format!(
            "source file does not exist or is not a regular file: {}",
            source_path.display()
        ));
    }

    let extension = extension_for_path(source_path)
        .ok_or_else(|| format!("unsupported import file extension for {}", source_path.display()))?;

    if ALLOWED_IMPORT_EXTENSIONS
        .iter()
        .any(|candidate| *candidate == extension)
    {
        return Ok(());
    }

    Err(format!(
        "unsupported file extension '.{extension}'. Supported formats: {}",
        ALLOWED_IMPORT_EXTENSIONS.join(", ")
    ))
}

fn parse_hms_to_ms(hours: i64, minutes: i64, seconds: f64) -> i64 {
    let whole_seconds = seconds.trunc() as i64;
    let milliseconds = ((seconds - seconds.trunc()) * 1000.0).round() as i64;
    (hours * 3_600_000) + (minutes * 60_000) + (whole_seconds * 1000) + milliseconds
}

fn parse_clock_timestamp_to_ms(value: &str) -> Option<i64> {
    let mut parts = value.split(':');
    let hours = parts.next()?.trim().parse::<i64>().ok()?;
    let minutes = parts.next()?.trim().parse::<i64>().ok()?;
    let seconds = parts
        .next()?
        .trim()
        .replace(',', ".")
        .parse::<f64>()
        .ok()?;
    if parts.next().is_some() {
        return None;
    }

    Some(parse_hms_to_ms(hours, minutes, seconds))
}

fn parse_ffmpeg_duration_ms(payload: &str) -> Option<i64> {
    let duration_regex = Regex::new(r"Duration:\s+(\d{2}):(\d{2}):(\d{2}(?:\.\d+)?)").ok()?;
    let captures = duration_regex.captures(payload)?;
    let hours = captures.get(1)?.as_str().parse::<i64>().ok()?;
    let minutes = captures.get(2)?.as_str().parse::<i64>().ok()?;
    let seconds = captures.get(3)?.as_str().parse::<f64>().ok()?;
    Some(parse_hms_to_ms(hours, minutes, seconds))
}

fn parse_ffmpeg_out_time_ms(value: &str) -> Option<i64> {
    if let Ok(raw) = value.trim().parse::<i64>() {
        if raw > 10_000_000 {
            return Some(raw / 1000);
        }
        return Some(raw);
    }

    parse_clock_timestamp_to_ms(value.trim())
}

fn emit_conversion_progress(
    app: &tauri::AppHandle, status: &str, message: impl Into<String>, out_time_ms: i64, total_duration_ms: Option<i64>,
) {
    let percent = total_duration_ms
        .map(|total| calculate_percent(u64::try_from(out_time_ms.max(0)).unwrap_or_default(), total as u64))
        .unwrap_or(0.0);

    let payload = ConversionProgress {
        status: status.to_string(),
        message: message.into(),
        out_time_ms,
        total_duration_ms,
        percent,
    };
    let _ = app.emit(IMPORT_CONVERSION_PROGRESS_EVENT, payload);
}

fn emit_transcription_progress(app: &tauri::AppHandle, status: &str, message: impl Into<String>, percent: f64) {
    let payload = TranscriptionProgress {
        status: status.to_string(),
        message: message.into(),
        percent: percent.clamp(0.0, 100.0),
    };
    let _ = app.emit(IMPORT_TRANSCRIPTION_PROGRESS_EVENT, payload);
}

async fn probe_ffmpeg_duration_ms(ffmpeg_program: &str, input_path: &Path) -> Result<Option<i64>, String> {
    let mut command = tokio::process::Command::new(ffmpeg_program);
    command.arg("-i").arg(input_path);
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());

    let output = tokio::time::timeout(Duration::from_secs(COMMAND_TIMEOUT_SECONDS), command.output())
        .await
        .map_err(|_| format!("timed out while probing media duration for {}", input_path.display()))?
        .map_err(|error| format!("failed to run ffmpeg duration probe: {error}"))?;

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(parse_ffmpeg_duration_ms(&format!("{stderr}\n{stdout}")))
}

async fn run_ffmpeg_conversion(
    app: &tauri::AppHandle, ffmpeg_program: &str, input_path: &Path, output_path: &Path,
) -> Result<(), String> {
    let total_duration_ms = probe_ffmpeg_duration_ms(ffmpeg_program, input_path).await?;
    emit_conversion_progress(
        app,
        "running",
        format!("Converting {} to 16kHz mono WAV...", input_path.display()),
        0,
        total_duration_ms,
    );

    let mut command = tokio::process::Command::new(ffmpeg_program);
    command
        .arg("-i")
        .arg(input_path)
        .arg("-ar")
        .arg("16000")
        .arg("-ac")
        .arg("1")
        .arg("-c:a")
        .arg("pcm_s16le")
        .arg("-progress")
        .arg("pipe:1")
        .arg("-nostats")
        .arg("-y")
        .arg(output_path);
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());

    let mut child = command
        .spawn()
        .map_err(|error| format!("failed to start ffmpeg conversion: {error}"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "ffmpeg conversion did not provide stdout".to_string())?;
    let mut stderr = child
        .stderr
        .take()
        .ok_or_else(|| "ffmpeg conversion did not provide stderr".to_string())?;

    let stderr_task = tokio::spawn(async move {
        let mut collected = String::new();
        stderr
            .read_to_string(&mut collected)
            .await
            .map_err(|error| format!("failed to read ffmpeg stderr: {error}"))?;
        Ok::<String, String>(collected)
    });

    let mut latest_out_time_ms = 0_i64;
    let mut lines = BufReader::new(stdout).lines();
    while let Some(line) = lines
        .next_line()
        .await
        .map_err(|error| format!("failed to read ffmpeg progress: {error}"))?
    {
        let trimmed = line.trim();
        if let Some(value) = trimmed.strip_prefix("out_time_ms=") {
            if let Some(out_time_ms) = parse_ffmpeg_out_time_ms(value) {
                latest_out_time_ms = out_time_ms.max(latest_out_time_ms);
                emit_conversion_progress(
                    app,
                    "running",
                    "Converting audio with ffmpeg...",
                    latest_out_time_ms,
                    total_duration_ms,
                );
            }
            continue;
        }

        if let Some(value) = trimmed.strip_prefix("out_time=") {
            if let Some(out_time_ms) = parse_ffmpeg_out_time_ms(value) {
                latest_out_time_ms = out_time_ms.max(latest_out_time_ms);
                emit_conversion_progress(
                    app,
                    "running",
                    "Converting audio with ffmpeg...",
                    latest_out_time_ms,
                    total_duration_ms,
                );
            }
            continue;
        }

        if trimmed == "progress=end" {
            let final_out_time = total_duration_ms.unwrap_or(latest_out_time_ms);
            emit_conversion_progress(
                app,
                "completed",
                format!("Audio conversion complete: {}", output_path.display()),
                final_out_time,
                total_duration_ms.or(Some(final_out_time)),
            );
        }
    }

    let status = child
        .wait()
        .await
        .map_err(|error| format!("failed waiting for ffmpeg conversion: {error}"))?;
    let stderr_output = stderr_task
        .await
        .map_err(|error| format!("ffmpeg stderr task failed: {error}"))??;

    if !status.success() {
        emit_conversion_progress(app, "error", "Audio conversion failed.", latest_out_time_ms, total_duration_ms);
        return Err(format!(
            "ffmpeg conversion failed: {}",
            summarize_command_output(stderr_output.as_bytes(), &[])
        ));
    }

    Ok(())
}

fn parse_progress_percent(line: &str) -> Option<f64> {
    let captures = Regex::new(r"(\d{1,3}(?:\.\d+)?)%").ok()?.captures(line)?;
    let raw = captures.get(1)?.as_str().parse::<f64>().ok()?;
    Some(raw.clamp(0.0, 100.0))
}

fn value_to_i64(value: &Value) -> Option<i64> {
    value
        .as_i64()
        .or_else(|| value.as_u64().and_then(|number| i64::try_from(number).ok()))
        .or_else(|| value.as_f64().map(|number| number.round() as i64))
}

fn parse_transcript_segment(item: &Value) -> Option<TranscriptSegment> {
    let text = item
        .get("text")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("")
        .to_string();
    if text.is_empty() {
        return None;
    }

    let from_offset = item.get("offsets").and_then(|offsets| offsets.get("from")).and_then(value_to_i64);
    let to_offset = item.get("offsets").and_then(|offsets| offsets.get("to")).and_then(value_to_i64);
    let from_timestamp = item
        .get("timestamps")
        .and_then(|timestamps| timestamps.get("from"))
        .and_then(Value::as_str)
        .and_then(parse_clock_timestamp_to_ms);
    let to_timestamp = item
        .get("timestamps")
        .and_then(|timestamps| timestamps.get("to"))
        .and_then(Value::as_str)
        .and_then(parse_clock_timestamp_to_ms);

    let from_seconds = item
        .get("start")
        .and_then(Value::as_f64)
        .map(|seconds| (seconds * 1000.0).round() as i64);
    let to_seconds = item
        .get("end")
        .and_then(Value::as_f64)
        .map(|seconds| (seconds * 1000.0).round() as i64);

    let start_ms = from_offset.or(from_timestamp).or(from_seconds)?;
    let mut end_ms = to_offset.or(to_timestamp).or(to_seconds).unwrap_or(start_ms);
    if end_ms < start_ms {
        end_ms = start_ms;
    }

    Some(TranscriptSegment { start_ms, end_ms, text })
}

fn parse_whisper_segments(payload: &Value) -> Vec<TranscriptSegment> {
    let candidate_arrays = [
        payload.get("transcription").and_then(Value::as_array),
        payload.get("segments").and_then(Value::as_array),
    ];

    let mut segments = candidate_arrays
        .into_iter()
        .flatten()
        .flat_map(|entries| entries.iter())
        .filter_map(parse_transcript_segment)
        .collect::<Vec<_>>();

    segments.sort_by_key(|segment| (segment.start_ms, segment.end_ms));
    segments
}

fn build_transcript_text(segments: &[TranscriptSegment]) -> String {
    let mut transcript = String::new();
    for segment in segments {
        if transcript.is_empty() {
            transcript.push_str(segment.text.trim());
            continue;
        }
        transcript.push(' ');
        transcript.push_str(segment.text.trim());
    }
    transcript
}

fn max_duration_seconds(segments: &[TranscriptSegment]) -> i64 {
    let max_end_ms = segments.iter().map(|segment| segment.end_ms).max().unwrap_or(0);
    ((max_end_ms as f64) / 1000.0).ceil() as i64
}

fn path_for_storage(path: &Path, app_data_dir: &Path) -> String {
    if let Ok(relative) = path.strip_prefix(app_data_dir) {
        return relative.to_string_lossy().to_string();
    }
    path.to_string_lossy().to_string()
}

fn resolve_whisper_model_path(app_data_dir: &Path) -> Result<PathBuf, String> {
    let model_dir = app_data_dir.join("models");
    let preferred = model_dir.join(whisper_model_file_name(DEFAULT_WHISPER_MODEL_NAME));
    if preferred.is_file() {
        return Ok(preferred);
    }

    let entries =
        fs::read_dir(&model_dir).map_err(|error| format!("failed to read models directory {}: {error}", model_dir.display()))?;
    for entry in entries {
        let path = entry
            .map_err(|error| format!("failed to inspect models directory {}: {error}", model_dir.display()))?
            .path();
        if path.is_file()
            && path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("bin"))
        {
            return Ok(path);
        }
    }

    Err(format!(
        "no whisper model file found in {}. Run setup to download {}.",
        model_dir.display(),
        whisper_model_file_name(DEFAULT_WHISPER_MODEL_NAME)
    ))
}

async fn run_whisper_transcription(
    app: &tauri::AppHandle, whisper_program: &str, model_path: &Path, wav_path: &Path, output_base: &Path,
) -> Result<Vec<TranscriptSegment>, String> {
    emit_transcription_progress(
        app,
        "running",
        format!("Transcribing {} with whisper-cli...", wav_path.display()),
        0.0,
    );

    let mut command = tokio::process::Command::new(whisper_program);
    command
        .arg("-m")
        .arg(model_path)
        .arg("-f")
        .arg(wav_path)
        .arg("-oj")
        .arg("-osrt")
        .arg("-ovtt")
        .arg("-of")
        .arg(output_base)
        .arg("-l")
        .arg("auto")
        .arg("-t")
        .arg(DEFAULT_WHISPER_THREADS.to_string())
        .arg("-pp");
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());

    let mut child = command
        .spawn()
        .map_err(|error| format!("failed to start whisper-cli transcription: {error}"))?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| "whisper-cli transcription did not provide stdout".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "whisper-cli transcription did not provide stderr".to_string())?;

    let stdout_task = tokio::spawn(async move {
        let mut collected = String::new();
        stdout
            .read_to_string(&mut collected)
            .await
            .map_err(|error| format!("failed to read whisper stdout: {error}"))?;
        Ok::<String, String>(collected)
    });

    let mut stderr_lines = BufReader::new(stderr).lines();
    let mut stderr_output = String::new();
    let mut highest_progress = 0.0;
    while let Some(line) = stderr_lines
        .next_line()
        .await
        .map_err(|error| format!("failed to read whisper progress output: {error}"))?
    {
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            stderr_output.push_str(trimmed);
            stderr_output.push('\n');
            if let Some(percent) = parse_progress_percent(trimmed) {
                if percent > highest_progress {
                    highest_progress = percent;
                }
                emit_transcription_progress(app, "running", "Transcribing audio with whisper-cli...", highest_progress);
            }
        }
    }

    let status = child
        .wait()
        .await
        .map_err(|error| format!("failed while waiting for whisper-cli: {error}"))?;
    let stdout_output = stdout_task
        .await
        .map_err(|error| format!("whisper stdout task failed: {error}"))??;

    if !status.success() {
        emit_transcription_progress(app, "error", "Transcription failed.", highest_progress);
        return Err(format!(
            "whisper-cli transcription failed: {}",
            summarize_command_output(stderr_output.as_bytes(), stdout_output.as_bytes())
        ));
    }

    let json_path = output_base.with_extension("json");
    let json_payload = fs::read_to_string(&json_path)
        .map_err(|error| format!("failed to read whisper JSON output at {}: {error}", json_path.display()))?;
    let parsed = serde_json::from_str::<Value>(&json_payload)
        .map_err(|error| format!("failed to parse whisper JSON output {}: {error}", json_path.display()))?;

    let segments = parse_whisper_segments(&parsed);
    emit_transcription_progress(
        app,
        "completed",
        format!("Transcription complete. {} segments generated.", segments.len()),
        100.0,
    );
    Ok(segments)
}

fn persist_document(
    database_path: &Path, document_id: &str, title: &str, source_uri: &str, transcript: &str, audio_path: &str,
    subtitle_srt_path: &str, subtitle_vtt_path: &str, duration_seconds: i64, segments: &[TranscriptSegment],
) -> Result<(), String> {
    let connection = Connection::open(database_path)
        .map_err(|error| format!("failed to open database {}: {error}", database_path.display()))?;
    let transaction = connection
        .unchecked_transaction()
        .map_err(|error| format!("failed to start persistence transaction: {error}"))?;

    let now = Utc::now().to_rfc3339();
    transaction
        .execute(
            "INSERT INTO documents (
                id, source_type, source_uri, title, summary, transcript, audio_path, subtitle_srt_path, subtitle_vtt_path,
                duration_seconds, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                document_id,
                "file_import",
                source_uri,
                title,
                Option::<String>::None,
                transcript,
                audio_path,
                subtitle_srt_path,
                subtitle_vtt_path,
                duration_seconds,
                now,
                now
            ],
        )
        .map_err(|error| format!("failed to insert document {document_id}: {error}"))?;

    let mut statement = transaction
        .prepare(
            "INSERT INTO document_segments (id, document_id, start_ms, end_ms, text, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )
        .map_err(|error| format!("failed to prepare segment insert statement: {error}"))?;
    for segment in segments {
        statement
            .execute(params![
                Uuid::new_v4().to_string(),
                document_id,
                segment.start_ms,
                segment.end_ms,
                segment.text,
                now
            ])
            .map_err(|error| format!("failed to insert document segment for {document_id}: {error}"))?;
    }

    drop(statement);
    transaction
        .commit()
        .map_err(|error| format!("failed to commit document transaction for {document_id}: {error}"))?;
    Ok(())
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
async fn check_setup(app: tauri::AppHandle) -> Result<SetupStatus, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("failed to resolve app data directory: {error}"))?;

    let setup = check_setup_state(&app_data_dir).await?;
    log::info!(
        "setup status: whisper_ready={}, ollama_server_ready={}, missing_models={}, completed={}",
        setup.whisper_model_ready,
        setup.ollama_server_ready,
        setup.missing_ollama_models.join(","),
        setup.setup_completed
    );
    Ok(setup)
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
async fn download_whisper_model(app: tauri::AppHandle, model: Option<String>) -> Result<String, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("failed to resolve app data directory: {error}"))?;
    ensure_directory_layout(&app_data_dir)?;

    let model_name = validate_whisper_model_name(model.as_deref().unwrap_or(DEFAULT_WHISPER_MODEL_NAME))?;
    let model_path = download_whisper_model_file(&app, &app_data_dir, &model_name).await?;
    let database_path = database_path_from_app_data(&app_data_dir);
    initialize_database(&database_path)?;
    let setup_status = check_setup_state(&app_data_dir).await?;
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
async fn pull_ollama_model(app: tauri::AppHandle, model: String) -> Result<(), String> {
    let model_name = model.trim().to_string();
    if model_name.is_empty() {
        return Err("model_name must not be empty".to_string());
    }

    emit_ollama_progress(
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
        .post(OLLAMA_PULL_URL)
        .json(&serde_json::json!({ "name": model_name, "stream": true }))
        .send()
        .await
        .map_err(|error| format!("failed to call Ollama pull API at {OLLAMA_PULL_URL}: {error}"))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        let message = format!("Ollama pull failed with status {status}: {body}");
        emit_ollama_progress(&app, &model_name, "error", &message, 0, 0);
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

            let (status, completed, total, done) = parse_ollama_progress_line(&line)?;
            let progress_status = if done { "completed" } else { "running" };
            emit_ollama_progress(&app, &model_name, progress_status, status, completed, total);
            if done {
                received_done = true;
            }
        }
    }

    let trailing = buffer.trim();
    if !trailing.is_empty() {
        let (status, completed, total, done) = parse_ollama_progress_line(trailing)?;
        let progress_status = if done { "completed" } else { "running" };
        emit_ollama_progress(&app, &model_name, progress_status, status, completed, total);
        if done {
            received_done = true;
        }
    }

    if !received_done {
        emit_ollama_progress(
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
    let setup_status = check_setup_state(&app_data_dir).await?;
    log::info!(
        "pulled ollama model {} (missing_models_after_pull={})",
        model_name,
        setup_status.missing_ollama_models.join(",")
    );

    Ok(())
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
async fn import_audio_file(app: tauri::AppHandle, source_path: String) -> Result<ImportedDocument, String> {
    let source_trimmed = source_path.trim();
    if source_trimmed.is_empty() {
        return Err("source_path must not be empty".to_string());
    }

    let source = PathBuf::from(source_trimmed);
    ensure_supported_import_path(&source)?;

    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("failed to resolve app data directory: {error}"))?;
    ensure_directory_layout(&app_data_dir)?;
    let database_path = database_path_from_app_data(&app_data_dir);
    initialize_database(&database_path)?;

    let ffmpeg_program = resolve_runtime_binary_program(&app_data_dir, &FFMPEG_BINARY_SPEC).await?;
    let whisper_program = resolve_runtime_binary_program(&app_data_dir, &WHISPER_BINARY_SPEC).await?;
    let model_path = resolve_whisper_model_path(&app_data_dir)?;

    let document_id = Uuid::new_v4().to_string();
    let extension =
        extension_for_path(&source).ok_or_else(|| format!("failed to determine extension for {}", source.display()))?;
    let copied_source_path = app_data_dir
        .join("audio")
        .join(format!("{document_id}-source.{extension}"));
    fs::copy(&source, &copied_source_path)
        .map_err(|error| format!("failed to copy source audio into app data: {error}"))?;

    let converted_wav_path = app_data_dir.join("audio").join(format!("{document_id}.wav"));
    run_ffmpeg_conversion(&app, &ffmpeg_program, &copied_source_path, &converted_wav_path).await?;

    let subtitle_base = app_data_dir.join("subtitles").join(&document_id);
    let segments =
        run_whisper_transcription(&app, &whisper_program, &model_path, &converted_wav_path, &subtitle_base).await?;
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

    let transcript = build_transcript_text(&segments);
    let duration_seconds = max_duration_seconds(&segments);
    let title = source
        .file_stem()
        .and_then(|stem| stem.to_str())
        .map(str::trim)
        .filter(|stem| !stem.is_empty())
        .unwrap_or("Imported audio")
        .to_string();

    let audio_path = path_for_storage(&converted_wav_path, &app_data_dir);
    let subtitle_srt = path_for_storage(&subtitle_srt_path, &app_data_dir);
    let subtitle_vtt = path_for_storage(&subtitle_vtt_path, &app_data_dir);
    let source_uri = source.to_string_lossy().to_string();
    persist_document(
        &database_path,
        &document_id,
        &title,
        &source_uri,
        &transcript,
        &audio_path,
        &subtitle_srt,
        &subtitle_vtt,
        duration_seconds,
        &segments,
    )?;

    let created_at = Utc::now().to_rfc3339();
    Ok(ImportedDocument {
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
fn list_documents(app: tauri::AppHandle) -> Result<Vec<DocumentSummary>, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("failed to resolve app data directory: {error}"))?;
    ensure_directory_layout(&app_data_dir)?;
    let database_path = database_path_from_app_data(&app_data_dir);
    initialize_database(&database_path)?;

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
            Ok(DocumentSummary {
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
fn get_document(app: tauri::AppHandle, id: String) -> Result<DocumentDetail, String> {
    let document_id = id.trim();
    if document_id.is_empty() {
        return Err("id must not be empty".to_string());
    }

    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("failed to resolve app data directory: {error}"))?;
    ensure_directory_layout(&app_data_dir)?;
    let database_path = database_path_from_app_data(&app_data_dir);
    initialize_database(&database_path)?;

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
                Ok(DocumentDetail {
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
            Ok(TranscriptSegment {
                start_ms: row.get(0)?,
                end_ms: row.get(1)?,
                text: row.get(2)?,
            })
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
fn initialize_app(app: tauri::AppHandle) -> Result<AppBootstrapResult, String> {
    bootstrap_from_app(&app)
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
async fn preflight(app: tauri::AppHandle) -> Result<PreflightResult, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("failed to resolve app data directory: {error}"))?;

    ensure_directory_layout(&app_data_dir)?;

    let mut result = PreflightResult::default();

    match ensure_runtime_binary(&app_data_dir, &WHISPER_BINARY_SPEC).await {
        Ok(message) => {
            record_preflight_detail(&app, &mut result, WHISPER_BINARY_SPEC.check, CheckStatus::Pass, message)
        }
        Err(error) => record_preflight_detail(&app, &mut result, WHISPER_BINARY_SPEC.check, CheckStatus::Fail, error),
    }

    match ensure_runtime_binary(&app_data_dir, &FFMPEG_BINARY_SPEC).await {
        Ok(message) => record_preflight_detail(&app, &mut result, FFMPEG_BINARY_SPEC.check, CheckStatus::Pass, message),
        Err(error) => record_preflight_detail(&app, &mut result, FFMPEG_BINARY_SPEC.check, CheckStatus::Fail, error),
    }

    match ensure_runtime_binary(&app_data_dir, &YT_DLP_BINARY_SPEC).await {
        Ok(message) => record_preflight_detail(&app, &mut result, YT_DLP_BINARY_SPEC.check, CheckStatus::Pass, message),
        Err(error) => record_preflight_detail(
            &app,
            &mut result,
            YT_DLP_BINARY_SPEC.check,
            CheckStatus::Warn,
            format!("{error} URL import remains disabled until yt-dlp is available."),
        ),
    }

    let whisper_model_missing = match whisper_model_present(&app_data_dir.join("models")) {
        Ok(true) => {
            record_preflight_detail(
                &app,
                &mut result,
                PreflightCheck::WhisperModel,
                CheckStatus::Pass,
                "whisper model files are present.",
            );
            false
        }
        Ok(false) => {
            record_preflight_detail(
                &app,
                &mut result,
                PreflightCheck::WhisperModel,
                CheckStatus::Fail,
                "No whisper model found in appdata/models. Open setup to download ggml-base.en.bin.",
            );
            true
        }
        Err(error) => {
            record_preflight_detail(
                &app,
                &mut result,
                PreflightCheck::WhisperModel,
                CheckStatus::Fail,
                error,
            );
            false
        }
    };

    let mut ollama_models_missing = false;
    match fetch_ollama_model_names().await {
        Ok(models) => {
            record_preflight_detail(
                &app,
                &mut result,
                PreflightCheck::OllamaServer,
                CheckStatus::Pass,
                "Ollama server is reachable.",
            );
            let missing_models = missing_required_ollama_models(&models);
            if missing_models.is_empty() {
                record_preflight_detail(
                    &app,
                    &mut result,
                    PreflightCheck::OllamaModels,
                    CheckStatus::Pass,
                    "Required Ollama models are available.",
                );
            } else {
                ollama_models_missing = true;
                record_preflight_detail(
                    &app,
                    &mut result,
                    PreflightCheck::OllamaModels,
                    CheckStatus::Fail,
                    format!(
                        "Missing Ollama models: {}. Pull them with `ollama pull <model>`.",
                        missing_models.join(", ")
                    ),
                );
            }
        }
        Err(error) => {
            record_preflight_detail(
                &app,
                &mut result,
                PreflightCheck::OllamaServer,
                CheckStatus::Fail,
                format!("{error} Start Ollama with `ollama serve`."),
            );
            record_preflight_detail(
                &app,
                &mut result,
                PreflightCheck::OllamaModels,
                CheckStatus::Fail,
                "Required Ollama models could not be verified because the server is unavailable.",
            );
        }
    }

    let database_path = database_path_from_app_data(&app_data_dir);
    match initialize_database(&database_path) {
        Ok(_) => record_preflight_detail(
            &app,
            &mut result,
            PreflightCheck::Database,
            CheckStatus::Pass,
            "SQLite database is accessible and migrations are current.",
        ),
        Err(error) => record_preflight_detail(&app, &mut result, PreflightCheck::Database, CheckStatus::Fail, error),
    }

    let setup_dependencies_ready = !whisper_model_missing && !ollama_models_missing;
    result.should_open_setup = !setup_dependencies_ready;
    result.all_required_passed = compute_all_required_passed(&result);
    set_setup_completed(&database_path, setup_dependencies_ready)?;
    log::info!(
        "preflight finished: all_required_passed={}, setup_dependencies_ready={}, should_open_setup={}",
        result.all_required_passed,
        setup_dependencies_ready,
        result.should_open_setup
    );

    Ok(result)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let app_data_dir = app.path().app_data_dir().map_err(std::io::Error::other)?;
            let log_dir = app_data_dir.join("logs");
            fs::create_dir_all(&log_dir).map_err(std::io::Error::other)?;

            app.handle()
                .plugin(
                    tauri_plugin_log::Builder::new()
                        .level(log::LevelFilter::Info)
                        .targets([
                            tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Stdout),
                            tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Folder {
                                path: log_dir,
                                file_name: Some("audiox".to_string()),
                            }),
                        ])
                        .build(),
                )
                .map_err(std::io::Error::other)?;

            bootstrap_from_app(app.handle()).map_err(std::io::Error::other)?;
            log::info!("Audio X bootstrap complete.");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            initialize_app,
            preflight,
            check_setup,
            download_whisper_model,
            pull_ollama_model,
            import_audio_file,
            list_documents,
            get_document
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};
    #[cfg(unix)]
    use std::{os::unix::fs::PermissionsExt, vec};

    fn temp_dir_path(label: &str) -> PathBuf {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos();

        std::env::temp_dir().join(format!("audiox-{label}-{now}"))
    }

    fn table_exists(connection: &Connection, table_name: &str) -> bool {
        connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
                params![table_name],
                |row| row.get::<_, i64>(0),
            )
            .map(|exists| exists == 1)
            .unwrap_or(false)
    }

    #[test]
    fn bootstrap_creates_required_directories_and_schema() {
        let test_root = temp_dir_path("bootstrap");
        let bootstrap = bootstrap_at(&test_root).expect("bootstrap should succeed");

        for directory_name in REQUIRED_DIRECTORIES {
            assert!(test_root.join(directory_name).is_dir());
        }

        let connection = Connection::open(&bootstrap.database_path).expect("database should be readable");
        assert!(table_exists(&connection, "settings"));
        assert!(table_exists(&connection, "documents"));
        assert!(table_exists(&connection, "document_segments"));
        assert!(table_exists(&connection, "schema_meta"));

        fs::remove_dir_all(test_root).expect("test data should be removed");
    }

    #[test]
    fn bootstrap_is_idempotent_after_first_run() {
        let test_root = temp_dir_path("idempotent");
        let first_bootstrap = bootstrap_at(&test_root).expect("first bootstrap should succeed");
        assert!(!first_bootstrap.created_directories.is_empty());

        let second_bootstrap = bootstrap_at(&test_root).expect("second bootstrap should succeed");
        assert!(second_bootstrap.created_directories.is_empty());
        assert_eq!(first_bootstrap.database_path, second_bootstrap.database_path);

        fs::remove_dir_all(test_root).expect("test data should be removed");
    }

    #[test]
    fn matching_models_accept_tag_suffix_variants() {
        assert!(model_name_matches("nomic-embed-text:latest", "nomic-embed-text"));
        assert!(model_name_matches("gemma3:4b", "gemma3:4b"));
        assert!(model_name_matches("gemma3:latest", "gemma3:4b"));
        assert!(!model_name_matches("gemma3:1b", "gemma3:4b"));
    }

    #[test]
    fn detects_missing_ollama_models() {
        let models = vec!["nomic-embed-text:latest".to_string()];
        let missing = missing_required_ollama_models(&models);
        assert_eq!(missing, vec!["gemma3:4b".to_string()]);
    }

    #[test]
    fn all_required_checks_ignore_optional_warnings() {
        let result = PreflightResult {
            whisper_cli: CheckStatus::Pass,
            ffmpeg: CheckStatus::Pass,
            yt_dlp: CheckStatus::Warn,
            whisper_model: CheckStatus::Pass,
            ollama_server: CheckStatus::Pass,
            ollama_models: CheckStatus::Pass,
            database: CheckStatus::Pass,
            should_open_setup: false,
            all_required_passed: false,
            details: Vec::new(),
        };

        assert!(compute_all_required_passed(&result));
    }

    #[test]
    fn sha256_validation_requires_hex_digest() {
        assert!(is_valid_sha256(
            "2cf24dba5fb0a030e6f0a50b9f8f4c9174f7f9a5f7f37f9e63f7f8b9c2f5f27d"
        ));
        assert!(!is_valid_sha256("12345"));
        assert!(!is_valid_sha256(
            "zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz"
        ));
    }

    #[test]
    fn setup_completion_state_persists_in_settings_table() {
        let test_root = temp_dir_path("setup-complete");
        let bootstrap = bootstrap_at(&test_root).expect("bootstrap should succeed");
        let database_path = PathBuf::from(bootstrap.database_path);

        set_setup_completed(&database_path, true).expect("set_setup_completed should write true");
        let connection = Connection::open(&database_path).expect("database should be readable");
        let value = read_setting(&connection, "setup_completed").expect("setting should be readable");
        assert_eq!(value.as_deref(), Some("true"));

        set_setup_completed(&database_path, false).expect("set_setup_completed should write false");
        let value = read_setting(&connection, "setup_completed").expect("setting should be readable");
        assert_eq!(value.as_deref(), Some("false"));

        fs::remove_dir_all(test_root).expect("test data should be removed");
    }

    #[cfg(unix)]
    #[test]
    fn sidecar_probe_failure_does_not_short_circuit_resolution_fallback() {
        let test_root = temp_dir_path("sidecar-fallback");
        fs::create_dir_all(&test_root).expect("test root should be created");

        let failing_sidecar = test_root.join("audiox-sidecar-fail");
        fs::write(&failing_sidecar, "#!/bin/sh\nexit 9\n").expect("failing sidecar should be written");
        let mut permissions = fs::metadata(&failing_sidecar)
            .expect("failing sidecar metadata should be readable")
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&failing_sidecar, permissions).expect("failing sidecar should be executable");

        let candidate: &'static str = Box::leak(
            failing_sidecar
                .to_string_lossy()
                .to_string()
                .into_boxed_str(),
        );
        let sidecar_candidates: &'static [&'static str] = Box::leak(vec![candidate].into_boxed_slice());

        let spec = RuntimeBinarySpec {
            check: PreflightCheck::WhisperCli,
            tool_id: "audiox-sidecar-fail",
            display_name: "audiox-sidecar-fail",
            version: "runtime",
            executable_stem: "audiox-sidecar-fail",
            version_args: &["--version"],
            path_candidates: &[],
            sidecar_candidates,
            download_url_env: "AUDIOX_SIDE_TEST_URL",
            download_sha256_env: "AUDIOX_SIDE_TEST_SHA256",
            allow_runtime_download: false,
        };

        let runtime = tokio::runtime::Runtime::new().expect("tokio runtime should initialize");
        let direct_probe = runtime
            .block_on(try_sidecar_binary(&spec))
            .expect("sidecar probing should not fail hard");
        assert!(
            direct_probe.is_none(),
            "failing sidecar should fall through to later resolution stages"
        );

        let fallback_result = runtime.block_on(ensure_runtime_binary(&test_root, &spec));
        let error_message = fallback_result.expect_err("resolution should continue and fail with missing guidance");
        assert!(
            error_message.contains("is unavailable"),
            "expected fallback guidance error, got: {error_message}"
        );

        fs::remove_dir_all(test_root).expect("test data should be removed");
    }
}
