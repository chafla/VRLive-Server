use std::net::IpAddr;

use rosc::OscMessage;
use serde::{Deserialize, Serialize};

pub mod osc_messages_in;
pub mod vrl_packet;
pub mod osc_messages_out;
pub mod handshake;
pub mod backing_track;
pub mod heartbeat;
pub mod synchronizer;
mod vrm_packet;
pub mod vrl_tcp_packet;

pub type UserIDType = u16;


#[derive(Serialize, Deserialize, Debug, Copy, Clone, Eq, PartialEq)]
pub enum UserType {
    Audience,
    Performer
}

impl From<i32> for UserType {
    fn from(value: i32) -> Self {
        match value {
            1 => UserType::Audience,
            2 => UserType::Performer,
            _ => unimplemented!()
        }
    }
}

impl From<u16> for UserType {
    fn from(value: u16) -> Self {
        match value {
            1 => UserType::Audience,
            2 => UserType::Performer,
            _ => unimplemented!()
        }
    }
}

impl From<UserType> for u16 {
    fn from(value: UserType) -> Self {
        match value {
            UserType::Audience => 1,
            UserType::Performer => 2,
        }
    }
}

impl From<UserType> for i32 {
    fn from(value: UserType) -> Self {
        match value {
            UserType::Audience => 1,
            UserType::Performer => 2,
        }
    }
}

impl From<&i32> for UserType {
    fn from(value: &i32) -> Self {
        match value {
            1 => UserType::Audience,
            2 => UserType::Performer,
            _ => unimplemented!()
        }
    }
}


/// The source (or destination) for a message.
/// The int associated with the user represents their user ID,
/// which should be unique per IP address.
/// 
/// If the associated value is Some, then it's assumed that it relates to a specific user.
/// If it's None, then it's assumed that it pertains to all targets, and should be multicasted.
#[derive(Clone)]
pub enum VRLUser {
    Performer(Option<UserData>),
    Server,
    Audience(Option<UserData>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UserData {
    pub participant_id: UserIDType, // you better not have this many people
    pub fancy_title: String,
    pub remote_ip_addr: IpAddr,
}


/// A message with this trait can be easily encoded to OSC.
pub trait OSCEncodable {
    /// Get the prefix for this OSC struct.
    fn base_prefix() -> String;

    /// Get the prefix for a specific instantiation of this struct.
    fn variant_prefix(&self) -> String;

    /// Create a message from a variant of this value.
    fn to_message(&self, existing_prefix: Vec<String>) -> OscMessage;

    /// Convert a server message to an OSC message.
    fn encode(&self) -> OscMessage {
        self.to_message(vec![])
    }
}

/// A message with this trait can, similarly, be easily decoded from OSC.
pub trait OSCDecodable : Sized {

    /// Decode from an OSC message. This is probably the method that you want to call.
    fn from_osc_message(message: &OscMessage) -> Option<Self> {
        let offset = if message.addr.chars().next()? == '/' {1} else {0};
        // trim off the leading slash, makes parsing easier
        Self::deconstruct_osc(&message.addr[offset..], message)
    }

    /// Step through an OSC message
    fn deconstruct_osc(prefix: &str, message: &OscMessage) -> Option<Self>;
}