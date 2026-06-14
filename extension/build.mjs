// Multi-entry MV3 build orchestrator.
//
// Builds each extension entry (background service worker, content script, popup)
// as its OWN single, self-contained file by running Vite once per entry with
// `inlineDynamicImports: true`. A single multi-input Rollup build always factors
// shared modules (e.g. `src/messages.ts`) into a separate chunk; an MV3 service
// worker and an injected content script are each loaded as one file and cannot
// resolve a sibling `./chunk.js` at runtime, so we must inline instead.
//
// Static assets that are not part of any JS graph (manifest.json, popup.html)
// are copied into `dist/` after the JS is built.
//
// Usage: `node build.mjs [--watch]`

import { build } from "vite";
import { copyFileSync, mkdirSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = dirname(fileURLToPath(import.meta.url));
const outDir = resolve(root, "dist");
const watch = process.argv.includes("--watch");

const ENTRIES = {
  background: "src/background.ts",
  content: "src/content.ts",
  popup: "src/popup.ts",
};

async function buildEntry(name, input, isFirst) {
  await build({
    root,
    // Inline the entire config here so nothing merges with (and re-adds inputs
    // from) vite.config.ts — a single input per pass is required for
    // inlineDynamicImports.
    configFile: false,
    clearScreen: false,
    build: {
      outDir: "dist",
      // Only the first pass clears dist; later passes append their one file.
      emptyOutDir: isFirst,
      sourcemap: true,
      assetsInlineLimit: 0,
      target: "chrome120",
      ...(watch ? { watch: {} } : {}),
      rollupOptions: {
        input: { [name]: resolve(root, input) },
        output: {
          format: "es",
          entryFileNames: "[name].js",
          inlineDynamicImports: true,
        },
      },
    },
  });
}

function copyStatic() {
  mkdirSync(outDir, { recursive: true });
  for (const file of ["manifest.json", "popup.html"]) {
    copyFileSync(resolve(root, file), resolve(outDir, file));
  }
}

const names = Object.keys(ENTRIES);
for (let i = 0; i < names.length; i++) {
  const name = names[i];
  await buildEntry(name, ENTRIES[name], i === 0);
}
copyStatic();
