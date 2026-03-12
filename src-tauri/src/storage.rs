//! SQLite & FileSystem persistence

use super::{models, parsers};
use chrono::Utc;
use regex::Regex;
use rusqlite::{params, Connection, OptionalExtension};
use std::fs;
use std::path::{Path, PathBuf};
use tauri::{Manager, State};
use uuid::Uuid;

pub struct PersistDocumentInput<'a> {
    pub document_id: &'a str,
    pub source_type: &'a str,
    pub title: &'a str,
    pub summary: Option<&'a str>,
    pub keywords_csv: Option<&'a str>,
    pub source_uri: &'a str,
    pub transcript: &'a str,
    pub audio_path: &'a str,
    pub subtitle_srt_path: &'a str,
    pub subtitle_vtt_path: &'a str,
    pub duration_seconds: i64,
    pub segments: &'a [models::TranscriptSegment],
    pub chunks: &'a [models::EmbeddedChunk],
}

#[derive(Clone, Debug)]
pub struct FileStore {
    app_data_dir: PathBuf,
}

impl FileStore {
    pub fn new(app_data_dir: impl Into<PathBuf>) -> Self {
        Self { app_data_dir: app_data_dir.into() }
    }

    pub fn from_path(app_data_dir: &Path) -> Self {
        Self::new(app_data_dir.to_path_buf())
    }

    pub fn app_data_dir(&self) -> &Path {
        &self.app_data_dir
    }

    pub fn bootstrap_at(&self) -> Result<models::AppBootstrapResult, String> {
        let created_directories = self.ensure_directory_layout()?;
        let database_path = self.database_path_from_app_data();
        let data_store = DataStore::new(&database_path)?;
        data_store.initialize_database()?;

        Ok(models::AppBootstrapResult {
            app_data_dir: self.app_data_dir.display().to_string(),
            database_path: database_path.display().to_string(),
            created_directories,
            schema_version: models::SCHEMA_VERSION,
        })
    }

    pub fn ensure_directory_layout(&self) -> Result<Vec<String>, String> {
        fs::create_dir_all(&self.app_data_dir).map_err(|error| {
            format!(
                "failed to create app data directory at {}: {error}",
                self.app_data_dir.display()
            )
        })?;

        let mut created_directories = Vec::new();
        for directory_name in models::REQUIRED_DIRECTORIES {
            let directory_path = self.app_data_dir.join(directory_name);
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

    pub fn path_for_storage(&self, path: &Path) -> String {
        if let Ok(relative) = path.strip_prefix(&self.app_data_dir) {
            return relative.to_string_lossy().to_string();
        }
        path.to_string_lossy().to_string()
    }

    pub fn resolve_storage_path(&self, stored_path: &str) -> PathBuf {
        let candidate = PathBuf::from(stored_path);
        if candidate.is_absolute() {
            return candidate;
        }
        self.app_data_dir.join(candidate)
    }

    pub fn resolve_whisper_model_path(&self) -> Result<PathBuf, String> {
        let model_dir = self.app_data_dir.join("models");
        let preferred = model_dir.join(parsers::whisper_model_file_name(models::WHISPER_DEFAULTS.model_name));
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
            parsers::whisper_model_file_name(models::WHISPER_DEFAULTS.model_name)
        ))
    }

    pub fn database_path_from_app_data(&self) -> PathBuf {
        self.app_data_dir.join("db").join("audiox.db")
    }
}

pub struct DataStore {
    connection: Connection,
}

impl DataStore {
    pub fn new(database_path: &Path) -> Result<Self, String> {
        let connection = Connection::open(database_path)
            .map_err(|error| format!("failed to open database {}: {error}", database_path.display()))?;
        Ok(Self { connection })
    }

    fn upsert_setting(&self, key: &str, value: &str) -> Result<(), String> {
        let key_pattern =
            Regex::new(r"^[a-z0-9_]+$").map_err(|error| format!("failed to compile key validation regex: {error}"))?;

        if !key_pattern.is_match(key) {
            return Err(format!("setting key '{key}' is invalid"));
        }

        self.connection
            .execute(
                "INSERT INTO settings (key, value, updated_at) VALUES (?1, ?2, ?3)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
                params![key, value, Utc::now().to_rfc3339()],
            )
            .map_err(|error| format!("failed to write setting '{key}': {error}"))?;

        Ok(())
    }

    fn has_column(&self, table: &str, column: &str) -> Result<bool, String> {
        let mut statement = self
            .connection
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

    fn ensure_documents_table_columns(&self) -> Result<(), String> {
        let required_columns = [
            ("audio_path", "TEXT"),
            ("subtitle_srt_path", "TEXT"),
            ("subtitle_vtt_path", "TEXT"),
            ("keywords", "TEXT"),
        ];

        for (column_name, definition) in required_columns {
            if self.has_column("documents", column_name)? {
                continue;
            }

            self.connection
                .execute(
                    &format!("ALTER TABLE documents ADD COLUMN {column_name} {definition}"),
                    [],
                )
                .map_err(|error| format!("failed to add documents.{column_name}: {error}"))?;
        }

        Ok(())
    }

    pub fn set_setup_completed(&self, completed: bool) -> Result<(), String> {
        let value = if completed { "true" } else { "false" };
        self.upsert_setting("setup_completed", value)?;
        if completed {
            self.upsert_setting("setup_completed_at", &Utc::now().to_rfc3339())?;
        }
        Ok(())
    }

    pub fn initialize_database(&self) -> Result<(), String> {
        self.connection
            .execute_batch(
                "
          PRAGMA journal_mode = WAL;
          PRAGMA foreign_keys = ON;
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
            keywords TEXT,
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

          CREATE TABLE IF NOT EXISTS chunks (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            document_id TEXT NOT NULL,
            chunk_index INTEGER NOT NULL,
            content TEXT NOT NULL,
            embedding BLOB NOT NULL,
            created_at TEXT NOT NULL,
            FOREIGN KEY(document_id) REFERENCES documents(id) ON DELETE CASCADE,
            UNIQUE(document_id, chunk_index)
          );

          CREATE INDEX IF NOT EXISTS idx_chunks_document ON chunks(document_id);
          CREATE INDEX IF NOT EXISTS idx_documents_created ON documents(created_at);
        ",
            )
            .map_err(|error| format!("failed to initialize schema: {error}"))?;

        self.ensure_documents_table_columns()?;

        self.connection
            .execute(
                "INSERT INTO schema_meta (key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params!["schema_version", models::SCHEMA_VERSION.to_string()],
            )
            .map_err(|error| format!("failed to persist schema version: {error}"))?;

        let installation_id = self
            .connection
            .query_row("SELECT value FROM settings WHERE key = 'installation_id'", [], |row| {
                row.get::<_, String>(0)
            })
            .optional()
            .map_err(|error| format!("failed to read installation id: {error}"))?;

        if installation_id.is_none() {
            self.upsert_setting("installation_id", &Uuid::new_v4().to_string())?;
        }
        self.upsert_setting("last_bootstrap_at", &Utc::now().to_rfc3339())?;

        Ok(())
    }

    pub fn persist_document(&mut self, input: &PersistDocumentInput<'_>) -> Result<(), String> {
        let transaction = self
            .connection
            .unchecked_transaction()
            .map_err(|error| format!("failed to start persistence transaction: {error}"))?;

        let now = Utc::now().to_rfc3339();
        transaction
            .execute(
                "INSERT INTO documents (
                    id, source_type, source_uri, title, summary, keywords, transcript, audio_path, subtitle_srt_path, subtitle_vtt_path,
                    duration_seconds, created_at, updated_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                params![
                    input.document_id,
                    input.source_type,
                    input.source_uri,
                    input.title,
                    input.summary,
                    input.keywords_csv,
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

        let mut chunk_statement = transaction
            .prepare(
                "INSERT INTO chunks (document_id, chunk_index, content, embedding, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )
            .map_err(|error| format!("failed to prepare chunk insert statement: {error}"))?;
        for chunk in input.chunks {
            chunk_statement
                .execute(params![
                    input.document_id,
                    chunk.chunk_index,
                    chunk.content,
                    embedding_to_blob(&chunk.embedding),
                    now
                ])
                .map_err(|error| format!("failed to insert chunk for {}: {error}", input.document_id))?;
        }

        drop(chunk_statement);
        drop(statement);
        transaction.commit().map_err(|error| {
            format!(
                "failed to commit document transaction for {}: {error}",
                input.document_id
            )
        })?;
        Ok(())
    }
}

pub fn parse_setting_bool(value: Option<String>) -> bool {
    value
        .map(|item| item.trim().eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

#[derive(Clone, Debug)]
pub struct StorageState {
    file_store: FileStore,
    database_path: PathBuf,
}

impl StorageState {
    pub fn from_app_data_dir(app_data_dir: impl Into<PathBuf>) -> Self {
        let file_store = FileStore::new(app_data_dir);
        let database_path = file_store.database_path_from_app_data();
        Self { file_store, database_path }
    }

    pub fn app_data_dir(&self) -> &Path {
        self.file_store.app_data_dir()
    }

    pub fn database_path(&self) -> &Path {
        &self.database_path
    }
}

pub fn state_from_manager(app: &tauri::AppHandle) -> State<'_, StorageState> {
    app.state::<StorageState>()
}

pub fn bootstrap_at(app_data_dir: &Path) -> Result<models::AppBootstrapResult, String> {
    FileStore::from_path(app_data_dir).bootstrap_at()
}

pub fn ensure_directory_layout(app_data_dir: &Path) -> Result<Vec<String>, String> {
    FileStore::from_path(app_data_dir).ensure_directory_layout()
}

pub fn database_path_from_app_data(app_data_dir: &Path) -> PathBuf {
    FileStore::from_path(app_data_dir).database_path_from_app_data()
}

pub fn initialize_database(database_path: &Path) -> Result<(), String> {
    DataStore::new(database_path)?.initialize_database()
}

pub fn set_setup_completed(database_path: &Path, completed: bool) -> Result<(), String> {
    DataStore::new(database_path)?.set_setup_completed(completed)
}

pub fn read_setting(connection: &Connection, key: &str) -> Result<Option<String>, String> {
    connection
        .query_row("SELECT value FROM settings WHERE key = ?1", params![key], |row| {
            row.get::<_, String>(0)
        })
        .optional()
        .map_err(|error| format!("failed to read setting '{key}': {error}"))
}

pub fn path_for_storage(path: &Path, app_data_dir: &Path) -> String {
    FileStore::from_path(app_data_dir).path_for_storage(path)
}

pub fn resolve_storage_path(app_data_dir: &Path, stored_path: &str) -> PathBuf {
    FileStore::from_path(app_data_dir).resolve_storage_path(stored_path)
}

pub fn resolve_whisper_model_path(app_data_dir: &Path) -> Result<PathBuf, String> {
    FileStore::from_path(app_data_dir).resolve_whisper_model_path()
}

pub fn persist_document(database_path: &Path, input: &PersistDocumentInput<'_>) -> Result<(), String> {
    let mut data_store = DataStore::new(database_path)?;
    data_store.persist_document(input)
}

pub fn bootstrap_from_app(app: &tauri::AppHandle) -> Result<models::AppBootstrapResult, String> {
    let state = state_from_manager(app);
    bootstrap_at(state.app_data_dir())
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

pub fn embedding_model_present(embed_dir: &Path) -> Result<bool, String> {
    if !embed_dir.exists() {
        return Ok(false);
    }

    let mut stack = vec![embed_dir.to_path_buf()];
    while let Some(directory) = stack.pop() {
        let entries = fs::read_dir(&directory)
            .map_err(|error| format!("failed to list embedding directory {}: {error}", directory.display()))?;

        for entry in entries {
            let entry = entry
                .map_err(|error| format!("failed to inspect embedding directory {}: {error}", directory.display()))?;
            let path = entry.path();
            if path.is_file() {
                return Ok(true);
            }
            if path.is_dir() {
                stack.push(path);
            }
        }
    }

    Ok(false)
}

fn embedding_to_blob(embedding: &[f32]) -> Vec<u8> {
    let mut blob = Vec::with_capacity(std::mem::size_of_val(embedding));
    for value in embedding {
        blob.extend_from_slice(&value.to_le_bytes());
    }
    blob
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
