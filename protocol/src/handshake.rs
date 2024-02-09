/**
 * Basic protocol for the main client/server handshake initialized on connection.
 * Once this has been completed, the client/server is moved into the ready state.
 */

use std::collections::HashMap;

use crate::USER_ID_TYPE;
use serde::{Serialize, Deserialize};

enum HandshakeErrorVariant {
    TimedOut,
    InvalidServerState
}

#[derive(Serialize, Deserialize, Debug)]
enum PacketUserType {
    Audience = 1,
    Performer = 2
}


/// Initial message sent in response to a new connection.
#[derive(Serialize, Deserialize, Debug)]
struct HandshakeAck {
    /// Informing of their user ID
    user_id: USER_ID_TYPE,
    /// Server identifier length in bytes
    user_identifier_length: u16,
    /// Pretty name for the server.
    server_pretty_identifier: String,
}

#[derive(Serialize, Deserialize, Debug)]
struct HandshakeSynAck {
    /// Identifier confirming their type -- performer or audience
    user_type: PacketUserType,
    /// Confirming own ID
    user_id: USER_ID_TYPE,
    /// Pretty name for the client
    own_identifier: String,
    /// Flags denoting the user's capabilities. These will consist of some known strings to denote things like mocap availability, microphone availability, etc.
    user_flags: Vec<String>,
    /// Port made available for receiving backing track data.
    backing_track_port: u16,
    /// Port made available for receiving future server events after handshake completion.
    server_event_port: u16,
    /// Port made available for receiving audience motion capture data.
    audience_motion_capture_port: u16,
    /// Additional, named ports that the client wishes to let the server know it is making available for data receipt.
    extra_ports: HashMap<String, String>,
}

/// Last message of handshake, indicating that the server is okay and things are ready to go.
#[derive(Serialize, Deserialize, Debug)]
struct HandshakeCompletion {
    /// Ports for different services that should be made 
    extra_ports: HashMap<String, String>
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