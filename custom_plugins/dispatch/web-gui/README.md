# Dispatch web GUI

Minimal HTML/CSS/JS UI for the Dispatch plugin. The UI is embedded directly in the plugin via `include_str!`.

## Dev server

From the repo root:

```bash
pnpm install
pnpm start-dispatch-gui-dev
```

To point the plugin at the dev server, set `DISPATCH_GUI_DEV_SERVER=1` before launching the host.
