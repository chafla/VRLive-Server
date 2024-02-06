// These pertain to messages that we will be receiving from clients,
// and should be prefixed with /client

use crate::VRLUser;
use rosc::OscBundle;
use crate::osc_messages_out::PerformerToggle;
use crate::vrl_packet::RawVRLOSCPacket;



pub enum ClientMessage {
    Any,
    Audience,
    Performer
}

pub enum PerformerClientMessage {
    Ready(bool),
    Toggle(PerformerToggle)
}