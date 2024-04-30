use webrtc::rtp::packet::Packet;
use protocol::osc_messages_in::ClientMessage;
use protocol::UserIDType;

pub mod server;
pub mod client;
mod analytics;
mod performance;


/// Maximum buffer size for the channel before we start to block
const MAX_CHAN_SIZE: usize = 2 << 12;

pub type ClientMessageChannelType = (UserIDType, ClientMessage);

type AudioPacket = Packet;
