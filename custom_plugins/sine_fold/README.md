# Sine Fold

Sine Fold is a minimal saturation effect (clip(sat * input) * gain) with a webview-based GUI. It is meant to serve as a practical starter template for new plugins.

## GUI development

From the repo root:

```bash
pnpm --dir sine_fold/web-gui install
pnpm --dir sine_fold/web-gui dev
```

The plugin automatically detects a local dev server by requesting
`http://localhost:5173/wth-plugin-name`. If the response has a text MIME type,
the UI is loaded from that URL. Otherwise it falls back to the published URL
at `https://wth-plugins-sine-fold.vercel.app`.

Configure the Vite dev server to rewrite `/wth-plugin-name` to
`/sine_fold` so the plugin only connects to the intended app.
