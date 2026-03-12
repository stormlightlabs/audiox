# ffmpeg Integration

## Distribution Strategy

Bundle ffmpeg as a **Tauri sidecar** (`bundle.externalBin`) for production builds.

- Preflight resolves in order: sidecar → managed runtime cache → `PATH`
- Runtime download is not required for `ffmpeg` in end-user flows

Suggested static build sources by platform:

| Platform    | Source                                                                                                                 | Approx. Size |
| ----------- | ---------------------------------------------------------------------------------------------------------------------- | ------------ |
| macOS arm64 | [evermeet.cx](https://evermeet.cx/ffmpeg/) or [shaka-project](https://github.com/shaka-project/static-ffmpeg-binaries) | ~43 MB       |
| macOS x64   | evermeet.cx                                                                                                            | ~45 MB       |
| Linux x64   | [johnvansickle.com](https://johnvansickle.com/ffmpeg/)                                                                 | ~50 MB       |
| Windows x64 | [gyan.dev](https://www.gyan.dev/ffmpeg/builds/)                                                                        | ~50 MB       |

### Roles in the Pipeline

ffmpeg is the universal audio/video format glue. It serves three roles:

**1. Audio Format Conversion (pre-transcription)**:

All audio must become 16kHz mono 16-bit PCM WAV before whisper.cpp can process it. ffmpeg handles any input format:

```sh
ffmpeg -i <input> -ar 16000 -ac 1 -c:a pcm_s16le -y <output.wav>
```

This replaces the `hound` crate for conversion — ffmpeg handles mp3, m4a, ogg, opus, flac, webm, and any video container.

**2. Audio Extraction from Video**:

When a video is downloaded via yt-dlp (or imported directly), extract audio for transcription:

```sh
ffmpeg -i <video.mp4> -vn -ar 16000 -ac 1 -c:a pcm_s16le -y <output.wav>
```

- `-vn` — discard video stream

**3. Subtitle Burn-in (video export)**:

After generating subtitles from a transcript, burn them into a video file for shareable export:

```sh
ffmpeg -i <video.mp4> -vf "subtitles=<subs.srt>" -c:a copy -y <output.mp4>
```

The `subtitles` filter requires libass (included in standard static builds). Custom styling can be applied via the `force_style` parameter.

### Progress Reporting

Use `-progress pipe:1` to get machine-readable progress on stdout:

```sh
ffmpeg -i <input> -ar 16000 -ac 1 -c:a pcm_s16le -progress pipe:1 -y <output.wav>
```

Output (key=value pairs, repeated every ~500ms):

```text
out_time=00:00:04.096000
speed=8.19x
progress=continue
```

Get total duration first via:

```sh
ffmpeg -i <input> 2>&1 | grep "Duration"
# or parse from yt-dlp metadata
```

Compare `out_time` against total duration for percentage. `progress=end` signals completion.
