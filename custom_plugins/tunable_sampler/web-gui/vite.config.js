import react from "@vitejs/plugin-react";
import fs from "node:fs";
import { defineConfig } from "vite";
import { VitePWA } from "vite-plugin-pwa";

const packageJson = JSON.parse(
  fs.readFileSync(new URL("./package.json", import.meta.url), "utf8")
);



export default defineConfig({
  base: "./",
  plugins: [
    react(),
    VitePWA({
      registerType: "autoUpdate",
      injectRegister: "auto",
      workbox: {
        globPatterns: ["**/*.{js,css,html}"],
        cleanupOutdatedCaches: true,
      },
      manifest: {
        name: "Tunable Sampler",
        short_name: "TunableSampler",
        theme_color: "#ffffff",
        background_color: "#f6f1ea",
        display: "standalone",
      },
    }),
  ],
  define: {
    "import.meta.env.VITE_GUI_VERSION": JSON.stringify(packageJson.version),
  },
  build: {
    cssCodeSplit: false,
  },
});
