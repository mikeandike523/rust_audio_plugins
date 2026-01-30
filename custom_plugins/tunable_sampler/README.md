# Tunable Sampler

Tunable Sampler is a work-in-progress instrument with a webview-based GUI. It currently outputs silence and focuses on project-folder setup for sampler development.

## GUI development

From the repo root:

```bash
pnpm --dir tunable_sampler/web-gui install
pnpm --dir tunable_sampler/web-gui dev
```

The plugin automatically detects a local dev server by requesting
`http://localhost:5173/wth-plugin-name`. If the response has a text MIME type,
the UI is loaded from that URL. Otherwise it falls back to the published URL
at `https://wth-plugins-tunable-sampler.vercel.app`.

Configure the Vite dev server to rewrite `/wth-plugin-name` to
`/tunable_sampler` so the plugin only connects to the intended app.
