# yt-dlp Integration

## Distribution Strategy

Bundle `yt-dlp` as a sidecar as well, but keep it optional at the feature level (URL import).

| Platform        | Binary                                             | Size   |
| --------------- | -------------------------------------------------- | ------ |
| macOS universal | `yt-dlp_macos` → `yt-dlp-aarch64-apple-darwin`     | ~21 MB |
| Linux x86_64    | `yt-dlp_linux` → `yt-dlp-x86_64-unknown-linux-gnu` | ~21 MB |
| Windows x64     | `yt-dlp.exe` → `yt-dlp-x86_64-pc-windows-msvc.exe` | ~21 MB |

yt-dlp requires ffmpeg for audio extraction/conversion. The app points `--ffmpeg-location` to the bundled ffmpeg sidecar when present, otherwise uses system `PATH`.

## URL Import Flow

1. **User pastes a URL** (YouTube, podcast, any yt-dlp-supported site)
2. **Fetch metadata** (no download):

   ```sh
   yt-dlp --dump-json --no-playlist "<url>"
   ```

   Returns JSON to stdout with: `title`, `duration`, `thumbnail`, `uploader`, `description`, `webpage_url`, etc. Display a preview card in the UI.

3. **User confirms** → start download
4. **Download audio-only:**

   ```sh
   yt-dlp -x --audio-format wav --audio-quality 0 \
     --no-playlist --newline \
     --ffmpeg-location <bundled_ffmpeg_dir> \
     -o "<appdata>/audio/<uuid>.%(ext)s" "<url>"
   ```

5. **Download video (if user requests subtitled video export):**

   ```sh
   yt-dlp -f "bestvideo[ext=mp4]+bestaudio[ext=m4a]/best[ext=mp4]/best" \
     --no-playlist --newline \
     --ffmpeg-location <bundled_ffmpeg_dir> \
     -o "<appdata>/video/<uuid>.%(ext)s" "<url>"
   ```

6. **Parse progress** from stderr (each line with `--newline` flag):

   ```text
   [download]   5.2% of 45.30MiB at 2.50MiB/s ETA 00:17
   ```

   Parse percentage, speed, and ETA via regex for UI progress bar.

7. **Post-download:** feed the audio WAV into the transcription pipeline (§2). Store yt-dlp metadata (source URL, original title, uploader) on the document record.

## Metadata Storage

yt-dlp metadata is stored on the document record:

```sql
-- Added to documents table
source_url  TEXT,              -- original URL
source_meta TEXT,              -- yt-dlp JSON metadata (title, uploader, etc.)
```

## Supported Sites

yt-dlp supports 1000+ sites (YouTube, Vimeo, SoundCloud, Bandcamp, Twitter/X, Reddit, podcast RSS feeds, etc.). yt-dlp normalizes the interface so no special handling per-site needed.
