# whisper.cpp Integration

## Distribution Strategy

Bundle `whisper-cli` as a **Tauri sidecar** (`bundle.externalBin`) for production builds.

- Preflight resolves in order: sidecar → managed runtime cache → `PATH`
- Runtime download is not required for `whisper-cli` in end-user flows
- Sidecar strategy keeps onboarding simple (no first-run binary install prompts)

## Model Management

- **Default model:** `ggml-base.en.bin` (142 MB) — good accuracy/speed tradeoff
- **Download source:** `https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-{name}.bin`
- **First-run flow:** Check if model file exists at `appdata/models/`. If missing, download with progress reporting via Tauri events.
- **Optional models:** Allow user to download larger models (small: 466 MB, medium: 1.5 GB) from settings. Store selection in config.

## Transcription Pipeline

**Input requirements:** whisper.cpp requires 16-bit PCM WAV, 16kHz, mono. The bundled ffmpeg sidecar handles format conversion (see §4).

**Rust command flow:**

1. Convert input audio to required format via ffmpeg sidecar (see §4)
2. Spawn whisper-cli sidecar:

   ```sh
   whisper-cli -m <model_path> -f <audio_path> -oj -l auto -t 4 -pp
   ```

3. Stream `stderr` for progress (`-pp` flag prints progress percentage)
4. Parse JSON output from `stdout` on completion
5. Return structured transcript with timestamps

**Output format (whisper JSON):**

```json
{
  "transcription": [
    {
      "timestamps": { "from": "00:00:00,000", "to": "00:00:05,230" },
      "offsets": { "from": 0, "to": 5230 },
      "text": " Hello, this is a recording."
    }
  ]
}
```

## Subtitle Generation

whisper-cli natively generates subtitle files alongside JSON:

```sh
# Generate SRT + VTT alongside the transcript
whisper-cli -m <model_path> -f <audio_path> -oj -osrt -ovtt -of <output_base_path>
```

- `-osrt` → `<output_base_path>.srt`
- `-ovtt` → `<output_base_path>.vtt`

Generated subtitle files are saved to `appdata/subtitles/<document_uuid>.*`.

## Audio Recording

Use **`tauri-plugin-audio-recorder`** for cross-platform microphone recording via native Rust (cpal-based), instead of WebView `getUserMedia`/`MediaRecorder`.

**Why not WebView getUserMedia:**

- **Linux:** WebKitGTK ships without WebRTC/MediaStream support (`-DENABLE_WEB_RTC=ON -DENABLE_MEDIA_STREAM=ON` are off by default). Recording via `getUserMedia` is non-functional on most Linux distributions without a custom-compiled WebKitGTK.
- **macOS:** WKWebView only supports `audio/mp4` (AAC) via MediaRecorder — no `audio/webm` support. Users may see duplicate permission prompts (OS-level + webview-level) on macOS 14+.
- **Windows:** WebView2 (Chromium) supports `audio/webm;codecs=opus` but persists permission state in a way that makes re-prompting impossible if the user clicks "Block" without deleting the WebView2 Preferences file.

A native Rust plugin bypasses all webview engine differences and works reliably across all platforms.

**Plugin:** [`tauri-plugin-audio-recorder`](https://github.com/brenogonzaga/tauri-plugin-audio-recorder)

- **Platforms:** macOS, Windows, Linux (also iOS, Android)
- **Output:** WAV on desktop, M4A/AAC on mobile
- **Quality presets:** Low (16kHz mono), Medium (44.1kHz mono), High (48kHz stereo)
- **API:** `startRecording()`, `stopRecording()`, `pauseRecording()`, `resumeRecording()`, `getStatus()`, `getDevices()`, `checkPermission()`, `requestPermission()`

**Setup:**

```bash
cargo add tauri-plugin-audio-recorder
npm install tauri-plugin-audio-recorder-api
```

```rust
// src-tauri/src/lib.rs
tauri::Builder::default()
    .plugin(tauri_plugin_audio_recorder::init())
```

Add to `src-tauri/capabilities/default.json`:

```json
"permissions": [
  "audio-recorder:default"
]
```

**macOS entitlement:** Add `src-tauri/Info.plist`:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>NSMicrophoneUsageDescription</key>
    <string>This app needs microphone access to record audio for transcription.</string>
</dict>
</plist>
```

**Flow:**

1. Frontend calls `checkPermission()` → if denied, call `requestPermission()`
2. `startRecording({ quality: 'low' })` — records 16kHz mono WAV (matches whisper-cli input directly)
3. UI shows live waveform, elapsed time, pause/resume controls
4. `stopRecording()` → returns path to saved WAV file in `appdata/audio/`
5. Feed WAV into transcription pipeline (ffmpeg conversion may be skipped if already 16kHz mono)
