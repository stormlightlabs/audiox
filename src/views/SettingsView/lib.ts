export const WHISPER_MODEL_OPTIONS = [
  { value: "tiny", label: "tiny" },
  { value: "base", label: "base" },
  { value: "small", label: "small" },
  { value: "medium", label: "medium" },
  { value: "large", label: "large" },
  { value: "base.en", label: "base.en" },
];

export const WHISPER_LANGUAGE_OPTIONS = ["auto", "en", "es", "fr", "de", "it", "pt", "ja", "zh"];

export type WhisperModelInfo = { modelName: string; fileName: string; sizeBytes: number };

export type WhisperModelInventory = {
  selectedModel: string;
  installedModels: WhisperModelInfo[];
  totalSizeBytes: number;
};
