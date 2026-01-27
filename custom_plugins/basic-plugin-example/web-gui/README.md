# Basic Plugin Example web GUI

Minimal HTML/CSS/JS UI for the Basic Plugin Example plugin. The UI is embedded directly in the plugin via `include_str!`.

## Dev server

From the repo root:

```bash
pnpm install
pnpm start-basic-plugin-example-gui-dev
```

To point the plugin at the dev server, set `BASIC_PLUGIN_EXAMPLE_GUI_DEV_SERVER=1` before launching the host.
