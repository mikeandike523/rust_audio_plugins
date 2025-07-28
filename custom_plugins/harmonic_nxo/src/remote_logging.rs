use serde::Serialize;
use std::time::Duration;

#[derive(Clone)]
/// Helper for sending JSON logs to a local web server.
///
/// The logger is best-effort: if the server isn't reachable then any errors are
/// ignored so logging never interferes with the plugin.
#[derive(Clone)]
pub struct RemoteLogger {
    url: String,
    agent: ureq::Agent,
}

impl RemoteLogger {
    /// Create a new logger for the given port on localhost.
    pub fn new(port: u16) -> Self {
        Self {
            url: format!("http://localhost:{port}/log"),
            agent: ureq::AgentBuilder::new()
                .timeout_connect(Duration::from_secs(1))
                .timeout_read(Duration::from_secs(1))
                .build(),
        }
    }

    /// Send a log entry to the server.
    ///
    /// Any errors are swallowed so that logging is effectively optional.
    pub fn log<T: Serialize>(&self, entry: &T) {
        let _ = self
            .agent
            .post(&self.url)
            .set("Content-Type", "application/json")
            .send_json(entry);
    }
}
