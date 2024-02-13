// use std::sync::mpsc::Receiver;

use std::io;
use std::net::{IpAddr, SocketAddr, SocketAddrV4};
use std::str::FromStr;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc::{self, Sender, Receiver}, oneshot, RwLock};

use protocol::{osc_messages_out::ServerMessage, UserData, vrl_packet::VRLOSCPacket, VRLUser, UserIDType};
use protocol::osc_messages_in::ClientMessage;

use crate::{BackingTrackData, MAX_CHAN_SIZE, RemoteUserType, VRTPPacket};

pub enum UserType {
    Audience,
    Performer
}

/// Data tracked per client.
pub struct ServerUserData {
    user_type: UserType,
    base_user_data: UserData,
    // Store some channels that need to be accessible from the outside for sending data in
    /// Send on this channel to update thread on new backing tracks.
    backing_track_update_send: oneshot::Sender<BackingTrackData>,
    /// Send on this channel to update thread with new server events.
    server_event_update_send: Sender<ServerMessage>,
}

// TODO lay this out!
#[derive(Copy, Clone, Debug)]
struct PortMap {
    new_connections_port: u16,
    mocap_in_port: u16,
    audio_in_port: u16,
    client_event_port: u16,
    backing_track_port: u16
}


/// A subset of the data available on the server which can be passed through threads.
/// This data should more or less be constant, or at least thread safe.
#[derive(Clone, Debug)]
pub struct ServerThreadData {
    host: String,
    ports: PortMap,
    synchronizer_to_out_tx: Sender<VRTPPacket>,
    client_events_in_tx: Sender<ClientMessage>,
}

struct Server {
    pub host: String,
    /// Port responsible for handling new incoming connections (TCP).
    pub port_map: PortMap,
    // maybe a map connecting their joinhandles
    pub performers: Arc<RwLock<Vec<ServerUserData>>>,
    pub audience: Arc<RwLock<Vec<ServerUserData>>>,

    // These channels are here because they need to be cloned by all new clients as common channels.
    /// Output channel coming from each synchronizer and going to the high-priority output thread.
    pub synchronizer_to_out_tx: Sender<VRTPPacket>, // TODO add the type here
    /// Hidden internal channel that receives the message inside the output thread
    synchronizer_to_out_rx: Receiver<VRTPPacket>,

    /// Client events should be sent from their constituent threads through this
    pub client_event_in_tx: Sender<ClientMessage>,
    /// Input channel for client events, which should be managed by a global event thread
    client_event_in_rx: Receiver<ClientMessage>

    // due to how mpsc channels work, it's probably easier to include the music channel in 

}

// pub struct 

impl Server {
    pub fn new(host: String, port_map: PortMap) -> Self {

        let (sync_out_tx, sync_out_rx) = mpsc::channel::<VRTPPacket>(MAX_CHAN_SIZE);
        let (client_in_tx, client_in_rx) = mpsc::channel::<ClientMessage>(MAX_CHAN_SIZE);
        Server {
            host,
            port_map,
            performers: Arc::new(RwLock::new(vec![])),
            audience: Arc::new(RwLock::new(vec![])),
            synchronizer_to_out_tx: sync_out_tx,
            synchronizer_to_out_rx: sync_out_rx,
            client_event_in_rx: client_in_rx,
            client_event_in_tx: client_in_tx,

        }
    }

    /// Start up the host server's connection receiving thread.
    pub async fn start(&mut self) -> io::Result<()> {
        // let ip_addr = (::from_str(&self.host)).unwrap();
        // let connecting_addr = SocketAddr::new(f, self.new_connections_port);
        // run the main incoming connection loop
        let listener = TcpListener::bind(format!("{0}:{1}", self.host, self.port_map.new_connections_port)).await?;

        loop {
            let (socket, incoming_addr) = listener.accept().await?;

            let thread_data = self.create_thread_data();

            tokio::spawn(async move {
                Self::perform_handshake(socket, incoming_addr, thread_data).await
            });



        }

    }


    /// Perform the handshake. If this completes, it should add a new client.
    /// If this also succeeds, then it should spin up the necessary threads for listening
    async fn perform_handshake(socket: TcpStream, addr: SocketAddr, server_thread_data: ServerThreadData) {

    }


    fn create_thread_data(&self) -> ServerThreadData {
        ServerThreadData {
            host: self.host.clone(),
            ports: self.port_map,
            synchronizer_to_out_tx: self.synchronizer_to_out_tx.clone(),
            client_events_in_tx: self.client_event_in_tx.clone(),
        }
    }

    /// Main connection manager.
    /// Receive a new connection, calling initialize_new_user() when needed.
    // pub fn create_incoming_connection() -> Box<dyn VRLClient> {
    //
    // }


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

        // // This is the data that the server needs to hold onto for every user.
        // let user_data = ServerUserData {
        //     base_user_data: data,
        //     // server needs to update these channels to provide base user data.
        //     backing_track_update_send: backingTrackSend,
        //     server_event_update_send: serverEventOutSend
        // };

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
