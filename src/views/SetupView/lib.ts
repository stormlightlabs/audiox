import { ProgressStatus } from "$/types";

export const WHISPER_PROGRESS_EVENT = "setup://whisper-progress";
export const EMBEDDING_PROGRESS_EVENT = "setup://embedding-progress";
export const OLLAMA_PROGRESS_EVENT = "setup://ollama-progress";
export const GEMMA_REQUIREMENT = "gemma3";

export const STEP_ORDER: StepKey[] = ["whisper_model", "embedding_model", "metadata_backend", "ollama_fallback"];

export type StepKey = "whisper_model" | "embedding_model" | "metadata_backend" | "ollama_fallback";
export type StepStatus = "pending" | "running" | "pass" | "fail" | "blocked" | "warn";
export type SetupPhase = "checking" | "idle" | "running" | "failed" | "completed";

export type SetupStep = {
  key: StepKey;
  title: string;
  description: string;
  status: StepStatus;
  message: string;
  progress: number;
};

export type WhisperProgressEvent = {
  modelName: string;
  status: ProgressStatus;
  message: string;
  downloadedBytes: number;
  totalBytes: number | null;
  percent: number;
};

export type OllamaProgressEvent = {
  modelName: string;
  status: ProgressStatus;
  message: string;
  completed: number;
  total: number;
  percent: number;
};

export type EmbeddingProgressEvent = { status: ProgressStatus; message: string; percent: number };
