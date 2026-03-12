//! Setup and Application Bootstrapping

use crate::models;
use crate::parsers::{
    calculate_percent, is_valid_sha256, missing_required_ollama_models, normalize_sha256, parse_ffmpeg_duration_ms,
    parse_ffmpeg_out_time_ms, parse_ollama_model_names, parse_progress_percent, parse_whisper_segments,
    whisper_model_download_url, whisper_model_file_name,
};
use crate::storage::{
    database_path_from_app_data, embedding_model_present, ensure_directory_layout, initialize_database,
    parse_setting_bool, read_setting, set_setup_completed, whisper_model_present,
};
use reqwest::StatusCode;
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
use tauri::Emitter;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};

pub fn summarize_command_output(stderr: &[u8], stdout: &[u8]) -> String {
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

fn detail_status_mut(result: &mut models::PreflightResult, check: models::PreflightCheck) -> &mut models::CheckStatus {
    match check {
        models::PreflightCheck::WhisperCli => &mut result.whisper_cli,
        models::PreflightCheck::Ffmpeg => &mut result.ffmpeg,
        models::PreflightCheck::YtDlp => &mut result.yt_dlp,
        models::PreflightCheck::WhisperModel => &mut result.whisper_model,
        models::PreflightCheck::EmbeddingModel => &mut result.embedding_model,
        models::PreflightCheck::OllamaServer => &mut result.ollama_server,
        models::PreflightCheck::OllamaModels => &mut result.ollama_models,
        models::PreflightCheck::Database => &mut result.database,
    }
}

fn emit_preflight_detail(app: &tauri::AppHandle, detail: &models::PreflightCheckDetail) {
    let _ = app.emit(models::PREFLIGHT_EVENT, detail.clone());
}

pub fn record_preflight_detail(
    app: &tauri::AppHandle, result: &mut models::PreflightResult, check: models::PreflightCheck,
    status: models::CheckStatus, message: impl Into<String>,
) {
    let detail = models::PreflightCheckDetail { check, status, message: message.into() };
    *detail_status_mut(result, check) = status;
    result.details.push(detail.clone());
    emit_preflight_detail(app, &detail);
}

fn managed_binary_filename(spec: &models::RuntimeBinarySpec) -> String {
    #[cfg(target_os = "windows")]
    {
        format!("{}.exe", spec.executable_stem)
    }

    #[cfg(not(target_os = "windows"))]
    {
        spec.executable_stem.to_string()
    }
}

fn managed_binary_path(app_data_dir: &Path, spec: &models::RuntimeBinarySpec) -> PathBuf {
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

fn sidecar_candidate_paths(spec: &models::RuntimeBinarySpec) -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    if let Ok(current_exe) = std::env::current_exe() {
        if let Some(exe_dir) = current_exe.parent() {
            for candidate in spec.sidecar_candidates {
                candidates.push(normalize_executable_path(exe_dir.join(candidate)));
            }
            candidates.extend(read_sidecar_candidates_from_directory(exe_dir, spec.executable_stem));
            let sidecar_dir = exe_dir.join("binaries");
            candidates.extend(read_sidecar_candidates_from_directory(
                &sidecar_dir,
                spec.executable_stem,
            ));
        }
    }

    let manifest_sidecar_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("binaries");
    for candidate in spec.sidecar_candidates {
        candidates.push(normalize_executable_path(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(candidate),
        ));
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

    let output = tokio::time::timeout(models::Timeouts::Command.into(), command.output())
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

fn configured_download_url(spec: &models::RuntimeBinarySpec) -> Result<Option<String>, String> {
    env_var(spec.download_url_env)
}

fn configured_download_sha256(spec: &models::RuntimeBinarySpec) -> Result<Option<String>, String> {
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
        .timeout(models::Timeouts::Download.into())
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

async fn try_system_binary(spec: &models::RuntimeBinarySpec) -> Option<String> {
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

fn download_guidance(spec: &models::RuntimeBinarySpec) -> String {
    format!(
        "{} is missing. Install '{}' on PATH or configure {} and {} to allow runtime download.",
        spec.display_name, spec.executable_stem, spec.download_url_env, spec.download_sha256_env
    )
}

fn non_download_guidance(spec: &models::RuntimeBinarySpec) -> String {
    format!(
        "{} is unavailable. Reinstall Audio X to restore bundled dependencies. For local development, run `bash setup.sh` and ensure '{}' is installed on PATH.",
        spec.display_name, spec.executable_stem
    )
}

pub async fn try_sidecar_binary(spec: &models::RuntimeBinarySpec) -> Result<Option<PathBuf>, String> {
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

async fn resolve_runtime_binary(
    app_data_dir: &Path, spec: &models::RuntimeBinarySpec,
) -> Result<ResolvedBinary, String> {
    if let Some(sidecar_path) = try_sidecar_binary(spec).await? {
        return Ok(ResolvedBinary {
            program: sidecar_path.display().to_string(),
            message: format!(
                "{} sidecar is available at {}.",
                spec.display_name,
                sidecar_path.display()
            ),
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

pub async fn resolve_runtime_binary_program(
    app_data_dir: &Path, spec: &models::RuntimeBinarySpec,
) -> Result<String, String> {
    let resolved = resolve_runtime_binary(app_data_dir, spec).await?;
    Ok(resolved.program)
}

pub async fn ensure_runtime_binary(app_data_dir: &Path, spec: &models::RuntimeBinarySpec) -> Result<String, String> {
    let resolved = resolve_runtime_binary(app_data_dir, spec).await?;
    Ok(resolved.message)
}

pub async fn fetch_ollama_model_names() -> Result<Vec<String>, String> {
    let tags_url = models::OllamaUrl::Tags.as_str();
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .map_err(|error| format!("failed to initialize HTTP client: {error}"))?;

    let response = client
        .get(tags_url)
        .send()
        .await
        .map_err(|error| format!("failed to reach Ollama at {tags_url}: {error}"))?;

    if !response.status().is_success() {
        return Err(format!("Ollama responded with unexpected status {}", response.status()));
    }

    let tags_payload = response
        .json::<Value>()
        .await
        .map_err(|error| format!("failed to parse Ollama tags response: {error}"))?;

    Ok(parse_ollama_model_names(&tags_payload))
}

pub fn emit_whisper_progress(
    app: &tauri::AppHandle, model_name: &str, status: &str, message: impl Into<String>, downloaded_bytes: u64,
    total_bytes: Option<u64>,
) {
    let percent = total_bytes
        .map(|total| calculate_percent(downloaded_bytes, total))
        .unwrap_or(0.0);
    let payload = models::WhisperDownloadProgress {
        model_name: model_name.to_string(),
        status: status.to_string(),
        message: message.into(),
        downloaded_bytes,
        total_bytes,
        percent,
    };
    let _ = app.emit(models::ProgressEvent::SetupWhisper.as_str(), payload);
}

pub fn emit_ollama_progress(
    app: &tauri::AppHandle, model_name: &str, status: &str, message: impl Into<String>, completed: u64, total: u64,
) {
    let payload = models::OllamaPullProgress {
        model_name: model_name.to_string(),
        status: status.to_string(),
        message: message.into(),
        completed,
        total,
        percent: calculate_percent(completed, total),
    };
    let _ = app.emit(models::ProgressEvent::SetupOllama.as_str(), payload);
}

pub async fn download_whisper_model_file(
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
        .timeout(models::Timeouts::Download.into())
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
    whisper_model_ready: bool, embedding_model_ready: bool, ollama_server_ready: bool,
    missing_ollama_models: &[String], ollama_error: Option<&str>,
) -> Vec<String> {
    let mut guidance = Vec::new();
    if !whisper_model_ready {
        guidance.push(format!(
            "Download {} into appdata/models to enable transcription.",
            whisper_model_file_name(models::WHISPER_DEFAULTS.model_name)
        ));
    }
    if !embedding_model_ready {
        guidance.push(
            "Download the local embedding model into appdata/models/embed to enable semantic search.".to_string(),
        );
    }
    if !ollama_server_ready {
        let suffix = ollama_error.unwrap_or("Ollama did not respond.");
        guidance.push(format!(
            "{suffix} Install Ollama from https://ollama.com and start it with `ollama serve` for title/summary/tag generation."
        ));
    } else if !missing_ollama_models.is_empty() {
        guidance.push(format!(
            "Pull missing Ollama models for metadata generation: {}.",
            missing_ollama_models.join(", ")
        ));
    }

    guidance
}

pub async fn check_setup_state(app_data_dir: &Path) -> Result<models::SetupStatus, String> {
    ensure_directory_layout(app_data_dir)?;
    let database_path = database_path_from_app_data(app_data_dir);
    initialize_database(&database_path)?;

    let whisper_model_ready = whisper_model_present(&app_data_dir.join("models"))?;
    let embedding_model_ready = embedding_model_present(&app_data_dir.join("models").join("embed"))?;
    let (ollama_server_ready, missing_ollama_models, ollama_error) = match fetch_ollama_model_names().await {
        Ok(models) => {
            let missing = missing_required_ollama_models(&models);
            (true, missing, None)
        }
        Err(error) => (
            false,
            crate::models::REQUIRED_OLLAMA_MODELS
                .iter()
                .map(|item| (*item).to_string())
                .collect(),
            Some(error),
        ),
    };

    let all_required_ready = whisper_model_ready && embedding_model_ready;
    set_setup_completed(&database_path, all_required_ready)?;

    let connection = rusqlite::Connection::open(&database_path)
        .map_err(|error| format!("failed to open database {}: {error}", database_path.display()))?;
    let setup_completed = parse_setting_bool(read_setting(&connection, "setup_completed")?);

    Ok(models::SetupStatus {
        whisper_model_ready,
        embedding_model_ready,
        ollama_server_ready,
        missing_ollama_models: missing_ollama_models.clone(),
        setup_completed,
        all_required_ready,
        guidance: compute_setup_guidance(
            whisper_model_ready,
            embedding_model_ready,
            ollama_server_ready,
            &missing_ollama_models,
            ollama_error.as_deref(),
        ),
    })
}

pub fn compute_all_required_passed(result: &models::PreflightResult) -> bool {
    ![
        result.whisper_cli,
        result.ffmpeg,
        result.whisper_model,
        result.embedding_model,
        result.database,
    ]
    .contains(&models::CheckStatus::Fail)
}

fn emit_conversion_progress(
    app: &tauri::AppHandle, status: &str, message: impl Into<String>, out_time_ms: i64, total_duration_ms: Option<i64>,
) {
    let percent = total_duration_ms
        .map(|total| calculate_percent(u64::try_from(out_time_ms.max(0)).unwrap_or_default(), total as u64))
        .unwrap_or(0.0);

    let payload = models::ConversionProgress {
        status: status.to_string(),
        message: message.into(),
        out_time_ms,
        total_duration_ms,
        percent,
    };
    let _ = app.emit(models::ProgressEvent::ImportConversion.as_str(), payload);
}

fn emit_transcription_progress(app: &tauri::AppHandle, status: &str, message: impl Into<String>, percent: f64) {
    let payload = models::TranscriptionProgress {
        status: status.to_string(),
        message: message.into(),
        percent: percent.clamp(0.0, 100.0),
    };
    let _ = app.emit(models::ProgressEvent::ImportTranscription.as_str(), payload);
}

async fn probe_ffmpeg_duration_ms(ffmpeg_program: &str, input_path: &Path) -> Result<Option<i64>, String> {
    let mut command = tokio::process::Command::new(ffmpeg_program);
    command.arg("-i").arg(input_path);
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());

    let output = tokio::time::timeout(models::Timeouts::Command.into(), command.output())
        .await
        .map_err(|_| format!("timed out while probing media duration for {}", input_path.display()))?
        .map_err(|error| format!("failed to run ffmpeg duration probe: {error}"))?;

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(parse_ffmpeg_duration_ms(&format!("{stderr}\n{stdout}")))
}

pub async fn run_ffmpeg_conversion(
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
        emit_conversion_progress(
            app,
            "error",
            "Audio conversion failed.",
            latest_out_time_ms,
            total_duration_ms,
        );
        return Err(format!(
            "ffmpeg conversion failed: {}",
            summarize_command_output(stderr_output.as_bytes(), &[])
        ));
    }

    Ok(())
}

pub async fn run_whisper_transcription(
    app: &tauri::AppHandle, whisper_program: &str, model_path: &Path, wav_path: &Path, output_base: &Path,
) -> Result<Vec<crate::models::TranscriptSegment>, String> {
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
        .arg(models::WHISPER_DEFAULTS.threads.to_string())
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
                emit_transcription_progress(
                    app,
                    "running",
                    "Transcribing audio with whisper-cli...",
                    highest_progress,
                );
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_required_checks_ignore_optional_warnings() {
        use models::{CheckStatus, PreflightResult};

        let result = PreflightResult {
            whisper_cli: CheckStatus::Pass,
            ffmpeg: CheckStatus::Pass,
            yt_dlp: CheckStatus::Warn,
            whisper_model: CheckStatus::Pass,
            embedding_model: CheckStatus::Pass,
            ollama_server: CheckStatus::Warn,
            ollama_models: CheckStatus::Warn,
            database: CheckStatus::Pass,
            should_open_setup: false,
            all_required_passed: false,
            details: Vec::new(),
        };

        assert!(compute_all_required_passed(&result));
    }
}
