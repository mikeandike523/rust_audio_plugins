# Dispatch

Dispatch is a minimal audio effect that applies a gain parameter and exposes a webview-based GUI. It is intended as a starter template for new plugins.

## GUI development

From the repo root:

```bash
pnpm install
pnpm start-dispatch-gui-dev
```

To have the plugin load the dev server instead of the embedded HTML, set:

```bash
DISPATCH_GUI_DEV_SERVER=1
```
