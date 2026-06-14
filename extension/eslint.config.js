import js from "@eslint/js";
import tseslint from "typescript-eslint";
import globals from "globals";

// Flat ESLint config for the Spooky-Pass extension.
// Both background (service worker) and content/popup run in a browser-ish
// runtime with the `chrome.*` namespace available; `globals.webextensions`
// provides the `chrome`/`browser` globals.
export default tseslint.config(
  {
    ignores: ["dist", "node_modules"],
  },
  js.configs.recommended,
  ...tseslint.configs.recommended,
  {
    files: ["src/**/*.ts"],
    languageOptions: {
      ecmaVersion: 2020,
      sourceType: "module",
      globals: {
        ...globals.browser,
        ...globals.webextensions,
        ...globals.serviceworker,
      },
    },
    rules: {
      "@typescript-eslint/no-unused-vars": [
        "error",
        { argsIgnorePattern: "^_", varsIgnorePattern: "^_" },
      ],
    },
  },
);
