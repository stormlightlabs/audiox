export type NotePreview = { sourcePath: string | null; sourceName: string; title: string; content: string };

export function snippet(content: string): string {
  const trimmed = content.trim();
  if (trimmed.length <= 500) {
    return trimmed;
  }
  return `${trimmed.slice(0, 500)}...`;
}

export const IMPORT_CONVERSION_PROGRESS_EVENT = "import://conversion-progress";
export const IMPORT_TRANSCRIPTION_PROGRESS_EVENT = "import://transcription-progress";
export const IMPORT_METADATA_PROGRESS_EVENT = "import://metadata-progress";
export const TEXT_EXTENSIONS = new Set(["txt", "md"]);
