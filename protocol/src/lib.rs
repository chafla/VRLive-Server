pub mod osc_messages_in;
pub mod vrl_packet;
pub mod osc_messages_out;

use tokio::sync::mpsc;

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
    participant_id: u16, // you better not have this many people
    remote_port: u16,
    local_port: u16,
    remote_ip_addr: u32,


}