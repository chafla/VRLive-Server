use serde::{Deserialize, Serialize};

/// The end result of a server heartbeat message
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum HeartbeatStatus {
    OK,
    CleanDisconnect(String),
    Hangup(String)
}

pub struct HeartbeatMessage {
    // TODO
}