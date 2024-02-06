pub mod osc_messages_in;
pub mod vrl_packet;
mod osc_messages_out;

/// The source (or destination) for a message.
/// The int associated with the user represents their user ID,
/// which should be unique per IP address.
/// 
/// If the associated value is Some, then it's assumed that it relates to a specific user.
/// If it's None, then it's assumed that it pertains to all targets, and should be multicasted.
pub enum VRLUser {
    Performer(Option<UserData>),
    Server,
    Audience(Option<UserData>),
}


pub struct UserData {
    fancy_name: String,
    remote_port: u16,
    local_port: u16,
    remote_ip_addr: u32,
}