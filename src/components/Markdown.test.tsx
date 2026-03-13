import { fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { Markdown } from "./Markdown";

const { invokeMock } = vi.hoisted(() => ({ invokeMock: vi.fn() }));

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

describe("Markdown", () => {
  beforeEach(() => {
    const storage = new Map<string, string>();

    invokeMock.mockReset();
    Object.defineProperty(globalThis, "localStorage", {
      configurable: true,
      value: {
        getItem: (key: string) => storage.get(key) ?? null,
        setItem: (key: string, value: string) => {
          storage.set(key, value);
        },
        removeItem: (key: string) => {
          storage.delete(key);
        },
      },
    });
  });

  it("renders html returned by Tauri and shows available themes", async () => {
    invokeMock.mockImplementation((command: string, args?: { content?: string; theme?: string }) => {
      if (command === "list_markdown_themes") {
        return Promise.resolve(["zenburn", "tokyo-night"]);
      }

      if (command === "render_markdown") {
        return Promise.resolve({ html: `<h1>Rendered</h1><p>${args?.theme}</p>`, theme: args?.theme ?? "zenburn" });
      }

      return Promise.reject(new Error(`unexpected command: ${command}`));
    });

    render(() => <Markdown content={"# Rendered"} />);

    expect(await screen.findByRole("combobox", { name: "Syntax theme" })).toBeInTheDocument();
    expect(await screen.findByText("Rendered")).toBeInTheDocument();
    expect(await screen.findByText("zenburn")).toBeInTheDocument();
    expect(invokeMock).toHaveBeenCalledWith("list_markdown_themes");
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("render_markdown", { content: "# Rendered", theme: "zenburn" })
    );
  });

  it("re-renders with the newly selected theme", async () => {
    invokeMock.mockImplementation((command: string, args?: { content?: string; theme?: string }) => {
      if (command === "list_markdown_themes") {
        return Promise.resolve(["zenburn", "tokyo-night"]);
      }

      if (command === "render_markdown") {
        return Promise.resolve({ html: `<p>${args?.theme}</p>`, theme: args?.theme ?? "zenburn" });
      }

      return Promise.reject(new Error(`unexpected command: ${command}`));
    });

    render(() => <Markdown content={"```ts\nconst ready = true;\n```"} />);

    const select = await screen.findByRole("combobox", { name: "Syntax theme" });
    fireEvent.input(select, { currentTarget: { value: "tokyo-night" }, target: { value: "tokyo-night" } });

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("render_markdown", {
        content: "```ts\nconst ready = true;\n```",
        theme: "tokyo-night",
      })
    );
    await waitFor(() => expect(select).toHaveValue("tokyo-night"));
    expect(await screen.findByText("tokyo-night")).toBeInTheDocument();
    expect(globalThis.localStorage.getItem("audiox.markdown.theme")).toBe("tokyo-night");
  });

  it("hydrates the initial theme from localStorage", async () => {
    globalThis.localStorage.setItem("audiox.markdown.theme", "tokyo-night");

    invokeMock.mockImplementation((command: string, args?: { content?: string; theme?: string }) => {
      if (command === "list_markdown_themes") {
        return Promise.resolve(["zenburn", "tokyo-night"]);
      }

      if (command === "render_markdown") {
        return Promise.resolve({ html: `<p>${args?.theme}</p>`, theme: args?.theme ?? "zenburn" });
      }

      return Promise.reject(new Error(`unexpected command: ${command}`));
    });

    render(() => <Markdown content={"# Persisted"} />);

    const select = await screen.findByRole("combobox", { name: "Syntax theme" });
    await waitFor(() => expect(select).toHaveValue("tokyo-night"));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("render_markdown", { content: "# Persisted", theme: "tokyo-night" })
    );
  });

  it("does not ask Tauri to render empty content", async () => {
    invokeMock.mockImplementation((command: string) => {
      if (command === "list_markdown_themes") {
        return Promise.resolve(["zenburn"]);
      }

      return Promise.reject(new Error(`unexpected command: ${command}`));
    });

    render(() => <Markdown content={"   "} />);

    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("list_markdown_themes"));
    expect(invokeMock).not.toHaveBeenCalledWith("render_markdown", expect.anything());
  });
});
