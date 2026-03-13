import { fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ImportView } from "./ImportView";

const { invokeMock, listenMock, openMock, readTextFileMock, navigateMock } = vi.hoisted(() => ({
  invokeMock: vi.fn(),
  listenMock: vi.fn(),
  openMock: vi.fn(),
  readTextFileMock: vi.fn(),
  navigateMock: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));
vi.mock("@tauri-apps/api/event", () => ({ listen: listenMock }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: openMock }));
vi.mock("@tauri-apps/plugin-fs", () => ({ readTextFile: readTextFileMock }));
vi.mock("@solidjs/router", async () => {
  const actual = await vi.importActual<typeof import("@solidjs/router")>("@solidjs/router");
  return { ...actual, useNavigate: () => navigateMock };
});

describe("ImportView", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    listenMock.mockReset();
    openMock.mockReset();
    readTextFileMock.mockReset();
    navigateMock.mockReset();
    listenMock.mockResolvedValue(() => Promise.resolve());
  });

  it("hides the paste panel after selecting a note file and restores it when removed", async () => {
    const user = userEvent.setup();

    openMock.mockResolvedValue("/tmp/highlighting-sample.md");
    readTextFileMock.mockResolvedValue("# Sample\n\n```ts\nconst ready = true;\n```");

    render(() => <ImportView />);

    await user.click(screen.getByRole("button", { name: "Notes" }));
    expect(screen.getByText("Paste note content")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Choose note file" }));

    expect(await screen.findByText("Preview: highlighting-sample.md")).toBeInTheDocument();
    await waitFor(() => expect(screen.queryByText("Paste note content")).not.toBeInTheDocument());

    await user.click(screen.getByRole("button", { name: "Remove file" }));

    await waitFor(() => expect(screen.queryByText("Preview: highlighting-sample.md")).not.toBeInTheDocument());
    expect(screen.getByText("Paste note content")).toBeInTheDocument();
  });

  it("keeps the file preview hidden until the notes mode is active", async () => {
    openMock.mockResolvedValue("/tmp/highlighting-sample.md");
    readTextFileMock.mockResolvedValue("# Sample");

    render(() => <ImportView />);

    expect(screen.queryByText("Paste note content")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Notes" }));
    expect(await screen.findByText("Paste note content")).toBeInTheDocument();
  });
});
