import * as logger from "@tauri-apps/plugin-log";
import {
  checkPermission,
  getDevices,
  type PermissionResponse,
  requestPermission,
} from "tauri-plugin-audio-recorder-api";

const AUDIO_INPUT_DEVICE_STORAGE_KEY = "audiox:recording:device-id";

export type AudioInputDevice = { id: string; name: string; isDefault: boolean };

function isTauriRuntime(): boolean {
  return globalThis.window !== undefined && "__TAURI_INTERNALS__" in globalThis;
}

export function supportsMediaRecording(): boolean {
  return isTauriRuntime();
}

export async function listAudioInputDevices(): Promise<AudioInputDevice[]> {
  if (!isTauriRuntime()) {
    return [];
  }

  const response = await getDevices();
  return response.devices ?? [];
}

export async function checkMicrophonePermission(): Promise<PermissionResponse> {
  if (!isTauriRuntime()) {
    return { granted: false, canRequest: false };
  }
  return checkPermission();
}

export async function requestMicrophonePermission(): Promise<PermissionResponse> {
  if (!isTauriRuntime()) {
    return { granted: false, canRequest: false };
  }
  return requestPermission();
}

export function getPreferredAudioInputDeviceId(): string | null {
  try {
    const stored = localStorage.getItem(AUDIO_INPUT_DEVICE_STORAGE_KEY)?.trim();
    return stored || null;
  } catch {
    return null;
  }
}

export function setPreferredAudioInputDeviceId(deviceId: string | null): void {
  try {
    if (!deviceId) {
      localStorage.removeItem(AUDIO_INPUT_DEVICE_STORAGE_KEY);
      return;
    }
    localStorage.setItem(AUDIO_INPUT_DEVICE_STORAGE_KEY, deviceId);
  } catch {
    logger.warn("ignore storage failures in private mode or browser-only tests.");
  }
}
