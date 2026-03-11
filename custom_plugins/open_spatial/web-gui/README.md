# Open Spatial web GUI

Vite + React (TypeScript) UI for the Open Spatial plugin.

## Dev server

From the repo root:

```bash
pnpm --dir open_spatial/web-gui install
pnpm --dir open_spatial/web-gui dev
```

The plugin probes `http://localhost:5173/wth-plugin-name`. The Vite dev server
now responds with the app shell for that route so the plugin can confirm it's
running in debug mode.

## Publish embedded HTML

Embedded HTML is no longer used for this plugin. You can delete any old artifacts.
