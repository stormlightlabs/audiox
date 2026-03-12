use std::env;
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

const SIDECAR_TOOLS: [&str; 3] = ["whisper-cli", "ffmpeg", "yt-dlp"];

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
    println!("cargo:rerun-if-env-changed=WRITER_APP_VERSION");

    let version = std::env::var("WRITER_APP_VERSION")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(git_describe_version)
        .unwrap_or_else(|| format!("v{}", env!("CARGO_PKG_VERSION")));

    println!("cargo:rustc-env=WRITER_APP_VERSION={version}");

    if let Err(error) = ensure_debug_sidecars() {
        panic!("{error}");
    }
    tauri_build::build()
}
