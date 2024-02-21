/**
 * Basic protocol for the main client/server handshake initialized on connection.
 * Once this has been completed, the client/server is moved into the ready state.
 */

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::UserIDType;

// enum HandshakeErrorVariant {
//     TimedOut,
//     InvalidServerState
// }


/// Initial message sent in response to a new connection.
#[derive(Serialize, Deserialize, Debug)]
pub struct HandshakeAck {
    /// Informing of their user ID
    user_id: UserIDType,
    /// Pretty name for the server.
    server_pretty_identifier: String,
}

impl HandshakeAck {
    pub fn new(user_id: UserIDType, server_pretty_identifier: String) -> Self {
        Self {
            user_id,
            server_pretty_identifier
        }

    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct HandshakeSynAck {
    /// Identifier confirming their type -- performer or audience
    pub user_type: u16,
    /// Confirming own ID
    pub user_id: UserIDType,
    /// Pretty name for the client
    pub own_identifier: String,
    /// Flags denoting the user's capabilities. These will consist of some known strings to denote things like mocap availability, microphone availability, etc.
    pub user_flags: Vec<String>,
    /// Port made available for receiving backing track data.
    pub backing_track_port: u16,
    /// Port made available for receiving future server events after handshake completion.
    pub server_event_port: u16,
    /// Port made available for receiving performer vrtp data
    pub vrtp_mocap_port: u16,
    /// Port made available for receiving audience motion capture data.
    pub audience_motion_capture_port: u16,
    /// Additional, named ports that the client wishes to let the server know it is making available for data receipt.
    pub extra_ports: HashMap<String, u16>,
}

/// Last message of handshake, indicating that the server is okay and things are ready to go.
#[derive(Serialize, Deserialize, Debug)]
pub struct HandshakeCompletion {
    /// Ports for different services that should be made 
    pub extra_ports: HashMap<String, u16>
}


/// Struct indicating an error of some kind has taken place.
#[derive(Serialize, Deserialize, Debug)]
struct HandshakeError {
    /// Whether or not the user should try to reconnect after this, or if this error indicates that the connection should not be re-instated.
    should_reconnect: bool,
    /// Error name/identifier
    err_name: String,
    /// Error message
    err_message: String,

}