pub mod server;
pub mod client;


/// Data that will be sent along for backing track messages.
/// Will probably just be a string describing the local file path for the song in use
type BackingTrackData = String;


/// Maximum buffer size for the channel before we start to block
const MAX_CHAN_SIZE: usize = 2 << 12;

type AudioPacket = ();  // TODO
type VRTPPacket = ();
