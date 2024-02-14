use tokio::sync::mpsc::{Receiver, Sender};

use protocol::osc_messages_in::ClientMessage;
use protocol::osc_messages_out::ServerMessage;
use protocol::UserData;
use protocol::vrl_packet::VRLOSCPacket;

use crate::{AudioPacket, BackingTrackData, VRTPPacket};
use crate::server::ServerUserData;

pub struct AudienceMember {
    pub user_data: UserData,
    pub base_channels: ClientChannelData,

}

impl VRLClient for AudienceMember {
    async fn create_connection(&self, user_data: ServerUserData) {
        todo!()
    }


    async fn start_main_channels(&self) {
        todo!()
    }

    /// Task responsible for sending out server events.
    async fn server_event_sender(&self) {
        // let sender = UdpSocket::bind("").await;
    }

    /// Task responsible for listening for incoming client events.
    async fn client_event_listener(&self) {
        todo!()
    }

    /// Task responsible for monitoring the server's backing track channel, and
    /// dispatching a new background track when it becomes available.
    async fn backing_track_sender(&self) {
        todo!()
    }
}


/// Client-specific channel data.
/// Stored out in a separate struct for organization, especially since this data will be common to all client types.
pub struct ClientChannelData {
    // channels that are somewhat managed by the server

    /// Get events from the server's main event thread, to send out to a user
    pub server_events_out: Receiver<ServerMessage>,
    /// Get backing track data to send out to our connected server.
    pub backing_track_in: tokio::sync::oneshot::Receiver<BackingTrackData>,
    /// Pass events from our client to the main server
    pub client_events_in: Sender<ClientMessage>,

    // channels dependent on the synchronizer, thus may exist depending on the type of client that we are

    // Performer clients manage their own synchronizer threads.
    // This means that they may exist, or may not if we're an audience member.
    pub synchronizer_osc_in: Option<Receiver<VRLOSCPacket>>,
    pub synchronizer_audio_in: Option<Receiver<AudioPacket>>,
    /// Sending data from the synchronizer to the server's output thread.
    pub synchronizer_vrtp_out: Option<Sender<VRTPPacket>>,
}

impl ClientChannelData {
    pub fn new(server_events_out: Receiver<ServerMessage>, client_events_in: Sender<ClientMessage>, backing_track_in: tokio::sync::oneshot::Receiver<BackingTrackData>) -> Self {
        Self {
            server_events_out,
            client_events_in,
            backing_track_in,
            synchronizer_osc_in: None,
            synchronizer_audio_in: None,
            synchronizer_vrtp_out: None
        }
    }
}



/// Trait defining the necessary behavior for a client of our server.
pub trait VRLClient {
    /// Initialize this connection, creating the necessary channels.
    async fn create_connection(&self, user_data: ServerUserData);

    /// Start all of the main channel tasks.
    async fn start_main_channels(&self);

    /// Thread handling output for any server events.
    async fn server_event_sender(&self);

    /// Thread handling input for any client events.
    async fn client_event_listener(&self);

    /// Thread responsible for updating the backing track as needed.
    async fn backing_track_sender(&self);
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