use crate::VRLUser;
use rosc::OscBundle;



pub enum ClientMessage {
    Audience,
    Performer
}

pub enum ServerMessage {
    Scene,
    Timing,
    Performer
}

/// Basic representation for a bundle we'll be handling.
pub struct VRLOSCPacket {
    source: VRLUser,
    destination: VRLUser,
    payload: OscBundle,
}

