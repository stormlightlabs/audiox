# Sidecar Binaries

Tauri `externalBin` entries in `src-tauri/tauri.conf.json`:

- `binaries/whisper-cli`
- `binaries/ffmpeg`
- `binaries/yt-dlp`

For each entry, provide the platform-target-suffixed file expected by Tauri, for example:

- `whisper-cli-aarch64-apple-darwin`
- `ffmpeg-aarch64-apple-darwin`
- `yt-dlp-aarch64-apple-darwin`
- `whisper-cli-x86_64-unknown-linux-gnu`
- `ffmpeg-x86_64-unknown-linux-gnu`
- `yt-dlp-x86_64-unknown-linux-gnu`
- `whisper-cli-x86_64-pc-windows-msvc.exe`
- `ffmpeg-x86_64-pc-windows-msvc.exe`
- `yt-dlp-x86_64-pc-windows-msvc.exe`

## Development Helpers

- Run `bash setup.sh` (or `pnpm setup:sidecars`) to generate wrapper sidecars for your local host target.
- Debug/test builds also auto-generate missing wrapper sidecars from `src-tauri/build.rs`.

Wrappers are for development only. Replace wrappers with real release binaries before shipping production builds.
