use webrtc::rtp::packet::Packet;

pub mod server;
pub mod client;
mod analytics;
mod performance;


/// Maximum buffer size for the channel before we start to block
const MAX_CHAN_SIZE: usize = 2 << 12;

type AudioPacket = Packet;  // TODO
