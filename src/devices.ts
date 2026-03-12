const AUDIO_INPUT_DEVICE_STORAGE_KEY = "audiox:recording:device-id";

export function supportsMediaRecording(): boolean {
  return typeof navigator !== "undefined" && "mediaDevices" in navigator && "MediaRecorder" in globalThis;
}

export async function listAudioInputDevices(): Promise<MediaDeviceInfo[]> {
  if (!navigator.mediaDevices?.enumerateDevices) {
    return [];
  }

  const devices = await navigator.mediaDevices.enumerateDevices();
  return devices.filter((device) => device.kind === "audioinput");
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
    // Ignore storage failures in private mode or browser-only tests.
  }
}
