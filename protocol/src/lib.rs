use std::io::Read;
use std::mem::size_of;
use std::net::IpAddr;
use std::str::from_utf8;
use bytes::{Buf, BufMut, Bytes, BytesMut};

use rosc::OscMessage;
use serde::{Deserialize, Serialize};
use tokio::io::AsyncReadExt;

pub mod osc_messages_in;
pub mod vrl_packet;
pub mod osc_messages_out;
pub mod handshake;
pub mod backing_track;
pub mod heartbeat;
pub mod synchronizer;
mod vrm_packet;

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

/// Channels to assign to all

pub struct PerformerChannels {
    
}

pub struct AudienceChannels {

}

/// The standardized packet format we use for sending data out to our listening clients.
pub struct VRLTCPPacket {
    /// Length of our message type
    header_msg_len: u8,
    /// String message marking our message type.
    header_msg: String,
    /// Length of the header (not including the lengths and header message) in bytes.
    /// You can store whatever information you want in here, just make sure it's accounted for.
    header_len: u16,
    /// Length of the body payload in bytes.
    body_len: u32,
    /// Header, containing any information you want to stash in here.
    /// Note that in the message this will be offset after header_msg, header_len, and body_len.
    header: Bytes,
    /// Payload
    body: Bytes
}

impl VRLTCPPacket {
    pub fn new(header_msg: &str, header: Bytes, body: Bytes) -> Self {
        Self {
            header_msg_len: header_msg.len() as u8,
            header_msg: header_msg.into(),
            header_len: header.len() as u16,
            header,
            body_len: body.len() as u32,
            body
        }
    }
}

impl From<Bytes> for VRLTCPPacket {
    fn from(mut value: Bytes) -> Self {
        let message_type_len = value.get_u8();
        let title = &value[0..message_type_len as usize];
        let title = from_utf8(title).unwrap().to_owned();
        let header_len = value.get_u16();
        let body_len = value.get_u32();
        // header offset doesn't include body/header sizes
        let header_offset = message_type_len as usize + 1 + 2 + 4;
        let body_offset = header_offset + header_len as usize;
        let header = &value[header_offset..body_offset];
        // let header = Bytes::from(header);
        let body = &value[body_offset..body_offset + body_len as usize];

        Self {
            header_msg_len: message_type_len,
            header_msg: title,
            header_len,
            body_len,
            header: Bytes::copy_from_slice(header),
            body: Bytes::copy_from_slice(body)
        }
    }
}

impl From<VRLTCPPacket> for Bytes {
    fn from(value: VRLTCPPacket) -> Self {
        let mut out_bytes = BytesMut::with_capacity(value.body_len as usize + value.header_len as usize + value.header_msg_len as usize + 7);
        out_bytes.put_u8(value.header_msg_len);
        out_bytes.put(value.header_msg.as_bytes());
        out_bytes.put_u16(value.header_len);
        out_bytes.put_u32(value.body_len);
        out_bytes.put(value.header);
        out_bytes.put(value.body);

        return out_bytes.freeze();
    }
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
        // trim off the leading slash
        dbg!(message);
        Self::deconstruct_osc(&message.addr[offset..], message)
    }

    /// Step through an OSC message
    fn deconstruct_osc(prefix: &str, message: &OscMessage) -> Option<Self>;
}