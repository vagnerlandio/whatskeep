import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { viteSingleFile } from "vite-plugin-singlefile";

// A single index.html, with JS, CSS and the Argon2id WASM inlined: the exported
// bundle depends on no file other than the data.
export default defineConfig({
  plugins: [react(), viteSingleFile()],
  build: {
    target: "es2022",
    assetsInlineLimit: Number.MAX_SAFE_INTEGER,
    cssCodeSplit: false,
  },
});
