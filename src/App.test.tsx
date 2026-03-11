import { render, screen, waitFor } from "@solidjs/testing-library";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";

const { invokeMock } = vi.hoisted(() => ({ invokeMock: vi.fn() }));

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

const bootstrapPayload = {
  appDataDir: "/tmp/audiox/appdata",
  databasePath: "/tmp/audiox/appdata/db/audiox.db",
  createdDirectories: ["models", "audio", "video", "subtitles", "db"],
  schemaVersion: 1,
};

describe("App shell routes", () => {
  beforeEach(() => {
    globalThis.history.replaceState({}, "", "/");
    invokeMock.mockReset();
    invokeMock.mockResolvedValue(bootstrapPayload);
  });

  it("renders splash view by default and initializes app", async () => {
    render(() => <App />);

    expect(await screen.findByRole("heading", { name: "Splash and startup checks" })).toBeInTheDocument();
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("initialize_app"));
    expect(await screen.findByText(bootstrapPayload.databasePath)).toBeInTheDocument();
  });

  it("navigates between placeholder milestone views", async () => {
    const user = userEvent.setup();
    render(() => <App />);

    await screen.findByRole("heading", { name: "Splash and startup checks" });

    await user.click(screen.getByRole("link", { name: "Library" }));
    expect(await screen.findByRole("heading", { name: "Document library" })).toBeInTheDocument();

    await user.click(screen.getByRole("link", { name: "Settings" }));
    expect(await screen.findByRole("heading", { name: "System configuration" })).toBeInTheDocument();
  });

  it("shows bootstrap failures and supports retry", async () => {
    const user = userEvent.setup();
    invokeMock.mockRejectedValueOnce(new Error("database unavailable"));
    invokeMock.mockResolvedValueOnce(bootstrapPayload);

    render(() => <App />);

    expect(await screen.findByRole("alert")).toHaveTextContent("database unavailable");
    await user.click(screen.getByRole("button", { name: "Retry initialization" }));

    expect(await screen.findByText(bootstrapPayload.databasePath)).toBeInTheDocument();
    expect(invokeMock).toHaveBeenCalledTimes(2);
  });
});
