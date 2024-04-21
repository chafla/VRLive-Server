use serde::{Deserialize, Serialize};

/// The end result of a server heartbeat message
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum HeartbeatStatus {
    /// The other side is fine.
    OK,
    /// The other side is choosing to disconnect, and should be properly cleaned up.
    CleanDisconnect(String),
    /// The other side is just dead. We should wait for them to reconnect.
    Hangup(String)
}

pub struct HeartbeatMessage {
    // TODO
}