//! Parsing Utilities

use regex::Regex;
use serde_json::Value;
use std::collections::HashSet;
use std::path::Path;

use crate::models::{TranscriptSegment, ALLOWED_IMPORT_EXTENSIONS, REQUIRED_OLLAMA_MODELS};

pub fn parse_ollama_model_names(payload: &Value) -> Vec<String> {
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

pub fn model_name_matches(candidate: &str, required: &str) -> bool {
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

pub fn missing_required_ollama_models(models: &[String]) -> Vec<String> {
    REQUIRED_OLLAMA_MODELS
        .iter()
        .filter(|required| !models.iter().any(|candidate| model_name_matches(candidate, required)))
        .map(|required| required.to_string())
        .collect()
}

pub fn validate_whisper_model_name(model_name: &str) -> Result<String, String> {
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

pub fn whisper_model_file_name(model_name: &str) -> String {
    format!("ggml-{model_name}.bin")
}

pub fn whisper_model_download_url(model_name: &str) -> String {
    format!(
        "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/{}",
        whisper_model_file_name(model_name)
    )
}

pub fn calculate_percent(completed: u64, total: u64) -> f64 {
    if total == 0 {
        return 0.0;
    }

    let percent = (completed as f64 / total as f64) * 100.0;
    percent.clamp(0.0, 100.0)
}

pub fn parse_ollama_progress_line(line: &str) -> Result<(String, u64, u64, bool), String> {
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

pub fn extension_for_path(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.trim().to_ascii_lowercase())
}

pub fn ensure_supported_import_path(source_path: &Path) -> Result<(), String> {
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

pub fn parse_hms_to_ms(hours: i64, minutes: i64, seconds: f64) -> i64 {
    let whole_seconds = seconds.trunc() as i64;
    let milliseconds = ((seconds - seconds.trunc()) * 1000.0).round() as i64;
    (hours * 3_600_000) + (minutes * 60_000) + (whole_seconds * 1000) + milliseconds
}

pub fn parse_clock_timestamp_to_ms(value: &str) -> Option<i64> {
    let mut parts = value.split(':');
    let hours = parts.next()?.trim().parse::<i64>().ok()?;
    let minutes = parts.next()?.trim().parse::<i64>().ok()?;
    let seconds = parts.next()?.trim().replace(',', ".").parse::<f64>().ok()?;
    if parts.next().is_some() {
        return None;
    }

    Some(parse_hms_to_ms(hours, minutes, seconds))
}

pub fn parse_ffmpeg_duration_ms(payload: &str) -> Option<i64> {
    let duration_regex = Regex::new(r"Duration:\s+(\d{2}):(\d{2}):(\d{2}(?:\.\d+)?)").ok()?;
    let captures = duration_regex.captures(payload)?;
    let hours = captures.get(1)?.as_str().parse::<i64>().ok()?;
    let minutes = captures.get(2)?.as_str().parse::<i64>().ok()?;
    let seconds = captures.get(3)?.as_str().parse::<f64>().ok()?;
    Some(parse_hms_to_ms(hours, minutes, seconds))
}

pub fn parse_ffmpeg_out_time_ms(value: &str) -> Option<i64> {
    if let Ok(raw) = value.trim().parse::<i64>() {
        if raw > 10_000_000 {
            return Some(raw / 1000);
        }
        return Some(raw);
    }

    parse_clock_timestamp_to_ms(value.trim())
}

pub fn parse_progress_percent(line: &str) -> Option<f64> {
    let captures = Regex::new(r"(\d{1,3}(?:\.\d+)?)%").ok()?.captures(line)?;
    let raw = captures.get(1)?.as_str().parse::<f64>().ok()?;
    Some(raw.clamp(0.0, 100.0))
}

pub fn value_to_i64(value: &Value) -> Option<i64> {
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

    let from_offset = item
        .get("offsets")
        .and_then(|offsets| offsets.get("from"))
        .and_then(value_to_i64);
    let to_offset = item
        .get("offsets")
        .and_then(|offsets| offsets.get("to"))
        .and_then(value_to_i64);
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

pub fn parse_whisper_segments(payload: &Value) -> Vec<TranscriptSegment> {
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

pub fn build_transcript_text(segments: &[TranscriptSegment]) -> String {
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

pub fn max_duration_seconds(segments: &[TranscriptSegment]) -> i64 {
    let max_end_ms = segments.iter().map(|segment| segment.end_ms).max().unwrap_or(0);
    ((max_end_ms as f64) / 1000.0).ceil() as i64
}

fn normalized_tag(raw: &str) -> Option<String> {
    let trimmed = raw
        .trim()
        .trim_matches(|character: char| matches!(character, '#' | '"' | '\'' | ',' | '.' | ';' | ':'));

    if trimmed.is_empty() {
        return None;
    }

    let collapsed = trimmed.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        return None;
    }
    Some(collapsed)
}

pub fn sanitize_tags(tags: &[String]) -> Vec<String> {
    let mut deduped = Vec::new();
    let mut seen = HashSet::new();

    for candidate in tags {
        let Some(tag) = normalized_tag(candidate) else {
            continue;
        };
        let key = tag.to_ascii_lowercase();
        if seen.insert(key) {
            deduped.push(tag);
        }
    }

    deduped
}

pub fn parse_keywords_csv(value: Option<&str>) -> Vec<String> {
    let Some(raw) = value else {
        return Vec::new();
    };

    let candidates = raw.split(',').map(str::to_string).collect::<Vec<_>>();
    sanitize_tags(&candidates)
}

pub fn serialize_keywords_csv(tags: &[String]) -> Option<String> {
    let cleaned = sanitize_tags(tags);
    if cleaned.is_empty() {
        return None;
    }
    Some(cleaned.join(", "))
}

fn chunk_text_by_words(text: &str, target_words: usize) -> Vec<String> {
    if target_words == 0 {
        return Vec::new();
    }

    let words = text.split_whitespace().collect::<Vec<_>>();
    if words.is_empty() {
        return Vec::new();
    }

    words
        .chunks(target_words)
        .map(|chunk| chunk.join(" "))
        .filter(|chunk| !chunk.trim().is_empty())
        .collect()
}

pub fn build_embedding_chunks(segments: &[TranscriptSegment], transcript: &str, target_words: usize) -> Vec<String> {
    if target_words == 0 {
        return Vec::new();
    }

    let mut chunks = Vec::new();
    let mut current_chunk = Vec::new();
    let mut current_words = 0_usize;

    for segment in segments {
        let text = segment.text.trim();
        if text.is_empty() {
            continue;
        }

        let segment_word_count = text.split_whitespace().count().max(1);
        if !current_chunk.is_empty() && current_words + segment_word_count > target_words {
            chunks.push(current_chunk.join(" "));
            current_chunk.clear();
            current_words = 0;
        }

        current_chunk.push(text.to_string());
        current_words += segment_word_count;
    }

    if !current_chunk.is_empty() {
        chunks.push(current_chunk.join(" "));
    }

    if chunks.is_empty() {
        return chunk_text_by_words(transcript, target_words);
    }

    chunks
        .into_iter()
        .map(|chunk| chunk.trim().to_string())
        .filter(|chunk| !chunk.is_empty())
        .collect()
}

pub fn normalize_sha256(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

pub fn is_valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.chars().all(|character| character.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_missing_ollama_models() {
        let models = vec!["nomic-embed-text:latest".to_string()];
        let missing = missing_required_ollama_models(&models);
        assert_eq!(missing, vec!["gemma3:4b".to_string()]);
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
    fn keyword_csv_round_trip_is_normalized() {
        let parsed = parse_keywords_csv(Some("  AI, #podcast, ai , \"notes\" "));
        assert_eq!(parsed, vec!["AI", "podcast", "notes"]);
        assert_eq!(serialize_keywords_csv(&parsed), Some("AI, podcast, notes".to_string()));
    }

    #[test]
    fn embedding_chunks_follow_target_word_budget() {
        let segments = vec![
            TranscriptSegment { start_ms: 0, end_ms: 1000, text: "one two three".to_string() },
            TranscriptSegment { start_ms: 1001, end_ms: 2000, text: "four five".to_string() },
            TranscriptSegment { start_ms: 2001, end_ms: 3000, text: "six seven eight".to_string() },
        ];

        let chunks = build_embedding_chunks(&segments, "", 4);
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0], "one two three");
        assert_eq!(chunks[1], "four five");
        assert_eq!(chunks[2], "six seven eight");
    }
}
