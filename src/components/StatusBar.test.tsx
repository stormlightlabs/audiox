import { Route, Router } from "@solidjs/router";
import { render, screen } from "@solidjs/testing-library";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { StatusBar } from "./StatusBar";

describe("StatusBar", () => {
  it("renders navigation links and status details", () => {
    render(() => (
      <Router>
        <Route
          path="/"
          component={() => (
            <StatusBar
              sidebarOpen
              onToggleSidebar={() => {}}
              preflightPhase="ready"
              completedChecks={7}
              totalChecks={8}
              appVersion="v0.1.0-3-gabc123" />
          )} />
      </Router>
    ));

    expect(screen.getByRole("link", { name: "Home" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Splash" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Setup" })).toBeInTheDocument();
    expect(screen.getByText(/PREFLIGHT/i)).toBeInTheDocument();
    expect(screen.getByText(/7\/8 checks complete/i)).toBeInTheDocument();
    expect(screen.getByText("v0.1.0-3-gabc123")).toBeInTheDocument();
  });

  it("calls toggle callback when the sidebar button is clicked", async () => {
    const user = userEvent.setup();
    const onToggleSidebar = vi.fn();

    render(() => (
      <Router>
        <Route
          path="/"
          component={() => (
            <StatusBar
              sidebarOpen={false}
              onToggleSidebar={onToggleSidebar}
              preflightPhase="running"
              completedChecks={2}
              totalChecks={8}
              appVersion="web" />
          )} />
      </Router>
    ));

    await user.click(screen.getByRole("button", { name: /hide navigation/i }));
    expect(onToggleSidebar).toHaveBeenCalledTimes(1);
  });
});
