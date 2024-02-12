pub mod osc_messages_in;
pub mod vrl_packet;
pub mod osc_messages_out;
pub mod handshake;

use tokio::sync::mpsc;
use rosc::{OscMessage, OscType};

type USER_ID_TYPE = u16;

/// The source (or destination) for a message.
/// The int associated with the user represents their user ID,
/// which should be unique per IP address.
/// 
/// If the associated value is Some, then it's assumed that it relates to a specific user.
/// If it's None, then it's assumed that it pertains to all targets, and should be multicasted.
#[derive(Clone, Copy)]
pub enum VRLUser {
    Performer(Option<UserData>),
    Server,
    Audience(Option<UserData>),
}

/// Channels to assign to all

pub struct PerformerChannels {
    
}

pub struct AudienceChannels {

}


#[derive(Clone, Copy)]
pub struct UserData {
    participant_id: USER_ID_TYPE, // you better not have this many people
    remote_port: u16,
    local_port: u16,
    remote_ip_addr: u32,
}


/// A message with this trait can be easily encoded to OSC.
pub trait OSCEncodable {
    /// Get the prefix for this OSC struct.
    fn base_prefix() -> String;

    /// Get the prefix for a specific instantiation of this struct.
    fn variant_prefix(&self) -> String;

    /// Create a message from a variant of this value.
    fn to_message(&self, existing_prefix: Vec<String>) -> OscMessage;
}

/// A message with this trait can, similarly, be easily decoded from OSC.
pub trait OSCDecodable : Sized {

    /// Decode from an OSC message. This is probably the method that you want to call.
    fn from_osc_message(message: &OscMessage) -> Option<Self> {
        let offset = if message.addr.chars().next()? == '/' {1} else {0};
        // trim off the leading slash
        dbg!(message);
        return Self::deconstruct_osc(&message.addr[offset..], message);
    }

    /// Step through an OSC message
    fn deconstruct_osc(prefix: &str, message: &OscMessage) -> Option<Self>;
}