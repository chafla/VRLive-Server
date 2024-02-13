use tokio::sync::mpsc::{Receiver, Sender};
use protocol::vrl_packet::VRLOSCPacket;
use crate::{AudioPacket, server, VRTPPacket};
use crate::server::ServerUserData;

pub struct AudienceMember {

}


/// Client-specific channel data.
/// Stored out in a separate struct for organization, especially since this data will be common to all client types.
pub struct ClientChannelData {
    /// Sending incoming mocap data to our synchronizer
    mocap_out: Sender<VRLOSCPacket>,
    /// Get events from the server's main event thread, to send out to a user
    server_events_out: Receiver<VRLOSCPacket>,
    /// Pass events from our client to the main server
    client_events_in: Sender<VRLOSCPacket>,
    // Performer clients manage their own synchronizer threads.
    // This means that they may exist, or may not if we're an audience member.
    synchronizer_osc_in: Option<Receiver<VRLOSCPacket>>,
    synchronizer_audio_in: Option<Receiver<AudioPacket>>,
    /// Sending data from the synchronizer to the server's output thread.
    synchronizer_vrtp_out: Option<Sender<VRTPPacket>>,

}


/// Trait defining the necessary behavior for a client of our server.
pub trait VRLClient {
    /// Initialize this connection, creating the necessary channels.
    fn create_connection(&self, user_data: ServerUserData);

    /// Start all of the main channel tasks.
    fn start_main_channels(&self);

    /// Listener thread for incoming motion capture events.
    /// These events will be forwarded through a channel to the main server's motion capture thread.
    fn mocap_listener(&self, mocap_sender: Sender<VRLOSCPacket>);

    /// Listener thread for incoming audio events.
    /// This will also be forwarded to the main server's mocap thread.
    /// Note that this may return early if we aren't handling any audio data.
    fn audio_listener(&self);  // TODO

    /// Thread handling output for any server events.
    fn server_event_sender(&self);

    /// Thread handling input for any client events.
    fn client_event_listener(&self);

    /// Thread responsible for updating the backing track as needed.
    fn backing_track_sender(&self);
}

pub struct AudienceConnection {

}

// impl VRLCLient for AudienceConnection {
//     fn create_connection(user_data: ServerUserData) {
//         return AudienceConnection {

//         }
//     }
// }

// pub struct