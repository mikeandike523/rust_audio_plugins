import { defineConfig } from "vite";
import react from "@vitejs/plugin-react-swc";
import monaco from "@tomjs/vite-plugin-monaco-editor";
import { VitePWA } from "vite-plugin-pwa";

// https://vite.dev/config/
export default defineConfig({
  plugins: [
    react(),
    monaco({ local: true }),
    VitePWA({
      registerType: "autoUpdate",
      injectRegister: "auto",
      workbox: {
        globPatterns: ["**/*.{js,css,html,wasm}"],
        cleanupOutdatedCaches: true,
        maximumFileSizeToCacheInBytes: 15 * 1024 * 1024,
      },
      manifest: {
        name: "Harmonic NXO",
        short_name: "HarmonicNXO",
        theme_color: "#1a1a1a",
        background_color: "#1a1a1a",
        display: "standalone",
      },
    }),
  ],
});
