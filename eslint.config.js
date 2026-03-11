// @ts-check
import eslint from "@eslint/js";
import tsParser from "@typescript-eslint/parser";
import react from "eslint-plugin-react";
import solid from "eslint-plugin-solid/configs/typescript";
import unicorn from "eslint-plugin-unicorn";
import { defineConfig } from "eslint/config";
import globals from "globals";
import tseslint from "typescript-eslint";
/** @typedef {import("eslint").Linter.Config} FlatConfig */
const solidConfig = /** @type {FlatConfig} */ (/** @type {unknown} */ (solid));
const unicornConfig = /** @type {FlatConfig} */ (
  /** @type {unknown} */ (unicorn.configs["flat/recommended"])
);

export default defineConfig(
  { ignores: ["dist/**", "node_modules/**", "src-tauri/target/**"] },
  eslint.configs.recommended,
  tseslint.configs.recommended,
  unicornConfig,
  {
    files: ["**/*.{ts,tsx}"],
    languageOptions: {
      parser: tsParser,
      parserOptions: { projectService: true },
      globals: globals.browser,
    },
    rules: { "no-undef": "off" },
  },
  {
    files: ["scripts/**/*.{js,mjs,cjs}", "vite.config.ts"],
    languageOptions: { globals: globals.node },
  },
  {
    files: ["**/*.tsx"],
    ...solidConfig,
    plugins: { react },
    rules: { "react/jsx-max-depth": ["error", { max: 4 }] },
  },
  {
    rules: {
      "unicorn/catch-error-name": "off",
      "unicorn/filename-case": "off",
      "unicorn/no-negated-condition": "off",
      "unicorn/no-null": "off",
      "unicorn/prefer-query-selector": "off",
      "unicorn/prefer-top-level-await": "off",
      "unicorn/prevent-abbreviations": "off",
      "unicorn/prefer-ternary": "off",
      "@typescript-eslint/no-unused-vars": [
        "warn",
        { argsIgnorePattern: "^_" },
      ],
    },
  },
);
