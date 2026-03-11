import { cleanup } from "@solidjs/testing-library";
import "@testing-library/jest-dom/vitest";
import { afterEach } from "vitest";
import { vi } from "vitest";

vi.mock(
  "solid-motionone",
  () => ({
    Motion: { div: (props: { children?: unknown }) => props.children as unknown },
    Presence: (props: { children?: unknown }) => props.children as unknown,
  }),
);

afterEach(cleanup);
