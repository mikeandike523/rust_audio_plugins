# Open Spatial

Open Spatial is a prototype far-field binaural spatializer with a webview-based GUI. The plugin now treats HRTF setup as a runtime initialization state machine: it keeps only a remote SOFA URL in code, downloads the HRTF into a persisted cache directory on first load, validates the cache on later loads, and stays silent until the measured renderer is ready.

## GUI development

From the repo root:

```bash
pnpm --dir custom_plugins/open_spatial/web-gui install
pnpm --dir custom_plugins/open_spatial/web-gui dev
```

The plugin automatically detects a local dev server by requesting
`http://localhost:5173/open-spatial`. If the response has a text MIME type,
the UI is loaded from that URL. Otherwise it falls back to the published URL
at `https://open-spatial-web-gui.vercel.app`.

The plugin persists a cache directory in its state and writes an
`asset_manifest.json` there alongside the downloaded SOFA file. The GUI shows
download/validation progress and lets the user revalidate the cache manually.
