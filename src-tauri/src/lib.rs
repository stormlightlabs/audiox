mod bootstrap;
mod commands;
mod models;
mod parsers;
mod storage;

use std::fs;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_audio_recorder::init())
        .setup(|app| {
            let app_data_dir = app.path().app_data_dir().map_err(std::io::Error::other)?;
            let log_dir = app_data_dir.join("logs");

            fs::create_dir_all(&log_dir).map_err(std::io::Error::other)?;

            app.manage(storage::StorageState::from_app_data_dir(app_data_dir));

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

            storage::bootstrap_from_app(app.handle()).map_err(std::io::Error::other)?;
            log::info!("Audio X bootstrap complete.");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::initialize_app,
            commands::preflight,
            commands::check_setup,
            commands::download_whisper_model,
            commands::pull_ollama_model,
            commands::import_audio_file,
            commands::import_recorded_audio,
            commands::list_documents,
            commands::get_document,
            commands::update_document,
            commands::delete_document,
            commands::search
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::{params, Connection};
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
        let bootstrap = storage::bootstrap_at(&test_root).expect("bootstrap should succeed");

        for directory_name in models::REQUIRED_DIRECTORIES {
            assert!(test_root.join(directory_name).is_dir());
        }

        let connection = Connection::open(&bootstrap.database_path).expect("database should be readable");
        assert!(table_exists(&connection, "settings"));
        assert!(table_exists(&connection, "documents"));
        assert!(table_exists(&connection, "document_segments"));
        assert!(table_exists(&connection, "chunks"));
        assert!(table_exists(&connection, "schema_meta"));

        fs::remove_dir_all(test_root).expect("test data should be removed");
    }

    #[test]
    fn bootstrap_is_idempotent_after_first_run() {
        let test_root = temp_dir_path("idempotent");
        let first_bootstrap = storage::bootstrap_at(&test_root).expect("first bootstrap should succeed");
        assert!(!first_bootstrap.created_directories.is_empty());

        let second_bootstrap = storage::bootstrap_at(&test_root).expect("second bootstrap should succeed");
        assert!(second_bootstrap.created_directories.is_empty());
        assert_eq!(first_bootstrap.database_path, second_bootstrap.database_path);

        fs::remove_dir_all(test_root).expect("test data should be removed");
    }

    #[test]
    fn matching_models_accept_gemma_family_variants() {
        assert!(parsers::model_name_matches(
            "nomic-embed-text:latest",
            "nomic-embed-text"
        ));
        assert!(parsers::model_name_matches("gemma3:4b", "gemma3:4b"));
        assert!(parsers::model_name_matches("gemma3:latest", "gemma3"));
        assert!(parsers::model_name_matches("gemma3:1b", "gemma3"));
        assert!(parsers::model_name_matches("gemma3:27b-cloud", "gemma3"));
        assert!(!parsers::model_name_matches("gemma2:9b", "gemma3"));
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

        let candidate: &'static str = Box::leak(failing_sidecar.to_string_lossy().to_string().into_boxed_str());
        let sidecar_candidates: &'static [&'static str] = Box::leak(vec![candidate].into_boxed_slice());

        let spec = models::RuntimeBinarySpec {
            check: models::PreflightCheck::WhisperCli,
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
            .block_on(bootstrap::try_sidecar_binary(&spec))
            .expect("sidecar probing should not fail hard");
        assert!(
            direct_probe.is_none(),
            "failing sidecar should fall through to later resolution stages"
        );

        let fallback_result = runtime.block_on(bootstrap::ensure_runtime_binary(&test_root, &spec));
        let error_message = fallback_result.expect_err("resolution should continue and fail with missing guidance");
        assert!(
            error_message.contains("is unavailable"),
            "expected fallback guidance error, got: {error_message}"
        );

        fs::remove_dir_all(test_root).expect("test data should be removed");
    }
}
