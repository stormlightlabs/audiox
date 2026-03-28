use std::env;
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::time::SystemTime;

const SIDECAR_TOOLS: [&str; 3] = ["whisper-cli", "ffmpeg", "yt-dlp"];
const APPLE_INTELLIGENCE_BRIDGE_NAME: &str = "libmurmur_apple_intelligence.dylib";

fn target_file_name(tool: &str, target: &str) -> String {
    if target.contains("windows") {
        format!("{tool}-{target}.exe")
    } else {
        format!("{tool}-{target}")
    }
}

fn write_dev_sidecar(path: &Path, tool: &str, target: &str) -> Result<(), String> {
    let contents = if target.contains("windows") {
        format!("@echo off\r\n{tool} %*\r\n")
    } else {
        format!("#!/bin/sh\nexec {tool} \"$@\"\n")
    };

    fs::write(path, contents).map_err(|error| format!("failed to write dev sidecar {}: {error}", path.display()))?;

    #[cfg(unix)]
    {
        let mut permissions = fs::metadata(path)
            .map_err(|error| format!("failed to read permissions for {}: {error}", path.display()))?
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions)
            .map_err(|error| format!("failed to set executable permissions for {}: {error}", path.display()))?;
    }

    Ok(())
}

fn ensure_debug_sidecars() -> Result<(), String> {
    let profile = env::var("PROFILE").unwrap_or_default();
    if profile != "debug" {
        return Ok(());
    }

    let target = env::var("TARGET").map_err(|error| format!("missing TARGET env var: {error}"))?;
    let manifest_dir =
        env::var("CARGO_MANIFEST_DIR").map_err(|error| format!("missing CARGO_MANIFEST_DIR env var: {error}"))?;
    let binaries_dir = Path::new(&manifest_dir).join("binaries");
    fs::create_dir_all(&binaries_dir)
        .map_err(|error| format!("failed to create sidecar directory {}: {error}", binaries_dir.display()))?;

    for tool in SIDECAR_TOOLS {
        let file_name = target_file_name(tool, &target);
        let file_path = binaries_dir.join(file_name);
        if file_path.exists() {
            continue;
        }
        write_dev_sidecar(&file_path, tool, &target)?;
        println!("cargo:warning=generated debug sidecar wrapper {}", file_path.display());
    }

    Ok(())
}

fn apple_intelligence_bridge_path(manifest_dir: &str, profile: &str) -> Result<std::path::PathBuf, String> {
    if profile == "debug" {
        let out_dir = env::var("OUT_DIR").map_err(|error| format!("missing OUT_DIR env var: {error}"))?;
        return Ok(Path::new(&out_dir).join(APPLE_INTELLIGENCE_BRIDGE_NAME));
    }

    Ok(Path::new(manifest_dir).join("binaries").join(APPLE_INTELLIGENCE_BRIDGE_NAME))
}

fn modified_at(path: &Path) -> Result<SystemTime, String> {
    fs::metadata(path)
        .map_err(|error| format!("failed to read metadata for {}: {error}", path.display()))?
        .modified()
        .map_err(|error| format!("failed to read modification time for {}: {error}", path.display()))
}

fn should_rebuild(output_path: &Path, inputs: &[&Path]) -> Result<bool, String> {
    if !output_path.is_file() {
        return Ok(true);
    }

    let output_modified = modified_at(output_path)?;
    for input in inputs {
        if modified_at(input)? > output_modified {
            return Ok(true);
        }
    }

    Ok(false)
}

fn ensure_apple_intelligence_bridge() -> Result<(), String> {
    let target = env::var("TARGET").map_err(|error| format!("missing TARGET env var: {error}"))?;
    if !target.contains("apple-darwin") {
        return Ok(());
    }

    let manifest_dir =
        env::var("CARGO_MANIFEST_DIR").map_err(|error| format!("missing CARGO_MANIFEST_DIR env var: {error}"))?;
    let profile = env::var("PROFILE").unwrap_or_default();
    let source_path = Path::new(&manifest_dir)
        .join("native")
        .join("apple_intelligence_bridge.swift");
    let build_script_path = Path::new(&manifest_dir).join("build.rs");
    let output_path = apple_intelligence_bridge_path(&manifest_dir, &profile)?;
    let module_cache_path = Path::new(
        &env::var("OUT_DIR").map_err(|error| format!("missing OUT_DIR env var: {error}"))?,
    )
    .join("swift-module-cache");

    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create bridge output directory {}: {error}",
                parent.display()
            )
        })?;
    }
    fs::create_dir_all(&module_cache_path).map_err(|error| {
        format!(
            "failed to create Swift module cache directory {}: {error}",
            module_cache_path.display()
        )
    })?;

    if !should_rebuild(&output_path, &[source_path.as_path(), build_script_path.as_path()])? {
        println!(
            "cargo:rustc-env=MURMUR_APPLE_INTELLIGENCE_BRIDGE_PATH={}",
            output_path.display()
        );
        return Ok(());
    }

    let sdk_output = std::process::Command::new("xcrun")
        .args(["--show-sdk-path", "--sdk", "macosx"])
        .output()
        .map_err(|error| format!("failed to resolve macOS SDK path with xcrun: {error}"))?;
    if !sdk_output.status.success() {
        return Err(format!(
            "xcrun failed to resolve macOS SDK path: {}",
            String::from_utf8_lossy(&sdk_output.stderr).trim()
        ));
    }
    let sdk_path = String::from_utf8(sdk_output.stdout)
        .map_err(|error| format!("macOS SDK path from xcrun was not valid UTF-8: {error}"))?
        .trim()
        .to_string();

    let mut command = std::process::Command::new("swiftc");
    command
        .arg("-sdk")
        .arg(&sdk_path)
        .arg("-parse-as-library")
        .arg("-emit-library")
        .arg("-module-cache-path")
        .arg(&module_cache_path)
        .arg("-o")
        .arg(&output_path)
        .arg(&source_path);

    let output = command
        .output()
        .map_err(|error| format!("failed to compile Apple Intelligence bridge with swiftc: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "swiftc failed to compile {}: {}",
            source_path.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    println!(
        "cargo:rustc-env=MURMUR_APPLE_INTELLIGENCE_BRIDGE_PATH={}",
        output_path.display()
    );
    Ok(())
}

fn git_describe_version() -> Option<String> {
    let output = std::process::Command::new("git")
        .args(["describe", "--tags", "--long", "--always", "--dirty"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let value = String::from_utf8(output.stdout).ok()?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }

    Some(trimmed.to_string())
}

fn main() {
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-env-changed=MURMUR_APP_VERSION");
    println!("cargo:rerun-if-changed=native/apple_intelligence_bridge.swift");

    let version = std::env::var("MURMUR_APP_VERSION")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(git_describe_version)
        .unwrap_or_else(|| format!("v{}", env!("CARGO_PKG_VERSION")));

    println!("cargo:rustc-env=MURMUR_APP_VERSION={version}");

    if let Err(error) = ensure_debug_sidecars() {
        panic!("{error}");
    }
    if let Err(error) = ensure_apple_intelligence_bridge() {
        panic!("{error}");
    }
    tauri_build::build()
}
