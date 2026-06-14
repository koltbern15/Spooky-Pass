import { defineConfig } from "vite";

// Base Vite config for the MV3 extension.
//
// The actual multi-entry build is orchestrated by `build.mjs`, which invokes
// Vite's JS API ONCE PER ENTRY (background, content, popup), overriding the
// input + output below for each pass. Building one entry at a time and forcing
// `inlineDynamicImports: true` is what guarantees each output is a single,
// self-contained file: MV3 service workers and content scripts are loaded as a
// single file each and cannot resolve a sibling `./chunk.js`, so no shared
// chunk (e.g. `messages.ts`) may be hoisted out.
//
// `vite build` with this bare config still works (it builds `src/background.ts`
// only) but `npm run build` / `build.mjs` is the supported entry point.
export default defineConfig({
  root: __dirname,
  clearScreen: false,
  build: {
    outDir: "dist",
    emptyOutDir: true,
    assetsInlineLimit: 0,
    sourcemap: true,
    // Targets are current Chromium (Brave + Vivaldi); no legacy transpiling.
    target: "chrome120",
    rollupOptions: {
      input: { background: "src/background.ts" },
      output: {
        format: "es",
        entryFileNames: "[name].js",
        // No code-splitting: inline every imported module into the entry file.
        inlineDynamicImports: true,
      },
    },
  },
});
