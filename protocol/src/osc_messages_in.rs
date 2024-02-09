// These pertain to messages that we will be receiving from clients,
// and should be prefixed with /client

use crate::{OSCEncodable, VRLUser};
use rosc::decoder::{decode_tcp, decode_udp};
use rosc::{decoder, OscBundle, OscMessage, OscPacket, OscType};
use crate::vrl_packet::RawVRLOSCPacket;


pub enum ClientMessage {
    Any,
    Audience,
    Performer(PerformerClientMessage)
}

impl OSCEncodable for ClientMessage {
    fn get_prefix() -> String {
        "client".to_owned()
    }

    fn to_message(&self, addr: Vec<String>) -> rosc::OscMessage {
        match self {
            Self::Performer(pcm) => pcm.to_message(addr),
            Self::Audience => unimplemented!(),
            Self::Any => unimplemented!(),
            _ => todo!()
        }
    }
}

pub enum PerformerToggle {
    Audio(bool),
    Actor(bool),
    Motion(bool),
}

impl OSCEncodable for PerformerToggle {
    fn get_prefix() -> String {
        "toggle".to_owned()
    }

    fn to_message(&self, mut existing_prefix: Vec<String>) -> OscMessage {
        existing_prefix.push(Self::get_prefix());
        let (prefix, val) = match self {
            Self::Audio(b) => ("audio", *b),
            Self::Actor(b) => ("actor", *b),
            Self::Motion(b) => ("motion", *b),
        };

        // this to_owned feels unnecessary
        existing_prefix.push(prefix.to_owned());

        return rosc::OscMessage {
            addr: existing_prefix.join("/"),
            args: vec![OscType::Bool(val)]
        }
    }
}

pub enum PerformerClientMessage {
    Ready(bool),
    Toggle(PerformerToggle)
}



impl OSCEncodable for PerformerClientMessage {
    fn get_prefix() -> String {
        "performer".to_owned()
    }

    fn to_message(&self, mut addr: Vec<String>) -> rosc::OscMessage {
        let pfx = Self::get_prefix();
        addr.push(pfx);

        match self {
            Self::Ready(b) => rosc::OscMessage {
                addr: addr.join("/") + "/ready",
                args: vec![OscType::Bool(*b)]
            },
            Self::Toggle(pt) => pt.to_message(addr)
        }
    }
}