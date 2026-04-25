import fs from "node:fs";
import path from "node:path";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";
import { VitePWA } from "vite-plugin-pwa";

const packageJson = JSON.parse(
  fs.readFileSync(new URL("./package.json", import.meta.url), "utf8")
);

const devProbeRoute = "/wth-dispatch";

const devProbePlugin = () => ({
  name: "dispatch-dev-probe",
  configureServer(server) {
    const indexPath = path.resolve(server.config.root, "index.html");

    server.middlewares.use(async (req, res, next) => {
      const url = req.url?.split("?")[0];
      if (url !== devProbeRoute) {
        return next();
      }

      try {
        const rawHtml = fs.readFileSync(indexPath, "utf8");
        const html = await server.transformIndexHtml(devProbeRoute, rawHtml);
        res.statusCode = 200;
        res.setHeader("Content-Type", "text/html");
        res.end(html);
      } catch (error) {
        next(error);
      }
    });
  },
  configurePreviewServer(server) {
    const indexPath = path.resolve(server.config.root, "dist", "index.html");

    server.middlewares.use((req, res, next) => {
      const url = req.url?.split("?")[0];
      if (url !== devProbeRoute) {
        return next();
      }

      try {
        const html = fs.readFileSync(indexPath, "utf8");
        res.statusCode = 200;
        res.setHeader("Content-Type", "text/html");
        res.end(html);
      } catch (error) {
        next(error);
      }
    });
  },
});

export default defineConfig({
  base: "./",
  plugins: [
    react(),
    devProbePlugin(),
    VitePWA({
      registerType: "autoUpdate",
      injectRegister: "auto",
      workbox: {
        globPatterns: ["**/*.{js,css,html}"],
        cleanupOutdatedCaches: true,
      },
      manifest: {
        name: "Dispatch",
        short_name: "Dispatch",
        theme_color: "#111113",
        background_color: "#111113",
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
