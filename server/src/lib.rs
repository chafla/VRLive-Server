use protocol::{osc_messages_out::ServerMessage, UserData};
use tokio::sync::{mpsc, oneshot};

mod server;
mod client;


/// Data that will be sent along for backing track messages.
/// Will probably just be a string describing the local file path for the song in use
type BackingTrackData = String;




enum RemoteUserType {
    Audience,
    Server,
}
