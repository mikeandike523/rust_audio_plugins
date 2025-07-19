import { defineConfig } from "vite";
import react from "@vitejs/plugin-react-swc";
import monaco from "@tomjs/vite-plugin-monaco-editor";

// https://vite.dev/config/
export default defineConfig({
  plugins: [
    react(),
    monaco({ local: true }),
  ],
});
