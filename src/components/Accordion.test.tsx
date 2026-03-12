import { render, screen } from "@solidjs/testing-library";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { Accordion } from "./Accordion";

describe("Accordion", () => {
  it("starts collapsed by default and toggles content visibility", async () => {
    const user = userEvent.setup();
    render(() => (
      <Accordion title="Runtime details">
        <p>Hidden body content</p>
      </Accordion>
    ));

    const toggle = screen.getByRole("button", { name: /Runtime details/i });
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByText("Hidden body content")).not.toBeInTheDocument();

    await user.click(toggle);
    expect(toggle).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByText("Hidden body content")).toBeInTheDocument();

    await user.click(toggle);
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByText("Hidden body content")).not.toBeInTheDocument();
  });

  it("respects defaultOpen and renders summary text", () => {
    render(() => (
      <Accordion title="Preflight checklist" summary="7/7 checks complete" defaultOpen>
        <p>Checklist body</p>
      </Accordion>
    ));

    const toggle = screen.getByRole("button", { name: /Preflight checklist\s*7\/7 checks complete/i });
    expect(toggle).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByText("Checklist body")).toBeInTheDocument();
    expect(screen.getByText("7/7 checks complete")).toBeInTheDocument();
  });
});
