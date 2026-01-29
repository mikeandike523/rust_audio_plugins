# Basic Plugin Example web GUI

Minimal HTML/CSS/JS UI for the Basic Plugin Example plugin. The UI is embedded directly in the plugin via `include_str!`.

## Dev server

From the repo root:

```bash
pnpm --dir basic_plugin_example/web-gui install
pnpm --dir basic_plugin_example/web-gui dev
```

To point the plugin at the dev server, set `BASIC_PLUGIN_EXAMPLE_GUI_DEV_SERVER=1` before launching the host.

## Publish embedded HTML

From the repo root:

```bash
node scripts/publish-basic_plugin_example-web-gui.mjs
```

This rebuilds the Vite app and writes `embedded.html`, which is what the plugin embeds by default.
