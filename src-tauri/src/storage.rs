//! SQLite & FileSystem persistence

use super::{models, parsers};
use chrono::Utc;
use regex::Regex;
use rusqlite::{params, Connection, OptionalExtension};
use std::fs;
use std::path::{Path, PathBuf};
use tauri::Manager;
use uuid::Uuid;

pub struct PersistDocumentInput<'a> {
    pub document_id: &'a str,
    pub source_type: &'a str,
    pub title: &'a str,
    pub source_uri: &'a str,
    pub transcript: &'a str,
    pub audio_path: &'a str,
    pub subtitle_srt_path: &'a str,
    pub subtitle_vtt_path: &'a str,
    pub duration_seconds: i64,
    pub segments: &'a [models::TranscriptSegment],
}

pub fn ensure_directory_layout(app_data_dir: &Path) -> Result<Vec<String>, String> {
    fs::create_dir_all(app_data_dir).map_err(|error| {
        format!(
            "failed to create app data directory at {}: {error}",
            app_data_dir.display()
        )
    })?;

    let mut created_directories = Vec::new();
    for directory_name in models::REQUIRED_DIRECTORIES {
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

pub fn read_setting(connection: &Connection, key: &str) -> Result<Option<String>, String> {
    connection
        .query_row("SELECT value FROM settings WHERE key = ?1", params![key], |row| {
            row.get::<_, String>(0)
        })
        .optional()
        .map_err(|error| format!("failed to read setting '{key}': {error}"))
}

pub fn parse_setting_bool(value: Option<String>) -> bool {
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

pub fn database_path_from_app_data(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("db").join("audiox.db")
}

pub fn set_setup_completed(database_path: &Path, completed: bool) -> Result<(), String> {
    let connection = Connection::open(database_path)
        .map_err(|error| format!("failed to open database {}: {error}", database_path.display()))?;

    let value = if completed { "true" } else { "false" };
    upsert_setting(&connection, "setup_completed", value)?;
    if completed {
        upsert_setting(&connection, "setup_completed_at", &Utc::now().to_rfc3339())?;
    }
    Ok(())
}

pub fn initialize_database(database_path: &Path) -> Result<(), String> {
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
            params!["schema_version", models::SCHEMA_VERSION.to_string()],
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

pub fn bootstrap_at(app_data_dir: &Path) -> Result<models::AppBootstrapResult, String> {
    let created_directories = ensure_directory_layout(app_data_dir)?;
    let database_path = database_path_from_app_data(app_data_dir);
    initialize_database(&database_path)?;

    Ok(models::AppBootstrapResult {
        app_data_dir: app_data_dir.display().to_string(),
        database_path: database_path.display().to_string(),
        created_directories,
        schema_version: models::SCHEMA_VERSION,
    })
}

pub fn bootstrap_from_app(app: &tauri::AppHandle) -> Result<models::AppBootstrapResult, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("failed to resolve app data directory: {error}"))?;
    bootstrap_at(&app_data_dir)
}

pub fn whisper_model_present(models_dir: &Path) -> Result<bool, String> {
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

pub fn path_for_storage(path: &Path, app_data_dir: &Path) -> String {
    if let Ok(relative) = path.strip_prefix(app_data_dir) {
        return relative.to_string_lossy().to_string();
    }
    path.to_string_lossy().to_string()
}

pub fn resolve_whisper_model_path(app_data_dir: &Path) -> Result<PathBuf, String> {
    let model_dir = app_data_dir.join("models");
    let preferred = model_dir.join(parsers::whisper_model_file_name(models::DEFAULT_WHISPER_MODEL_NAME));
    if preferred.is_file() {
        return Ok(preferred);
    }

    let entries = fs::read_dir(&model_dir)
        .map_err(|error| format!("failed to read models directory {}: {error}", model_dir.display()))?;
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
        parsers::whisper_model_file_name(models::DEFAULT_WHISPER_MODEL_NAME)
    ))
}

pub fn persist_document(database_path: &Path, input: &PersistDocumentInput<'_>) -> Result<(), String> {
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
                input.document_id,
                input.source_type,
                input.source_uri,
                input.title,
                Option::<String>::None,
                input.transcript,
                input.audio_path,
                input.subtitle_srt_path,
                input.subtitle_vtt_path,
                input.duration_seconds,
                now,
                now
            ],
        )
        .map_err(|error| format!("failed to insert document {}: {error}", input.document_id))?;

    let mut statement = transaction
        .prepare(
            "INSERT INTO document_segments (id, document_id, start_ms, end_ms, text, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )
        .map_err(|error| format!("failed to prepare segment insert statement: {error}"))?;
    for segment in input.segments {
        statement
            .execute(params![
                Uuid::new_v4().to_string(),
                input.document_id,
                segment.start_ms,
                segment.end_ms,
                segment.text,
                now
            ])
            .map_err(|error| format!("failed to insert document segment for {}: {error}", input.document_id))?;
    }

    drop(statement);
    transaction.commit().map_err(|error| {
        format!(
            "failed to commit document transaction for {}: {error}",
            input.document_id
        )
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir_path(label: &str) -> PathBuf {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos();

        std::env::temp_dir().join(format!("audiox-{label}-{now}"))
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
}
