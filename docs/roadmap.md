# Audio X Roadmap

Each milestone produces a usable app. Later milestones build on earlier ones.

## Overview

- Part 1 (MVP): M1 - M4
- Part 2: M5 - M7
- Part 3: M13
- Part 4: M8
- Part 5: M9
- Part 6: M10
- Part 7: M11
- Part 8: M12

## M8: URL Import via yt-dlp

**Goal:** Paste a URL to download and transcribe audio/video from the web.

- URL input field in Import view (accepts YouTube, Vimeo, SoundCloud, podcast URLs, etc.)
- Fetch metadata without downloading (`yt-dlp --dump-json --no-playlist`): title, duration, thumbnail, uploader
- Display preview card with metadata before confirming download
- Audio-only download mode:

  ```sh
  yt-dlp -x --audio-format wav --audio-quality 0 --no-playlist --newline --ffmpeg-location <bundled> -o <appdata>/audio/<uuid>.%(ext)s <url>
  ```

- Video download mode (for subtitle export):

  ```sh
  yt-dlp -f "bestvideo[ext=mp4]+bestaudio[ext=m4a]/best" --no-playlist --newline --ffmpeg-location <bundled> -o <appdata>/video/<uuid>.%(ext)s <url>
  ```

- Parse progress from stderr (percentage, speed, ETA) for UI progress bar
- Post-download: extract audio via ffmpeg if video → feed into transcription pipeline (M4 + M5)
- Store source URL and yt-dlp metadata on document record

**Usable state:** User pastes a URL → sees video/audio preview → downloads → gets a fully processed document in their library.

## M10: Subtitle Export & Video Burn-in

**Goal:** Export subtitles as files or burn them into downloaded videos.

- Download SRT/VTT files from the Document view
- For documents with a video file (yt-dlp downloads), offer "Export with subtitles"
- Burn subtitles into video via ffmpeg:

  ```sh
  ffmpeg -i <video.mp4> -vf "subtitles=<subs.srt>" -c:a copy -y <output.mp4>
  ```

- Progress reporting via ffmpeg `-progress pipe:1`
- Save/export the subtitled video file

**Usable state:** User can export standalone subtitle files or get a video with burned-in subtitles.

## M11: Settings & Model Management

**Goal:** User can configure whisper models, Ollama settings, and app preferences.

- Settings view:
  - Whisper model selector (tiny/base/small/medium/large) with download management
  - Ollama endpoint configuration (custom host/port)
  - Default language selection (auto, en, etc.)
  - Thread count for whisper
- Model download/delete management with disk usage display
- Re-check Ollama connection from settings
- Re-run preflight checks from settings
- Persist all settings in SQLite settings table

**Usable state:** User can tune performance/accuracy tradeoff and manage disk usage.

## M12: Polish & Quality of Life/Parking Lot

**Goal:** Production-quality UX.

- [ ] Page transitions with solid-motionone (view enter/exit animations)
- [ ] Loading/processing states with skeleton screens
- [ ] Error handling with user-friendly messages and retry actions
- [ ] Drag-and-drop audio/video file import (in addition to file picker)
- [ ] Keyboard shortcuts (Ctrl+N record, Ctrl+F search, etc.)
- [ ] Audio playback in Document view synced with transcript timestamps (click segment → play from that point)
- [ ] Export document as Markdown file
- [ ] Empty states for library, search results
- [ ] Window title updates based on current view
- [x] App icon and branding

## Completed

These are summarized in the project's [CHANGELOG](../CHANGELOG.md)

- M1: Project Scaffold & Shell
- M2: Preflight Splash Screen
- M3: First-Run Setup & Dependency Management
- M4: Audio Import & Transcription (with ffmpeg)
- M5: AI-Powered Document Processing
- M6: Document Library
- M7: Semantic Search
- M9: Microphone Recording
- M13: Local Embedding (Decouple from Ollama)
