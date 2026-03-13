use serde::Serialize;
use serde_json::json;
use std::sync::mpsc::{sync_channel, SyncSender, TrySendError};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Best-effort logger that forwards JSON payloads to the local dev log server.
#[derive(Clone)]
pub struct RemoteLogger {
    sender: SyncSender<String>,
}

impl RemoteLogger {
    pub fn new(port: u16) -> Self {
        let (sender, receiver) = sync_channel::<String>(256);
        let url = format!("http://localhost:{port}/log");

        thread::spawn(move || {
            let agent = ureq::AgentBuilder::new()
                .timeout_connect(Duration::from_secs(1))
                .timeout_read(Duration::from_secs(1))
                .build();

            while let Ok(body) = receiver.recv() {
                let _ = agent
                    .post(&url)
                    .set("Content-Type", "application/json")
                    .send_string(&body);
            }
        });

        Self { sender }
    }

    pub fn log<T: Serialize>(&self, entry: &T) {
        let Ok(body) = serde_json::to_string(entry) else {
            return;
        };

        match self.sender.try_send(body) {
            Ok(()) | Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => {}
        }
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
