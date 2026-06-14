import { defineConfig } from "vite";

// Vite config for the Spooky-Pass desktop webview UI.
// Port 1420 is the conventional Tauri dev port; clearScreen is disabled so the
// Rust/Tauri logs in the same terminal are not wiped on each rebuild.
export default defineConfig({
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
  },
});
