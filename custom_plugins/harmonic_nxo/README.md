# Harmonic NXO

This plugin uses a small logging server for development. The server is powered by [Pino](https://github.com/pinojs/pino) and listens on **port 9099**.

## Running the logging server

From the repository root run:

```bash
pnpm install
pnpm start-log-server
```

Logs are printed to the console. Any time the frontend sends a new harmonic definition, the plugin posts a JSON payload to `http://localhost:9099/log` which will appear in the server output. If the server isn't running the plugin simply ignores the error, so logging never interrupts normal operation.

