use libloading::{Library, Symbol};
use serde::Deserialize;
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::path::PathBuf;
use tauri::Manager;

#[derive(Debug, Clone, Deserialize)]
pub struct ProbeResult {
    pub available: bool,
    pub reason: Option<String>,
    #[serde(rename = "supportsLocale")]
    pub supports_locale: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct MetadataResultPayload {
    title: Option<String>,
    summary: Option<String>,
    tags: Vec<String>,
    error: Option<String>,
}

pub struct Metadata {
    pub title: Option<String>,
    pub summary: Option<String>,
    pub tags: Vec<String>,
}

impl Metadata {
    fn new(title: Option<String>, summary: Option<String>, tags: Vec<String>) -> Self {
        Self { title, summary, tags }
    }
}

type ProbeFn = unsafe extern "C" fn() -> *mut c_char;
type GenerateFn = unsafe extern "C" fn(*const c_char, *const c_char) -> *mut c_char;
type FreeFn = unsafe extern "C" fn(*mut c_char);

const BRIDGE_LIBRARY_NAME: &str = "libmurmur_apple_intelligence.dylib";

fn bridge_candidate_paths(app: &tauri::AppHandle) -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    let compiled_bridge = env!("MURMUR_APPLE_INTELLIGENCE_BRIDGE_PATH");
    if !compiled_bridge.trim().is_empty() {
        candidates.push(PathBuf::from(compiled_bridge));
    }

    if let Ok(resource_dir) = app.path().resource_dir() {
        candidates.push(resource_dir.join("binaries").join(BRIDGE_LIBRARY_NAME));
    }

    candidates.push(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("binaries")
            .join(BRIDGE_LIBRARY_NAME),
    );

    if let Ok(current_exe) = std::env::current_exe() {
        if let Some(exe_dir) = current_exe.parent() {
            candidates.push(exe_dir.join(BRIDGE_LIBRARY_NAME));
            candidates.push(exe_dir.join("binaries").join(BRIDGE_LIBRARY_NAME));
        }
    }

    let mut unique = Vec::new();
    for path in candidates {
        if !unique.iter().any(|candidate: &PathBuf| candidate == &path) {
            unique.push(path);
        }
    }
    unique
}

fn resolve_bridge_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    bridge_candidate_paths(app)
        .into_iter()
        .find(|path| path.is_file())
        .ok_or_else(|| "Apple Intelligence bridge library was not found.".to_string())
}

fn decode_owned_c_string(pointer: *mut c_char, free_fn: FreeFn) -> Result<String, String> {
    if pointer.is_null() {
        return Err("bridge returned a null response pointer".to_string());
    }

    let value = unsafe { CStr::from_ptr(pointer) }
        .to_str()
        .map_err(|error| format!("bridge returned non UTF-8 payload: {error}"))?
        .to_string();
    unsafe { free_fn(pointer) };
    Ok(value)
}

fn with_bridge<T, F>(app: &tauri::AppHandle, action: F) -> Result<T, String>
where
    F: FnOnce(&Library, ProbeFn, GenerateFn, FreeFn) -> Result<T, String>,
{
    let bridge_path = resolve_bridge_path(app)?;
    let library = unsafe { Library::new(&bridge_path) }
        .map_err(|error| format!("failed to load {}: {error}", bridge_path.display()))?;

    let probe: Symbol<'_, ProbeFn> = unsafe { library.get(b"murmur_probe_apple_intelligence") }
        .map_err(|error| format!("bridge symbol murmur_probe_apple_intelligence is unavailable: {error}"))?;
    let generate: Symbol<'_, GenerateFn> = unsafe { library.get(b"murmur_generate_apple_metadata") }
        .map_err(|error| format!("bridge symbol murmur_generate_apple_metadata is unavailable: {error}"))?;
    let free: Symbol<'_, FreeFn> = unsafe { library.get(b"murmur_free_bridge_string") }
        .map_err(|error| format!("bridge symbol murmur_free_bridge_string is unavailable: {error}"))?;

    action(&library, *probe, *generate, *free)
}

pub fn probe(app: &tauri::AppHandle) -> Result<ProbeResult, String> {
    with_bridge(app, |_, probe_fn, _, free_fn| {
        let payload = decode_owned_c_string(unsafe { probe_fn() }, free_fn)?;
        serde_json::from_str::<ProbeResult>(&payload)
            .map_err(|error| format!("failed to parse Apple Intelligence probe payload: {error}"))
    })
}

pub fn generate_metadata(app: &tauri::AppHandle, transcript: &str, fallback_title: &str) -> Result<Metadata, String> {
    with_bridge(app, |_, _, generate_fn, free_fn| {
        let transcript =
            CString::new(transcript).map_err(|error| format!("transcript contains an interior null byte: {error}"))?;
        let fallback_title = CString::new(fallback_title)
            .map_err(|error| format!("fallback title contains an interior null byte: {error}"))?;
        let payload = decode_owned_c_string(
            unsafe { generate_fn(transcript.as_ptr(), fallback_title.as_ptr()) },
            free_fn,
        )?;
        let parsed = serde_json::from_str::<MetadataResultPayload>(&payload)
            .map_err(|error| format!("failed to parse Apple Intelligence metadata payload: {error}"))?;

        match parsed.error {
            Some(error) => Err(error),
            None => Ok(Metadata::new(parsed.title, parsed.summary, parsed.tags)),
        }
    })
}
