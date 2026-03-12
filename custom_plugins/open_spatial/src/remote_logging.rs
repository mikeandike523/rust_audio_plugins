use serde::Serialize;
use serde_json::json;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Best-effort logger that forwards JSON payloads to the local dev log server.
#[derive(Clone)]
pub struct RemoteLogger {
    url: String,
    agent: ureq::Agent,
}

impl RemoteLogger {
    pub fn new(port: u16) -> Self {
        Self {
            url: format!("http://localhost:{port}/log"),
            agent: ureq::AgentBuilder::new()
                .timeout_connect(Duration::from_secs(1))
                .timeout_read(Duration::from_secs(1))
                .build(),
        }
    }

    pub fn log<T: Serialize>(&self, entry: &T) {
        let _ = self
            .agent
            .post(&self.url)
            .set("Content-Type", "application/json")
            .send_json(entry);
    }

    pub fn log_step(&self, step: &str, detail: impl Into<String>) {
        let timestamp_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis())
            .unwrap_or(0);

        self.log(&json!({
            "plugin": "open_spatial",
            "source": "rust",
            "step": step,
            "detail": detail.into(),
            "timestampMs": timestamp_ms,
        }));
    }
}
