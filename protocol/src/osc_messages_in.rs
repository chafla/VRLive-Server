// These pertain to messages that we will be receiving from clients,
// and should be prefixed with /client

use rosc::{OscMessage, OscType};

use crate::{OSCDecodable, OSCEncodable};

#[allow(dead_code)]

#[derive(PartialEq, Debug)]
pub enum ClientMessage {
    Any,
    Audience,
    Performer(PerformerClientMessage)
}

impl OSCEncodable for ClientMessage {
    fn base_prefix() -> String {
        "/client".to_owned()
    }

    fn variant_prefix(&self) -> String {
        match self {
            Self::Any => "any",
            Self::Audience => "audience",
            Self::Performer(_) => "performer",
        }.to_owned()
    }


    fn to_message(&self, addr: Vec<String>) -> OscMessage {
        match self {
            Self::Performer(pcm) => pcm.to_message(addr),
            Self::Audience => todo!(),
            Self::Any => todo!(),
        }
    }

}

impl OSCDecodable for ClientMessage {
    fn deconstruct_osc(working_str: &str, message: &OscMessage) -> Option<Self>
    {
        // TODO add testing to ensure that the deconstructed prefix matches the constructed prefix
        if let Some((start, rest)) = working_str.split_once('/') {
            match start {
                "any" => todo!(),
                "audience" => todo!(),
                "performer" => Some(Self::Performer(PerformerClientMessage::deconstruct_osc(rest, message)?)),
                _ => None,
            }
        }
        else {
            None
        }
    }
}

#[derive(PartialEq, Debug, Copy, Clone)]
pub enum PerformerToggle {
    Audio(bool),
    Actor(bool),
    Motion(bool),
}

impl OSCEncodable for PerformerToggle {
    fn base_prefix() -> String {
        "toggle".to_owned()
    }

    fn variant_prefix(&self) -> String {
        match self {
            Self::Audio(_) => "audio",
            Self::Actor(_) => "actor",
            Self::Motion(_) => "motion",
        }.to_owned()
    }

    fn to_message(&self, mut existing_prefix: Vec<String>) -> OscMessage {
        existing_prefix.push(Self::base_prefix());
        existing_prefix.push(self.variant_prefix());
        let val = match self {
            Self::Audio(b) | Self::Actor(b) | Self::Motion(b) => *b
        };

        OscMessage {
            addr: existing_prefix.join("/"),
            args: vec![OscType::Bool(val)]
        }
    }
}

impl OSCDecodable for PerformerToggle {
    fn deconstruct_osc(prefix: &str, message: &OscMessage) -> Option<Self> {
        if let Some((start, _)) = prefix.split_once('/') {
            // if there's a split, the character was in the string
            if message.args.len() != 1 {
                None
            }
            else if let OscType::Bool(b) = message.args[0] {
                match start {
                    "audio" => Some(Self::Audio(b)),
                    "actor" => Some(Self::Actor(b)),
                    "motion" => Some(Self::Motion(b)),
                    _ => None
                }
            }
            else {
                None
            }
        }
        else {
            None
        }
    }
}

#[derive(PartialEq, Debug)]
pub enum PerformerClientMessage {
    Ready(bool),
    Toggle(PerformerToggle)
}



impl OSCEncodable for PerformerClientMessage {
    fn base_prefix() -> String {
        "performer".to_owned()
    }

    fn variant_prefix(&self) -> String {
        match self {
            Self::Ready(_) => "ready",
            Self::Toggle(_) => "toggle",
        }.to_string()
    }

    fn to_message(&self, mut addr: Vec<String>) -> OscMessage {
        addr.push(Self::base_prefix());
        addr.push(self.variant_prefix());

        match self {
            Self::Ready(b) => OscMessage {
                addr: addr.join("/"),
                args: vec![OscType::Bool(*b)]
            },
            Self::Toggle(pt) => pt.to_message(addr)
        }
    }
}

impl OSCDecodable for PerformerClientMessage {
    fn deconstruct_osc(prefix: &str, message: &OscMessage) -> Option<Self> {

        if let Some((start, rest)) = prefix.split_once('/') {
            match start {
                "toggle" => Some(Self::Toggle(PerformerToggle::deconstruct_osc(rest, message)?)),
                _ => None
            }
        }
        else {
            match prefix {
                "ready" => {
                    if let OscType::Bool(b) = message.args[0] {
                        Some(Self::Ready(b))
                    }
                    else {
                        None
                    }
                },
                _ => None
            }
        }


    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn try_create_client_message() {
        let message = ClientMessage::Performer(PerformerClientMessage::Ready(true));

        let encoded = message.to_message(vec!["".to_owned()]);
        let unencoded = ClientMessage::from_osc_message(&encoded);
        dbg!(&unencoded);
        if let Some(server_msg) = &unencoded {
            let reencoded = server_msg.to_message(vec!["".to_owned()]);
            let reunencoded = ClientMessage::from_osc_message(&reencoded);
            assert_eq!(reencoded.addr, encoded.addr);
            assert_eq!(&reunencoded, &unencoded);
        }
        else {
            panic!("Object was not re-encoded")
        }
    }
}