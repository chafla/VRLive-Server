pub mod server;
pub mod client;
mod audience;
mod performer;


/// Maximum buffer size for the channel before we start to block
const MAX_CHAN_SIZE: usize = 2 << 12;

type AudioPacket = ();  // TODO
type VRTPPacket = ();
