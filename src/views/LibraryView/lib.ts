export function sourceBadgeLabel(sourceType: string): string {
  if (sourceType === "microphone_recording") {
    return "Recording";
  }
  if (sourceType === "text_note") {
    return "Text Note";
  }
  if (sourceType === "text_paste") {
    return "Pasted Note";
  }
  return "Audio";
}

export function sourceIcon(sourceType: string): string {
  if (sourceType === "microphone_recording") {
    return "i-bi-mic-fill";
  }
  if (sourceType === "text_note" || sourceType === "text_paste") {
    return "i-bi-file-text-fill";
  }
  return "i-bi-file-music-fill";
}

export function isTextSource(sourceType: string): boolean {
  return sourceType === "text_note" || sourceType === "text_paste";
}
