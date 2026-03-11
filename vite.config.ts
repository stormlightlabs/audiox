import { defineConfig } from "vite";
import solid from "vite-plugin-solid";
import type { ViteUserConfig } from "vitest/config";

const test: ViteUserConfig["test"] = {
  environment: "jsdom",
  globals: true,
  setupFiles: ["src/test/setup.ts"],
  ui: false,
  watch: false,
};

const host = process.env.TAURI_DEV_HOST;

export default defineConfig(async () => ({
  plugins: [solid()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 1421 } : undefined,
    watch: { ignored: ["**/src-tauri/**"] },
  },
  test,
}));
