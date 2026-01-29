# Basic Plugin Example web GUI

Minimal HTML/CSS/JS UI for the Basic Plugin Example plugin. The UI is embedded directly in the plugin via `include_str!`.

## Dev server

From the repo root:

```bash
pnpm --dir basic_plugin_example/web-gui install
pnpm --dir basic_plugin_example/web-gui dev
```

The plugin will probe `http://localhost:5173/wth-plugin-name`. If it receives a
text response, it loads the UI from that URL. Otherwise it falls back to the
published build at `https://wth-plugins-basic-plugin-example.vercel.app`.

Configure the Vite dev server to rewrite `/wth-plugin-name` to
`/basic_plugin_example` so the plugin only connects to the intended app.

## Publish embedded HTML

From the repo root:

```bash
node scripts/publish-basic_plugin_example-web-gui.mjs
```

This rebuilds the Vite app and writes `embedded.html`, which is what the plugin embeds by default.
