use chrono::Utc;
use regex::Regex;
use reqwest::StatusCode;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::ffi::OsStr;
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tauri::{Emitter, Manager};
use uuid::Uuid;

const REQUIRED_DIRECTORIES: [&str; 6] = ["models", "audio", "video", "subtitles", "db", "bin"];
const REQUIRED_OLLAMA_MODELS: [&str; 2] = ["nomic-embed-text", "gemma3:4b"];
const SCHEMA_VERSION: i64 = 1;
const PREFLIGHT_EVENT: &str = "preflight://check";
const OLLAMA_TAGS_URL: &str = "http://localhost:11434/api/tags";
const COMMAND_TIMEOUT_SECONDS: u64 = 8;
const DOWNLOAD_TIMEOUT_SECONDS: u64 = 120;

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
    download_url_env: &'static str,
    download_sha256_env: &'static str,
}

const WHISPER_BINARY_SPEC: RuntimeBinarySpec = RuntimeBinarySpec {
    check: PreflightCheck::WhisperCli,
    tool_id: "whisper-cli",
    display_name: "whisper-cli",
    version: "runtime",
    executable_stem: "whisper-cli",
    version_args: &["--version"],
    path_candidates: &["whisper-cli"],
    download_url_env: "AUDIOX_WHISPER_URL",
    download_sha256_env: "AUDIOX_WHISPER_SHA256",
};

const FFMPEG_BINARY_SPEC: RuntimeBinarySpec = RuntimeBinarySpec {
    check: PreflightCheck::Ffmpeg,
    tool_id: "ffmpeg",
    display_name: "ffmpeg",
    version: "runtime",
    executable_stem: "ffmpeg",
    version_args: &["-version"],
    path_candidates: &["ffmpeg"],
    download_url_env: "AUDIOX_FFMPEG_URL",
    download_sha256_env: "AUDIOX_FFMPEG_SHA256",
};

const YT_DLP_BINARY_SPEC: RuntimeBinarySpec = RuntimeBinarySpec {
    check: PreflightCheck::YtDlp,
    tool_id: "yt-dlp",
    display_name: "yt-dlp",
    version: "runtime",
    executable_stem: "yt-dlp",
    version_args: &["--version"],
    path_candidates: &["yt-dlp", "yt_dlp"],
    download_url_env: "AUDIOX_YTDLP_URL",
    download_sha256_env: "AUDIOX_YTDLP_SHA256",
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
    let database_path = app_data_dir.join("db").join("audiox.db");
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

async fn ensure_runtime_binary(app_data_dir: &Path, spec: &RuntimeBinarySpec) -> Result<String, String> {
    let binary_path = managed_binary_path(app_data_dir, spec);

    if binary_path.is_file() {
        let label = binary_path.display().to_string();
        execute_version_command(&binary_path, &label, spec.version_args).await?;
        return Ok(format!(
            "{} is available at {}.",
            spec.display_name,
            binary_path.display()
        ));
    }

    if let Some(system_binary) = try_system_binary(spec).await {
        return Ok(format!(
            "{} is available on PATH as '{system_binary}'.",
            spec.display_name
        ));
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
    Ok(format!(
        "Downloaded {} to {} (sha256: {}).",
        spec.display_name,
        binary_path.display(),
        installed_sha256
    ))
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

    candidate == required
        || candidate.strip_suffix(":latest") == Some(required.as_str())
        || candidate.starts_with(&format!("{required}:"))
        || required.starts_with(&format!("{candidate}:"))
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

    let database_path = app_data_dir.join("db").join("audiox.db");
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

    result.should_open_setup = whisper_model_missing || ollama_models_missing;
    result.all_required_passed = compute_all_required_passed(&result);

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
            bootstrap_from_app(app.handle()).map_err(std::io::Error::other)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![initialize_app, preflight])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

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
}
