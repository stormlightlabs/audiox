use chrono::Utc;
use regex::Regex;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::fs;
use std::path::Path;
use tauri::Manager;
use uuid::Uuid;

const REQUIRED_DIRECTORIES: [&str; 5] = ["models", "audio", "video", "subtitles", "db"];
const SCHEMA_VERSION: i64 = 1;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AppBootstrapResult {
    app_data_dir: String,
    database_path: String,
    created_directories: Vec<String>,
    schema_version: i64,
}

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

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
fn initialize_app(app: tauri::AppHandle) -> Result<AppBootstrapResult, String> {
    bootstrap_from_app(&app)
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
        .invoke_handler(tauri::generate_handler![initialize_app])
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
}
