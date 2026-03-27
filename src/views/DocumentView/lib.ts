export type TranscriptSegment = { startMs: number; endMs: number; text: string };

export function segmentDomKey(segment: TranscriptSegment): string {
  return `${segment.startMs}-${segment.endMs}`;
}
