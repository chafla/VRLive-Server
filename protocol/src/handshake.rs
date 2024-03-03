/**
 * Basic protocol for the main client/server handshake initialized on connection.
 * Once this has been completed, the client/server is moved into the ready state.
 */

use std::collections::HashMap;
use log::warn;

use serde::{Deserialize, Serialize};

use crate::UserIDType;


/// Initial message sent in response to a new connection.
#[derive(Serialize, Deserialize, Debug)]
pub struct HandshakeAck {
    /// Informing of their user ID
    user_id: UserIDType,
    /// Pretty name for the server.
    server_pretty_identifier: String,
    ports: ServerPortMap
}

impl HandshakeAck {
    pub fn new(user_id: UserIDType, server_pretty_identifier: String, server_ports: Option<ServerPortMap>) -> Self {
        Self {
            user_id,
            server_pretty_identifier,
            ports: server_ports.unwrap_or(ServerPortMap::default())
        }

    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ClientPortMap {
    /// Port made available for receiving backing track data.
    pub backing_track: u16,
    /// Port made available for receiving future server events after handshake completion.
    pub server_event: u16,
    /// Port made available for receiving performer vrtp data
    pub vrtp_data: u16,
    /// Port made available for receiving audience motion capture data.
    pub audience_motion_capture: u16,
    /// Additional, named ports that the client wishes to let the server know it is making available for data receipt.
    pub extra_ports: HashMap<String, u16>,
}

/// Data structure representing the raw JSON response we expect to receive.
/// If a message doesn't serialize to this, it's invalid
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
    pub ports: ClientPortMap,
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


#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
pub struct ServerPortMap {
    /// Port through which we accept new connections (TCP)
    pub new_connections: u16,
    /// Port through which we accept audience mocap data (UDP)
    pub audience_mocap_in: u16,
    /// Port through which we accept performer mocap data (UDP)
    pub performer_mocap_in: u16,
    /// Port for performer audio (UDP)
    pub performer_audio_in: u16,
    /// Port for taking in client events (TCP)
    pub client_event_conn_port: u16,
    /// Port for taking in server events (TCP).
    pub server_event_conn_port: u16,
    /// Port for the backing track socket (TCP)
    pub backing_track_conn_port: u16,
}



impl Default for ServerPortMap {
    fn default() -> Self {
        warn!("Using default port map!");
        Self {
            new_connections: 5653,
            performer_mocap_in: 5654,
            performer_audio_in: 5655,
            client_event_conn_port: 5656,
            backing_track_conn_port: 5657,
            server_event_conn_port: 5658,
            audience_mocap_in: 9000,
        }
    }
}