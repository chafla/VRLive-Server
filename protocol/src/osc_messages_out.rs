// these pertain to messages that we are sending out from the server
// and will be prefixed /server

use crate::{osc_messages_in::PerformerToggle, OSCDecodable, OSCEncodable};
use rosc::{OscMessage, OscType};


pub enum ServerMessage {
    Scene(SceneMessage),
    Timing,  // TODO
    Performer,
    Backing(BackingMessage)
}

impl OSCEncodable for ServerMessage {
    fn base_prefix() -> String {
        "server".to_owned()
    }

    fn to_message(&self, mut existing_prefix: Vec<String>) -> rosc::OscMessage {
        existing_prefix.push(Self::base_prefix());
        match self {
            Self::Scene(msg) => msg.to_message(existing_prefix),
            Self::Timing => todo!(),
            Self::Performer => todo!(),
            Self::Backing(bm) => todo!()

        }
    }

    fn variant_prefix(&self) -> String {
        match self {
            Self::Scene(_) => "scene",
            Self::Timing => "timing",
            Self::Performer => "performer",
            Self::Backing(_) => "backing"
        }.to_owned()
    }
}

impl OSCDecodable for ServerMessage {
    fn deconstruct_osc(prefix: &str, message: &OscMessage) -> Option<Self> {
        if let Some((start, rest)) = prefix.split_once("/") {
            match start {
                "scene" => Some(Self::Scene(SceneMessage::deconstruct_osc(rest, message)?)),
                _ => None,
            }
        }
        else {
            match prefix {
                "performer" | "timing" | "backing" => todo!(),
                _ => None
            }
        }
    }
}

/// Messages relating to the scene itself
pub enum SceneMessage {
    State(i32)
}

impl OSCEncodable for SceneMessage {
    fn base_prefix() -> String {
        "scene".to_owned()
    }

    fn to_message(&self, mut existing_prefix: Vec<String>) -> rosc::OscMessage {
        existing_prefix.push(Self::base_prefix());
        existing_prefix.push(self.variant_prefix());
        let pfx = existing_prefix.join("/");
        match self {
            Self::State(state_num) => rosc::OscMessage { addr: pfx, args: vec![OscType::Int(*state_num)] }
        }
    }

    fn variant_prefix(&self) -> String {
        match self {
            Self::State(_) => "state"
        }.to_owned()
    }
}

impl OSCDecodable for SceneMessage {
    fn deconstruct_osc(prefix: &str, message: &OscMessage) -> Option<Self> {
        if let Some((_)) = prefix.split_once("/") {
            None
        }
        else {
            match prefix {
                "state" => match message.args[0] {
                    OscType::Int(i) => Some(Self::State(i)),
                    _ => None
                },
                _ => None
            }
        }
    }
}

pub enum BackingMessage {
    Stop,
    /// At which timestamp should we start? If negative, start from the beginning.
    Start(f32),
    /// Load a new backing track with the given descriptor
    New(String),
}

impl OSCEncodable for BackingMessage {
    fn base_prefix() -> String {
        "backing".to_owned()
    }

    fn to_message(&self, mut existing_prefix: Vec<String>) -> rosc::OscMessage {
        existing_prefix.push(Self::base_prefix());
        existing_prefix.push(self.variant_prefix());
        let pfx = existing_prefix.join("/");
        match self {
            Self::Start(ts) => rosc::OscMessage { addr: pfx, args: vec![OscType::Float(*ts)] },
            Self::Stop => rosc::OscMessage {addr: pfx, args: vec![]},
            Self::New(new_song) => rosc::OscMessage {addr: pfx, args: vec![OscType::String(new_song.clone())]}
        }
    }

    fn variant_prefix(&self) -> String {
        match self {
            Self::Start(_) => "start",
            Self::Stop => "stop",
            Self::New(_) => "new"
        }.to_owned()
    }
}

impl OSCDecodable for BackingMessage {
    fn deconstruct_osc(prefix: &str, message: &OscMessage) -> Option<Self> {
        if let Some(_) = prefix.split_once("/") {
            None
        }
        else {
            match prefix {
                "stop" => Some(Self::Stop),
                "start" | "new" => {
                    if message.args.len() > 0 {
                        if let OscType::Float(f) = message.args[0] {
                            return Some(Self::Start(f))
                        }
                        if let OscType::String(str) = &message.args[0] {
                            return Some(Self::New(str.clone()))
                        }
                    }
                    None
                }
                _ => None
            }
        }

    }
}

/// distinctly different from the one in messages_in as this one has matchmaking
/// also represents the messages being sent to the performer from the server.
/// still kind of a TODO item

pub enum PerformerClientMessage {
    Ready(bool),
    Toggle(PerformerToggle),
    MatchMake(MatchMakeMessage)
}



impl OSCEncodable for PerformerClientMessage {
    fn base_prefix() -> String {
        "performer".to_owned()
    }

    fn variant_prefix(&self) -> String {
        match self {
            Self::MatchMake(_) => "matchmake",
            Self::Ready(_) => "ready",
            Self::Toggle(_) => "toggle"
        }.to_owned()
    }

    fn to_message(&self, mut addr: Vec<String>) -> rosc::OscMessage {
        addr.push(Self::base_prefix());
        addr.push(self.variant_prefix());
        let pfx = addr.join("/");

        match self {
            Self::Ready(b) => rosc::OscMessage {
                addr: pfx,
                args: vec![OscType::Bool(*b)]
            },
            Self::Toggle(pt) => pt.to_message(addr),
            Self::MatchMake(mm) => mm.to_message(addr),
        }
    }
}

impl OSCDecodable for PerformerClientMessage {
    fn deconstruct_osc(prefix: &str, message: &OscMessage) -> Option<Self> {
        if let Some((start, rest)) = prefix.split_once("/") {
            match start {
                "matchmake" => Some(Self::MatchMake(MatchMakeMessage::deconstruct_osc(rest, message)?)),
                "toggle" => Some(Self::Toggle(PerformerToggle::deconstruct_osc(rest, message)?)),
                _ => None
            }
        }
        else {
            if message.args.len() != 1 {
                return None
            }
            match (prefix, &message.args[0]) {
                ("ready", OscType::Bool(b)) => Some(Self::Ready(*b)),
                _ => None
            }
        }
    }
}


pub enum MatchMakeMessage {
    Request
}

impl OSCEncodable for MatchMakeMessage {
    fn base_prefix() -> String {
        "matchmake".to_owned()
    }

    fn variant_prefix(&self) -> String {
        match self {
            Self::Request => "request",
        }.to_owned()
    }

    fn to_message(&self, mut existing_prefix: Vec<String>) -> rosc::OscMessage {
        existing_prefix.push(Self::base_prefix());
        let pfx = existing_prefix.join("/");

        match self {
            Self::Request => rosc::OscMessage {addr: pfx + "/request", args: vec![]}
        }
    }
}

impl OSCDecodable for MatchMakeMessage {
    fn deconstruct_osc(prefix: &str, message: &OscMessage) -> Option<Self> {

        if let Some(_) = message.addr.split_once("/") {
            return None
        }

        match prefix {
            "request" => Some(Self::Request),
            _ => None
        }

    }
}
