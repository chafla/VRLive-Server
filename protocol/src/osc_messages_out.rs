// these pertain to messages that we are sending out from the server
// and will be prefixed /server

use log::error;
use rosc::{OscMessage, OscType};

use crate::{osc_messages_in::PerformerToggle, OSCDecodable, OSCEncodable, UserIDType, UserType};

#[derive(PartialEq, Debug, Clone)]
pub enum ServerMessage {
    Scene(SceneMessage),
    Timing,  // TODO
    Performer(PerformerServerMessage),
    Backing(BackingMessage),
    Status(StatusMessage)
}

impl OSCEncodable for ServerMessage {
    // add a starting slash for consistency
    fn base_prefix() -> String {
        "/server".to_owned()
    }

    fn variant_prefix(&self) -> String {
        match self {
            Self::Scene(_) => "scene",
            Self::Timing => "timing",
            Self::Performer(_) => "performer",
            Self::Backing(_) => "backing",
            Self::Status(_) => "status"
        }.to_owned()
    }

    fn to_message(&self, mut existing_prefix: Vec<String>) -> OscMessage {
        existing_prefix.push(Self::base_prefix());
        match self {
            Self::Scene(msg) => msg.to_message(existing_prefix),
            Self::Timing => todo!(),
            Self::Performer(psm) => psm.to_message(existing_prefix),
            Self::Backing(msg) => msg.to_message(existing_prefix),
            Self::Status(s) => s.to_message(existing_prefix),

        }
    }
}

impl OSCDecodable for ServerMessage {
    fn deconstruct_osc(prefix: &str, message: &OscMessage) -> Option<Self> {
        dbg!(&prefix);
        // this one is special, since it starts the hierarchy
        // TODO come up with a better way to do this
        let trimmed_prefix = if let Some((start, rest)) = prefix.split_once('/') {
            if start != "server" {
                error!("Server was given an invalid string.");

                return None
            }

            Some(rest)
        }
        else {
            None
        };


        if let Some((start, rest)) = trimmed_prefix?.split_once('/') {
            match start {
                "scene" => Some(Self::Scene(SceneMessage::deconstruct_osc(rest, message)?)),
                "performer" => Some(Self::Performer(PerformerServerMessage::deconstruct_osc(rest, message)?)),
                _ => None,
            }
        }
        else {
            match prefix {
                "timing" | "backing" => todo!(),
                _ => None
            }
        }
    }
}

/// Messages relating to the scene itself
#[derive(PartialEq, Debug, Copy, Clone)]
pub enum SceneMessage {
    State(i32)
}

impl OSCEncodable for SceneMessage {
    fn base_prefix() -> String {
        "scene".to_owned()
    }

    fn variant_prefix(&self) -> String {
        match self {
            Self::State(_) => "state"
        }.to_owned()
    }

    fn to_message(&self, mut existing_prefix: Vec<String>) -> OscMessage {
        existing_prefix.push(Self::base_prefix());
        existing_prefix.push(self.variant_prefix());
        let pfx = existing_prefix.join("/");
        match self {
            Self::State(state_num) => OscMessage { addr: pfx, args: vec![OscType::Int(*state_num)] }
        }
    }
}

impl OSCDecodable for SceneMessage {
    fn deconstruct_osc(prefix: &str, message: &OscMessage) -> Option<Self> {
        if prefix.split_once('/').is_some() {
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

#[derive(PartialEq, Debug, Clone)]
pub enum StatusMessage {
    UserAdd(UserIDType, UserType),
    UserRemove(UserIDType, UserType),
    UserReconnect(UserIDType, UserType),
}

impl OSCEncodable for StatusMessage {
    fn base_prefix() -> String {
        "status".to_owned()
    }

    fn variant_prefix(&self) -> String {
        match self {
            // TODO add another level of hierarchy here, this is kind of lazy
            Self::UserAdd(_, _) => "useradd",
            Self::UserRemove(_, _) => "userremove",
            Self::UserReconnect(_, _) => "userreconnect"
        }.to_owned()
    }

    fn to_message(&self, mut existing_prefix: Vec<String>) -> OscMessage {
        existing_prefix.push(Self::base_prefix());
        existing_prefix.push(self.variant_prefix());
        let pfx = existing_prefix.join("/");
        match self {
            Self::UserAdd(id, ut) => OscMessage { addr: pfx, args: vec![OscType::Int(*id as i32), OscType::Int((*ut).into())] },
            Self::UserRemove(id, ut) => OscMessage { addr: pfx, args: vec![OscType::Int(*id as i32), OscType::Int((*ut).into())]},
            Self::UserReconnect(id, ut) => OscMessage { addr: pfx, args: vec![OscType::Int(*id as i32), OscType::Int((*ut).into())]},
        }
    }
}

impl OSCDecodable for StatusMessage {
    fn deconstruct_osc(prefix: &str, message: &OscMessage) -> Option<Self> {
        if prefix.split_once('/').is_some() {
            None
        }
        else {
            if message.args.len() != 2  {
                return None;
            }
            match (prefix, &message.args[0], &message.args[1]) {
                // "useradd" => Some(Self::Stop),
                ("useradd", OscType::Int(id), OscType::Int(ut))  => {
                    return Some(Self::UserAdd(*id as u16, UserType::from(*ut)))
                },
                ("userremove", OscType::Int(i), OscType::Int(ut)) => {
                    return Some(Self::UserRemove(*i as u16, UserType::from(*ut)))
                },
                ("userreconnect", OscType::Int(i), OscType::Int(ut)) => {
                    return Some(Self::UserReconnect(*i as u16, UserType::from(*ut)))
                }
                _ => None
            }
        }

    }
}

#[derive(PartialEq, Debug, Clone)]
pub enum BackingMessage {
    Stop,
    /// How many samples in should the sound start?
    Start(i32),
    /// Load a new backing track with the given descriptor
    New(String),
}

impl OSCEncodable for BackingMessage {
    fn base_prefix() -> String {
        "backing".to_owned()
    }

    fn variant_prefix(&self) -> String {
        match self {
            Self::Start(_) => "start",
            Self::Stop => "stop",
            Self::New(_) => "new"
        }.to_owned()
    }

    fn to_message(&self, mut existing_prefix: Vec<String>) -> OscMessage {
        existing_prefix.push(Self::base_prefix());
        existing_prefix.push(self.variant_prefix());
        let pfx = existing_prefix.join("/");
        match self {
            Self::Start(ts) => OscMessage { addr: pfx, args: vec![OscType::Int(*ts)] },
            Self::Stop => OscMessage {addr: pfx, args: vec![]},
            Self::New(new_song) => OscMessage {addr: pfx, args: vec![OscType::String(new_song.clone())]}
        }
    }
}

impl OSCDecodable for BackingMessage {
    fn deconstruct_osc(prefix: &str, message: &OscMessage) -> Option<Self> {
        if prefix.split_once('/').is_some() {
            None
        }
        else {
            match prefix {
                "stop" => Some(Self::Stop),
                "start" | "new" => {
                    if !message.args.is_empty() {
                        if let OscType::Int(i) = message.args[0] {
                            return Some(Self::Start(i))
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

#[derive(PartialEq, Debug, Clone)]
pub enum PerformerServerMessage {
    Ready(bool),
    Toggle(PerformerToggle),
    MatchMake(MatchMakeMessage)
}



impl OSCEncodable for PerformerServerMessage {
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

    fn to_message(&self, mut addr: Vec<String>) -> OscMessage {
        addr.push(Self::base_prefix());
        addr.push(self.variant_prefix());
        let pfx = addr.join("/");

        match self {
            Self::Ready(b) => OscMessage {
                addr: pfx,
                args: vec![OscType::Bool(*b)]
            },
            Self::Toggle(pt) => pt.to_message(addr),
            Self::MatchMake(mm) => mm.to_message(addr),
        }
    }
}

impl OSCDecodable for PerformerServerMessage {
    fn deconstruct_osc(prefix: &str, message: &OscMessage) -> Option<Self> {
        if let Some((start, rest)) = prefix.split_once('/') {
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


#[derive(PartialEq, Debug, Clone)]
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

    fn to_message(&self, mut existing_prefix: Vec<String>) -> OscMessage {
        existing_prefix.push(Self::base_prefix());
        let pfx = existing_prefix.join("/");

        match self {
            Self::Request => OscMessage {addr: pfx + "/request", args: vec![]}
        }
    }
}

impl OSCDecodable for MatchMakeMessage {
    fn deconstruct_osc(prefix: &str, message: &OscMessage) -> Option<Self> {

        if message.addr.split_once('/').is_some() {
            return None
        }

        match prefix {
            "request" => Some(Self::Request),
            _ => None
        }

    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn try_create_server_message() {
        let message = ServerMessage::Performer(PerformerServerMessage::Ready(true));

        let encoded = message.to_message(vec!["".to_owned()]);
        let unencoded = ServerMessage::from_osc_message(&encoded);
        dbg!(&unencoded);
        if let Some(server_msg) = &unencoded {
            let reencoded = server_msg.to_message(vec!["".to_owned()]);
            let reunencoded = ServerMessage::from_osc_message(&reencoded);
            assert_eq!(reencoded.addr, encoded.addr);
            assert_eq!(&reunencoded, &unencoded);
        }
        else {
            panic!("Object was not re-encoded")
        }
    }
}