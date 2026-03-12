//! Local embedding model state and helpers.

use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub enum Prefix {
    Document,
    Query,
}

impl std::fmt::Display for Prefix {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Prefix::Document => write!(f, "search_document:"),
            Prefix::Query => write!(f, "search_query:"),
        }
    }
}

pub struct EmbeddingState {
    cache_dir: PathBuf,
    model: Mutex<Option<TextEmbedding>>,
}

impl EmbeddingState {
    pub fn from_app_data_dir(app_data_dir: impl Into<PathBuf>) -> Self {
        Self { cache_dir: app_data_dir.into().join("models").join("embed"), model: Mutex::new(None) }
    }

    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }

    pub fn ensure_ready(&self) -> Result<(), String> {
        self.with_model(|_| Ok(()))
    }

    pub fn embed_chunks(&self, chunks: &[String]) -> Result<Vec<Vec<f32>>, String> {
        if chunks.is_empty() {
            return Ok(Vec::new());
        }

        let prefixed_chunks = chunks
            .iter()
            .map(|chunk| format!("{} {chunk}", Prefix::Document))
            .collect::<Vec<_>>();
        self.with_model(move |model| {
            model
                .embed(prefixed_chunks, None)
                .map_err(|error| format!("failed to generate chunk embeddings with fastembed: {error}"))
        })
    }

    pub fn embed_query(&self, query: &str) -> Result<Vec<f32>, String> {
        let trimmed_query = query.trim();
        if trimmed_query.is_empty() {
            return Err("query must not be empty for embedding".to_string());
        }

        let prefixed_query = [format!("{} {trimmed_query}", Prefix::Query)];
        self.with_model(move |model| {
            model
                .embed(prefixed_query, None)
                .map_err(|error| format!("failed to generate query embedding with fastembed: {error}"))?
                .into_iter()
                .next()
                .ok_or_else(|| "fastembed returned no query embedding".to_string())
        })
    }

    fn with_model<T, F>(&self, action: F) -> Result<T, String>
    where
        F: FnOnce(&mut TextEmbedding) -> Result<T, String>,
    {
        let mut model_guard = self
            .model
            .lock()
            .map_err(|_| "embedding model state lock is poisoned".to_string())?;

        if model_guard.is_none() {
            *model_guard = Some(initialize_model(&self.cache_dir)?);
        }

        let model = model_guard
            .as_mut()
            .ok_or_else(|| "embedding model is unexpectedly unavailable".to_string())?;
        action(model)
    }
}

fn initialize_model(cache_dir: &Path) -> Result<TextEmbedding, String> {
    std::fs::create_dir_all(cache_dir).map_err(|error| {
        format!(
            "failed to create embedding cache directory {}: {error}",
            cache_dir.display()
        )
    })?;

    let options = InitOptions::new(EmbeddingModel::NomicEmbedTextV15)
        .with_cache_dir(cache_dir.to_path_buf())
        .with_show_download_progress(true);

    TextEmbedding::try_new(options)
        .map_err(|error| format!("failed to initialize fastembed NomicEmbedTextV15 model: {error}"))
}
