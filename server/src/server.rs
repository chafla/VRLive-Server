use std::sync::Arc;
use std::sync::mpsc::Receiver;
use tokio::sync::{mpsc::{self, Sender}, oneshot};
use protocol::{osc_messages_out::ServerMessage, vrl_packet::VRLOSCPacket, UserData, VRLUser, OSCEncodable};

use crate::{BackingTrackData, RemoteUserType};

/// Maximum buffer size for the channel before we start to block
const MAX_CHAN_SIZE: usize = 2 >> 12;

type AudioPacket = ();  // TODO
type VRTPPacket = ();


/// Data for the server side to keep track of for every managed client.
struct ServerUserData {
    base_user_data: UserData,
    // Store some channels that need to be accessible from the outside for sending data in
    /// Send on this channel to update thread on new backing tracks.
    backing_track_update_send: oneshot::Sender<BackingTrackData>,
    /// Send on this channel to update thread with new server events.
    server_event_update_send: Sender<ServerMessage>,
}

/// Client-specific channel data.
/// Stored out in a separate struct for organization, especially since this data will be common to all client types.
struct ClientChannelData {
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

impl ServerUserData {
    // pub fn new(data: UserData, backing_track_send: oneshot::Sender<BackingTrackData>, server_update_send: mpsc::Sender<ServerMessage>) -> Self {
    //     Self {
    //         base_user_data: data,
    //         backing_track_update_send: backing_track_send,
    //         server_event_update_send: server_update_send
    //     }
    // }
}

struct Server {
    // maybe a map connecting their joinhandles
    pub performers: Vec<VRLUser>,
    pub audience: Vec<VRLUser>,

    // These channels are here because they need to be cloned by all new clients as common channels.
    /// Output channel coming from each synchronizer and going to the high-priority output thread.
    pub synced_data_out_send: mpsc::Sender<VRTPPacket>, // TODO add the type here

    ///
    // pub client_event_in: mpsc::Sender<dyn OSCEncodable>,

    ///
    // synced_data_out_recv: mpsc::Receiver<OutputData>,

    /// Client events should be sent from their constituent threads through this
    pub client_event_in_send: mpsc::Sender<ServerMessage>,
    /// Input channel for client events, which should be managed by a global event thread
    client_event_in_recv: mpsc::Receiver<ServerMessage>

    // due to how mpsc channels work, it's probably easier to include the music channel in 

}

/// A struct that collects all communication channels needed during the construction of the server.
pub struct CommunicationChannels {

}

// pub struct 

impl Server {
    pub fn new() {}

    /// Main connection manager.
    /// Receive a new connection, calling initialize_new_user() when needed.
    pub fn incoming_connection_manager() {

    }


    /// Create a new user, spinning up all the necessary threads as needed.
    pub fn initialize_new_user(user: RemoteUserType, data: UserData) {
        
        // build up the channels that will be common between both

        // mocap osc in
        // events in
        // events out
        // audio stream
        
        // mocap osc in -- UDP routed
        // useful for audience and performers, though only performers will have a high-priority one
        let (mocapInSend, mocapInRecv) = mpsc::channel::<VRLOSCPacket>(MAX_CHAN_SIZE);

        // audience mocap out -- UDP routed
        // Everyone gets this, it comes free with your xbox
        let (audienceMocapOutSend, audienceMocapOutRecv) = mpsc::channel::<VRLOSCPacket>(MAX_CHAN_SIZE);

        // osc server events out -- TCP routed
        let (serverEventOutSend, serverEventOutRecv) = mpsc::channel::<ServerMessage>(MAX_CHAN_SIZE);

        // backing track out -- TCP routed
        let (backingTrackSend, backingTrackRecv) = oneshot::channel::<BackingTrackData>();

        // This is the data that the server needs to hold onto for every user.
        let user_data = ServerUserData {
            base_user_data: data,
            // server needs to update these channels to provide base user data.
            backing_track_update_send: backingTrackSend, 
            server_event_update_send: serverEventOutSend
        };

        // spin off channels here?
        // or do it elsewhere?
        
        // other channels are either unique to the client type OR handled by the server





        match user {
            RemoteUserType::Audience => {
                // Channels handling the receipt of mocap data
                let (audienceMocapInSend, audienceMocapInRecv) = mpsc::channel::<VRLOSCPacket>(MAX_CHAN_SIZE);


            },
            RemoteUserType::Server => {
                // TODO spawn the audio stream here
            }

        }


        
    }




}


/// Trait defining the necessary behavior for a client of our server.
pub trait VRLCLient {
    /// Initialize this connection, creating the necessary channels.
    fn create_connection(user_data: ServerUserData);

    /// Start all of the main channel tasks.
    fn start_main_channels();

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