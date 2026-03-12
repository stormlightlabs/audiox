/* eslint-disable unicorn/consistent-function-scoping */
import { cleanup } from "@solidjs/testing-library";
import "@testing-library/jest-dom/vitest";
import { afterEach, vi } from "vitest";

vi.mock(
  "solid-motionone",
  () => ({
    Motion: new Proxy({}, { get: () => (props: { children?: unknown }) => props.children as unknown }),
    Presence: (props: { children?: unknown }) => props.children as unknown,
  }),
);

Object.defineProperty(globalThis, "scrollTo", { value: vi.fn(), writable: true });

afterEach(cleanup);
