#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BIN_DIR="$ROOT_DIR/src-tauri/binaries"

force=0
target_triple=""

for arg in "$@"; do
  case "$arg" in
    --force)
      force=1
      ;;
    --target=*)
      target_triple="${arg#--target=}"
      ;;
    *)
      echo "Unknown argument: $arg" >&2
      echo "Usage: $0 [--force] [--target=<triple>]" >&2
      exit 1
      ;;
  esac
done

if [[ -z "$target_triple" ]]; then
  target_triple="$(rustc -vV | awk '/^host:/ { print $2 }')"
fi

mkdir -p "$BIN_DIR"

write_wrapper() {
  local tool="$1"
  local filename="${tool}-${target_triple}"

  if [[ "$target_triple" == *"windows"* ]]; then
    filename="${filename}.exe"
  fi

  local file="$BIN_DIR/$filename"

  if [[ -f "$file" && "$force" -ne 1 ]]; then
    echo "skip: $file already exists (use --force to overwrite)"
    return
  fi

  if [[ "$target_triple" == *"windows"* ]]; then
    printf '@echo off\r\n%s %%*\r\n' "$tool" > "$file"
  else
    cat > "$file" <<SCRIPT
#!/bin/sh
exec ${tool} "\$@"
SCRIPT
    chmod +x "$file"
  fi

  echo "wrote: $file"
}

write_wrapper "whisper-cli"
write_wrapper "ffmpeg"
write_wrapper "yt-dlp"

echo "Sidecar wrappers are ready in $BIN_DIR"
echo "Replace wrappers with real release binaries before shipping production builds."
