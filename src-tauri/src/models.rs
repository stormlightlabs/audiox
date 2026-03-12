//! Ollama and Whisper.cpp module

use serde::Serialize;
use std::fmt::{Display, Formatter};
use std::str::FromStr;
use std::time::Duration;

pub const REQUIRED_DIRECTORIES: [&str; 6] = ["models", "audio", "video", "subtitles", "db", "bin"];
pub const REQUIRED_OLLAMA_MODELS: [&str; 1] = ["gemma3"];
pub const SCHEMA_VERSION: i64 = 3;
pub const PREFLIGHT_EVENT: &str = "preflight://check";
pub const SETTING_KEY_WHISPER_MODEL: &str = "whisper_model";
pub const SETTING_KEY_WHISPER_LANGUAGE: &str = "whisper_language";
pub const SETTING_KEY_WHISPER_THREADS: &str = "whisper_threads";
pub const SETTING_KEY_OLLAMA_ENDPOINT: &str = "ollama_endpoint";
pub const WHISPER_LANGUAGE_AUTO: &str = "auto";
pub const OLLAMA_DEFAULT_ENDPOINT: &str = "http://localhost:11434";
pub const WHISPER_MIN_THREADS: usize = 1;
pub const WHISPER_MAX_THREADS: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProgressEvent {
    SetupWhisper,
    SetupEmbedding,
    SetupOllama,
    ImportConversion,
    ImportTranscription,
    ImportMetadata,
    DocumentMetadata,
}

impl ProgressEvent {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SetupWhisper => "setup://whisper-progress",
            Self::SetupEmbedding => "setup://embedding-progress",
            Self::SetupOllama => "setup://ollama-progress",
            Self::ImportConversion => "import://conversion-progress",
            Self::ImportTranscription => "import://transcription-progress",
            Self::ImportMetadata => "import://metadata-progress",
            Self::DocumentMetadata => "document://metadata-progress",
        }
    }
}

impl Display for ProgressEvent {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OllamaUrl {
    Tags,
    Pull,
    Generate,
}

impl OllamaUrl {
    pub const fn as_path(self) -> &'static str {
        match self {
            Self::Tags => "/api/tags",
            Self::Pull => "/api/pull",
            Self::Generate => "/api/generate",
        }
    }

    pub fn url(self, endpoint: &str) -> String {
        let normalized_endpoint = endpoint.trim_end_matches('/');
        format!("{normalized_endpoint}{}", self.as_path())
    }
}

impl Display for OllamaUrl {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_path())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OllamaModel {
    GenerateFamily,
    GenerateDefault,
}

impl OllamaModel {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::GenerateFamily => "gemma3",
            Self::GenerateDefault => "gemma3:4b",
        }
    }
}

impl Display for OllamaModel {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WhisperDefaults {
    pub model_name: &'static str,
    pub threads: usize,
}

pub const WHISPER_DEFAULTS: WhisperDefaults = WhisperDefaults { model_name: "base.en", threads: 4 };

/// ~512 token chunks (rough approximation: ~0.75 words/token for English prose).
pub const EMBEDDING_CHUNK_TARGET_WORDS: usize = 384;
pub const ALLOWED_IMPORT_EXTENSIONS: [&str; 7] = ["mp3", "m4a", "wav", "flac", "ogg", "opus", "webm"];
pub const ALLOWED_TEXT_IMPORT_EXTENSIONS: [&str; 2] = ["txt", "md"];

pub enum SearchLimit {
    Default,
    Max,
}

impl From<SearchLimit> for usize {
    fn from(value: SearchLimit) -> Self {
        match value {
            SearchLimit::Default => 8,
            SearchLimit::Max => 50,
        }
    }
}

pub enum Timeouts {
    Command,
    Download,
}

impl From<Timeouts> for u64 {
    fn from(value: Timeouts) -> Self {
        match value {
            Timeouts::Command => 8,
            Timeouts::Download => 120,
        }
    }
}

impl From<Timeouts> for Duration {
    fn from(val: Timeouts) -> Self {
        Duration::from_secs(val.into())
    }
}

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
    EmbeddingModel,
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
    pub embedding_model: CheckStatus,
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
    pub embedding_model_ready: bool,
    pub ollama_server_ready: bool,
    pub missing_ollama_models: Vec<String>,
    pub setup_completed: bool,
    pub all_required_ready: bool,
    pub guidance: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub whisper_model: String,
    pub whisper_language: String,
    pub whisper_threads: usize,
    pub ollama_endpoint: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WhisperModelInfo {
    pub model_name: String,
    pub file_name: String,
    pub size_bytes: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WhisperModelInventory {
    pub selected_model: String,
    pub installed_models: Vec<WhisperModelInfo>,
    pub total_size_bytes: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OllamaConnectionStatus {
    pub endpoint: String,
    pub reachable: bool,
    pub installed_models: Vec<String>,
    pub missing_models: Vec<String>,
    pub message: String,
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
    pub source_type: String,
    pub source_uri: String,
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
    pub source_type: String,
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
    pub source_type: String,
    pub source_uri: Option<String>,
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
            embedding_model: CheckStatus::Warn,
            ollama_server: CheckStatus::Warn,
            ollama_models: CheckStatus::Warn,
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
