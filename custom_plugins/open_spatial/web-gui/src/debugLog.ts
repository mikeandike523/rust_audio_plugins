const LOG_ENDPOINT = "http://localhost:9099/log";

type LogDetails = Record<string, unknown>;

export function debugLog(step: string, details: LogDetails = {}) {
  const payload = {
    plugin: "open_spatial",
    source: "frontend",
    step,
    timestampMs: Date.now(),
    ...details,
  };

  console.log("[open_spatial]", payload);

  void fetch(LOG_ENDPOINT, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(payload),
    keepalive: true,
  }).catch((error) => {
    console.warn("[open_spatial] remote log failed", step, error);
  });
}
