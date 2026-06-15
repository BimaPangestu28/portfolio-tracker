/// <reference types="vitest" />
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { VitePWA } from "vite-plugin-pwa";
import { fileURLToPath, URL } from "node:url";

export default defineConfig({
  plugins: [
    react(),
    VitePWA({
      // Auto-update the service worker in the background; no user prompt needed.
      registerType: "autoUpdate",
      // Static assets copied as-is (referenced from index.html / manifest).
      includeAssets: ["favicon.svg", "apple-touch-icon.png"],
      manifest: {
        name: "Noah",
        short_name: "Noah",
        description: "Noah — asisten pribadi: tugas, agenda, & keuangan.",
        lang: "id",
        theme_color: "#2977f5",
        background_color: "#ffffff",
        display: "standalone",
        orientation: "portrait",
        start_url: "/",
        scope: "/",
        icons: [
          { src: "pwa-192x192.png", sizes: "192x192", type: "image/png", purpose: "any" },
          { src: "pwa-512x512.png", sizes: "512x512", type: "image/png", purpose: "any" },
          { src: "pwa-maskable-512x512.png", sizes: "512x512", type: "image/png", purpose: "maskable" },
        ],
      },
      workbox: {
        // Precache the app shell (hashed static build assets) ONLY.
        // Intentionally NO runtimeCaching for /api — financial data stays
        // network-only so the UI never shows stale or cached portfolio figures.
        globPatterns: ["**/*.{js,css,html,svg,png,woff2}"],
        navigateFallbackDenylist: [/^\/api/],
      },
      // SW disabled in dev so it never interferes with HMR or the test runner.
      devOptions: { enabled: false },
    }),
  ],
  resolve: {
    alias: { "@": fileURLToPath(new URL("./src", import.meta.url)) },
  },
  server: {
    proxy: {
      "/api": { target: "http://localhost:8080", changeOrigin: true, rewrite: (p) => p.replace(/^\/api/, "") },
    },
  },
  test: {
    globals: true,
    environment: "jsdom",
    setupFiles: ["./src/test/setup.ts"],
  },
});
