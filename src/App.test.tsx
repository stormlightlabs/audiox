import { render, screen, waitFor } from "@solidjs/testing-library";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";

const { invokeMock, listenMock } = vi.hoisted(() => ({ invokeMock: vi.fn(), listenMock: vi.fn() }));

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));
vi.mock("@tauri-apps/api/event", () => ({ listen: listenMock }));

const successfulPreflight = {
  whisper_cli: "pass",
  ffmpeg: "pass",
  yt_dlp: "warn",
  whisper_model: "pass",
  ollama_server: "pass",
  ollama_models: "pass",
  database: "pass",
  should_open_setup: false,
  all_required_passed: true,
  details: [
    { check: "whisper_cli", status: "pass", message: "whisper-cli is available on PATH as 'whisper-cli'." },
    { check: "ffmpeg", status: "pass", message: "ffmpeg is available on PATH as 'ffmpeg'." },
    { check: "yt_dlp", status: "warn", message: "yt-dlp missing." },
    { check: "whisper_model", status: "pass", message: "whisper model files are present." },
    { check: "ollama_server", status: "pass", message: "Ollama server is reachable." },
    { check: "ollama_models", status: "pass", message: "Required Ollama models are available." },
    { check: "database", status: "pass", message: "SQLite database is accessible." },
  ],
};

const failingPreflight = {
  ...successfulPreflight,
  whisper_cli: "fail",
  all_required_passed: false,
  details: successfulPreflight.details.map((detail) => detail.check === "whisper_cli"
    ? { ...detail, status: "fail", message: "whisper-cli is missing. Install 'whisper-cli' on PATH." }
    : detail
  ),
};

const setupPreflight = {
  ...failingPreflight,
  whisper_model: "fail",
  should_open_setup: true,
  details: failingPreflight.details.map((detail) => detail.check === "whisper_model"
    ? { ...detail, status: "fail", message: "No whisper model found in appdata/models." }
    : detail
  ),
};

const setupStatus = {
  whisper_model_ready: false,
  ollama_server_ready: false,
  missing_ollama_models: ["nomic-embed-text", "gemma3:4b"],
  setup_completed: false,
  all_required_ready: false,
  guidance: ["Install Ollama and start it with `ollama serve`."],
};

describe("Preflight flow", () => {
  beforeEach(() => {
    globalThis.history.replaceState({}, "", "/");
    invokeMock.mockReset();
    listenMock.mockReset();
    listenMock.mockResolvedValue(() => Promise.resolve());
  });

  it("runs preflight on launch and auto-transitions to Library when checks pass", async () => {
    invokeMock.mockResolvedValue(successfulPreflight);
    render(() => <App />);

    expect(await screen.findByRole("heading", { name: "Audio X" })).toBeInTheDocument();
    expect(listenMock).toHaveBeenCalledWith("preflight://check", expect.any(Function));
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("preflight"));
    await waitFor(() => expect(screen.getByRole("heading", { name: "Document library" })).toBeInTheDocument(), {
      timeout: 2500,
    });
  });

  it("stays on splash with guidance when required checks fail and supports retry", async () => {
    const user = userEvent.setup();
    invokeMock.mockResolvedValueOnce(failingPreflight).mockResolvedValueOnce(successfulPreflight);
    render(() => <App />);

    expect(await screen.findByRole("heading", { name: "Audio X" })).toBeInTheDocument();
    const guidanceMatches = await screen.findAllByText("whisper-cli is missing. Install 'whisper-cli' on PATH.");
    expect(guidanceMatches.length).toBeGreaterThan(0);

    await user.click(screen.getByRole("button", { name: "Retry checks" }));
    await waitFor(() => expect(invokeMock).toHaveBeenCalledTimes(2));
    await waitFor(() => expect(screen.getByRole("heading", { name: "Document library" })).toBeInTheDocument(), {
      timeout: 2500,
    });
  });

  it("auto-transitions to Setup when model checks indicate first-run dependencies are missing", async () => {
    invokeMock.mockImplementation((command: string) => {
      if (command === "preflight") {
        return Promise.resolve(setupPreflight);
      }
      if (command === "check_setup") {
        return Promise.resolve(setupStatus);
      }
      return Promise.resolve();
    });
    render(() => <App />);

    expect(await screen.findByRole("heading", { name: "Audio X" })).toBeInTheDocument();
    await waitFor(() => expect(screen.getByRole("heading", { name: "First-run setup wizard" })).toBeInTheDocument(), {
      timeout: 3000,
    });
  });
});
