# Sine Fold

Sine Fold is a sine-folding distortion effect (gain * sin(k * pi/2 * input)) with a webview-based GUI.

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
