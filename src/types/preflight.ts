export const PREFLIGHT_EVENT = "preflight://check";

export const PREFLIGHT_CHECK_ORDER = [
  "whisper_cli",
  "ffmpeg",
  "yt_dlp",
  "whisper_model",
  "embedding_model",
  "ollama_server",
  "ollama_models",
  "database",
] as const;

export type PreflightCheck = (typeof PREFLIGHT_CHECK_ORDER)[number];

export type CheckStatus = "pass" | "fail" | "warn";

export type PreflightPhase = "idle" | "running" | "ready" | "failed";

export type CheckDisplayStatus = CheckStatus | "pending";

export type PreflightCheckDetail = { check: PreflightCheck; status: CheckStatus; message: string };
