use protocol::{osc_messages_out::ServerMessage, UserData};
use tokio::sync::{mpsc, oneshot};

mod server;



/// As-of-yet-undetermined data format that will combine both audio data and mocap data.
type OutputData = ();

/// Data that will be sent along for backing track messages.
/// Will probably just be a string describing the local file path for the song in use
type BackingTrackData = String;




enum RemoteUserType {
    Audience,
    Server,
}
