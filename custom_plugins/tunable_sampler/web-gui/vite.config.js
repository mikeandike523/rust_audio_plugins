import react from "@vitejs/plugin-react";
import fs from "node:fs";
import { defineConfig } from "vite";

const packageJson = JSON.parse(
  fs.readFileSync(new URL("./package.json", import.meta.url), "utf8")
);



export default defineConfig({
  base: "./",
  plugins: [react()],
  define: {
    "import.meta.env.VITE_GUI_VERSION": JSON.stringify(packageJson.version),
  },
  build: {
    cssCodeSplit: false,
  },
});
