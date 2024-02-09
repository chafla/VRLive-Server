// these pertain to messages that we are sending out from the server
// and will be prefixed /server

use crate::{osc_messages_in::PerformerToggle, OSCEncodable};
use rosc::OscType;


pub enum ServerMessage {
    Scene(SceneMessage),
    Timing,  // TODO
    Performer,
    Backing(BackingMessage)
}

impl OSCEncodable for ServerMessage {
    fn get_prefix() -> String {
        "server".to_owned()
    }

    fn to_message(&self, mut existing_prefix: Vec<String>) -> rosc::OscMessage {
        existing_prefix.push(Self::get_prefix());
        match self {
            Self::Scene(msg) => msg.to_message(existing_prefix),
            Self::Timing => todo!(),
            Self::Performer => todo!(),
            Self::Backing(bm) => todo!()

        }
    }
}

/// Messages relating to the scene itself
pub enum SceneMessage {
    State(i32)
}

impl OSCEncodable for SceneMessage {
    fn get_prefix() -> String {
        "scene".to_owned()
    }

    fn to_message(&self, mut existing_prefix: Vec<String>) -> rosc::OscMessage {
        existing_prefix.push(Self::get_prefix());
        let pfx = existing_prefix.join("/");
        match self {
            Self::State(state_num) => rosc::OscMessage { addr: pfx + "/scene", args: vec![OscType::Int(*state_num)] }
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
    fn get_prefix() -> String {
        "backing".to_owned()
    }

    fn to_message(&self, mut existing_prefix: Vec<String>) -> rosc::OscMessage {
        existing_prefix.push(Self::get_prefix());
        let pfx = existing_prefix.join("/");
        match self {
            Self::Start(ts) => rosc::OscMessage { addr: pfx + "/start", args: vec![OscType::Float(*ts)] },
            Self::Stop => rosc::OscMessage {addr: pfx + "/stop", args: vec![]},
            Self::New(new_song) => rosc::OscMessage {addr: pfx + "/new", args: vec![OscType::String(new_song.clone())]}
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
    fn get_prefix() -> String {
        "performer".to_owned()
    }

    fn to_message(&self, mut addr: Vec<String>) -> rosc::OscMessage {
        addr.push(Self::get_prefix());
        let pfx = addr.join("/");

        match self {
            Self::Ready(b) => rosc::OscMessage {
                addr: pfx + "/ready",
                args: vec![OscType::Bool(*b)]
            },
            Self::Toggle(pt) => pt.to_message(addr),
            Self::MatchMake(mm) => mm.to_message(addr),
        }
    }
}


pub enum MatchMakeMessage {
    Request
}

impl OSCEncodable for MatchMakeMessage {
    fn get_prefix() -> String {
        "matchmake".to_owned()
    }

    fn to_message(&self, mut existing_prefix: Vec<String>) -> rosc::OscMessage {
        existing_prefix.push(Self::get_prefix());
        let pfx = existing_prefix.join("/");

        match self {
            Self::Request => rosc::OscMessage {addr: pfx + "/request", args: vec![]}
        }
    }
}
