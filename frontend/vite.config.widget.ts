import { defineConfig } from "vite";
import { fileURLToPath, URL } from "node:url";

// Standalone single-file build of the embeddable widget. No hashing, no PWA,
// no code-splitting — one self-initializing cs-widget.js served as a static asset.
export default defineConfig({
  resolve: { alias: { "@": fileURLToPath(new URL("./src", import.meta.url)) } },
  build: {
    emptyOutDir: false, // do NOT wipe the SPA's dist
    lib: {
      entry: fileURLToPath(new URL("./src/cs-widget/index.ts", import.meta.url)),
      formats: ["iife"],
      name: "CsWidget",
      fileName: () => "cs-widget.js",
    },
    rollupOptions: {
      output: { entryFileNames: "cs-widget.js", inlineDynamicImports: true },
    },
  },
});
