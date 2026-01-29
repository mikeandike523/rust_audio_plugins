# Basic Plugin Example

Basic Plugin Example is a minimal saturation effect (clip(sat * input) * gain) with a webview-based GUI. It is meant to serve as a practical starter template for new plugins.

## GUI development

From the repo root:

```bash
pnpm --dir basic_plugin_example/web-gui install
pnpm --dir basic_plugin_example/web-gui dev
```

To have the plugin load the dev server instead of the embedded HTML, set:

```bash
BASIC_PLUGIN_EXAMPLE_GUI_DEV_SERVER=1
```
