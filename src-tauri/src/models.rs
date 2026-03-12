//! Ollama and Whisper.cpp module

use std::str::FromStr;

use serde::Serialize;

pub const REQUIRED_DIRECTORIES: [&str; 6] = ["models", "audio", "video", "subtitles", "db", "bin"];
pub const REQUIRED_OLLAMA_MODELS: [&str; 2] = ["nomic-embed-text", "gemma3:4b"];
pub const SCHEMA_VERSION: i64 = 3;
pub const PREFLIGHT_EVENT: &str = "preflight://check";
pub const SETUP_WHISPER_PROGRESS_EVENT: &str = "setup://whisper-progress";
pub const SETUP_OLLAMA_PROGRESS_EVENT: &str = "setup://ollama-progress";
pub const IMPORT_CONVERSION_PROGRESS_EVENT: &str = "import://conversion-progress";
pub const IMPORT_TRANSCRIPTION_PROGRESS_EVENT: &str = "import://transcription-progress";
pub const OLLAMA_TAGS_URL: &str = "http://localhost:11434/api/tags";
pub const OLLAMA_PULL_URL: &str = "http://localhost:11434/api/pull";
pub const OLLAMA_GENERATE_URL: &str = "http://localhost:11434/api/generate";
pub const OLLAMA_EMBED_URL: &str = "http://localhost:11434/api/embed";
pub const OLLAMA_GENERATE_MODEL: &str = "gemma3:4b";
pub const OLLAMA_EMBED_MODEL: &str = "nomic-embed-text";
pub const DEFAULT_WHISPER_MODEL_NAME: &str = "base.en";
pub const DEFAULT_WHISPER_THREADS: usize = 4;

/// ~512 token chunks (rough approximation: ~0.75 words/token for English prose).
pub const EMBEDDING_CHUNK_TARGET_WORDS: usize = 384;
pub const DEFAULT_SEARCH_LIMIT: usize = 8;
pub const MAX_SEARCH_LIMIT: usize = 50;
pub const COMMAND_TIMEOUT_SECONDS: u64 = 8;
pub const DOWNLOAD_TIMEOUT_SECONDS: u64 = 120;
pub const ALLOWED_IMPORT_EXTENSIONS: [&str; 7] = ["mp3", "m4a", "wav", "flac", "ogg", "opus", "webm"];

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppBootstrapResult {
    pub app_data_dir: String,
    pub database_path: String,
    pub created_directories: Vec<String>,
    pub schema_version: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CheckStatus {
    Pass,
    Fail,
    Warn,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PreflightCheck {
    WhisperCli,
    Ffmpeg,
    YtDlp,
    WhisperModel,
    OllamaServer,
    OllamaModels,
    Database,
}

#[derive(Clone, Debug, Serialize)]
pub struct PreflightCheckDetail {
    pub check: PreflightCheck,
    pub status: CheckStatus,
    pub message: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct PreflightResult {
    pub whisper_cli: CheckStatus,
    pub ffmpeg: CheckStatus,
    pub yt_dlp: CheckStatus,
    pub whisper_model: CheckStatus,
    pub ollama_server: CheckStatus,
    pub ollama_models: CheckStatus,
    pub database: CheckStatus,
    pub should_open_setup: bool,
    pub all_required_passed: bool,
    pub details: Vec<PreflightCheckDetail>,
}

#[derive(Clone, Debug, Serialize)]
pub struct SetupStatus {
    pub whisper_model_ready: bool,
    pub ollama_server_ready: bool,
    pub missing_ollama_models: Vec<String>,
    pub setup_completed: bool,
    pub all_required_ready: bool,
    pub guidance: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WhisperDownloadProgress {
    pub model_name: String,
    pub status: String,
    pub message: String,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub percent: f64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OllamaPullProgress {
    pub model_name: String,
    pub status: String,
    pub message: String,
    pub completed: u64,
    pub total: u64,
    pub percent: f64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversionProgress {
    pub status: String,
    pub message: String,
    pub out_time_ms: i64,
    pub total_duration_ms: Option<i64>,
    pub percent: f64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptionProgress {
    pub status: String,
    pub message: String,
    pub percent: f64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptSegment {
    pub start_ms: i64,
    pub end_ms: i64,
    pub text: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportedDocument {
    pub id: String,
    pub title: String,
    pub summary: Option<String>,
    pub tags: Vec<String>,
    pub transcript: String,
    pub audio_path: String,
    pub subtitle_srt_path: String,
    pub subtitle_vtt_path: String,
    pub duration_seconds: i64,
    pub created_at: String,
    pub segments: Vec<TranscriptSegment>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentSummary {
    pub id: String,
    pub title: String,
    pub summary: Option<String>,
    pub tags: Vec<String>,
    pub duration_seconds: Option<i64>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentDetail {
    pub id: String,
    pub title: String,
    pub summary: Option<String>,
    pub tags: Vec<String>,
    pub transcript: String,
    pub audio_path: Option<String>,
    pub subtitle_srt_path: Option<String>,
    pub subtitle_vtt_path: Option<String>,
    pub duration_seconds: Option<i64>,
    pub created_at: String,
    pub updated_at: String,
    pub segments: Vec<TranscriptSegment>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocumentSort {
    CreatedDesc,
    CreatedAsc,
    TitleAsc,
    TitleDesc,
    DurationAsc,
    DurationDesc,
}

impl FromStr for DocumentSort {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let value = match s.trim() {
            "created_asc" => Self::CreatedAsc,
            "title_asc" => Self::TitleAsc,
            "title_desc" => Self::TitleDesc,
            "duration_asc" => Self::DurationAsc,
            "duration_desc" => Self::DurationDesc,
            _ => Self::CreatedDesc,
        };

        Ok(value)
    }
}

impl DocumentSort {
    pub fn parse(sort: Option<&str>) -> Self {
        let value = sort.unwrap_or("");
        Self::from_str(value).unwrap_or(Self::CreatedDesc)
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    pub document_id: String,
    pub document_title: String,
    pub document_summary: Option<String>,
    pub document_tags: Vec<String>,
    pub chunk_index: i64,
    pub chunk_content: String,
    pub similarity: f64,
    pub segment_start_ms: Option<i64>,
    pub segment_end_ms: Option<i64>,
}

#[derive(Clone, Debug)]
pub struct EmbeddedChunk {
    pub chunk_index: i64,
    pub content: String,
    pub embedding: Vec<f32>,
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
pub struct RuntimeBinarySpec {
    pub check: PreflightCheck,
    pub tool_id: &'static str,
    pub display_name: &'static str,
    pub version: &'static str,
    pub executable_stem: &'static str,
    pub version_args: &'static [&'static str],
    pub path_candidates: &'static [&'static str],
    pub sidecar_candidates: &'static [&'static str],
    pub download_url_env: &'static str,
    pub download_sha256_env: &'static str,
    pub allow_runtime_download: bool,
}

pub const WHISPER_BINARY_SPEC: RuntimeBinarySpec = RuntimeBinarySpec {
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

pub const FFMPEG_BINARY_SPEC: RuntimeBinarySpec = RuntimeBinarySpec {
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

pub const YT_DLP_BINARY_SPEC: RuntimeBinarySpec = RuntimeBinarySpec {
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
