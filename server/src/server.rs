// use std::sync::mpsc::Receiver;

use std::borrow::Cow;
use std::collections::HashMap;
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;

// use serde_json::Result;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc::{self, Receiver, Sender}, Mutex, oneshot, RwLock};

use protocol::{osc_messages_out::ServerMessage, UserData, UserIDType, vrl_packet::VRLOSCPacket};
use protocol::handshake::{HandshakeAck, HandshakeCompletion, HandshakeSynAck};
use protocol::osc_messages_in::ClientMessage;
use protocol::UserType::{Audience, Performer};

use crate::{BackingTrackData, MAX_CHAN_SIZE, RemoteUserType, VRTPPacket};
use crate::client::ClientChannelData;

const HANDSHAKE_BUF_SIZE: usize = 2048;


/// Data tracked per client.
#[derive(Debug)]
pub struct ServerUserData {
    user_type: protocol::UserType,
    base_user_data: UserData,
    // Store some channels that need to be accessible from the outside for sending data in
    /// Send on this channel to update thread on new backing tracks.
    backing_track_update_send: oneshot::Sender<BackingTrackData>,
    /// Send on this channel to update thread with new server events.
    server_event_update_send: Sender<ServerMessage>,
}

// TODO lay this out!
#[derive(Copy, Clone, Debug)]
pub struct PortMap {
    new_connections_port: u16,
    mocap_in_port: u16,
    audio_in_port: u16,
    client_event_port: u16,
    backing_track_port: u16
}

impl PortMap {
    pub fn new(new_connections_port: u16) -> Self {
        eprintln!("Portmap uses base ports!");
        Self {
            new_connections_port,
            mocap_in_port: 5653,
            audio_in_port: 5654,
            client_event_port: 5655,
            backing_track_port: 5656
        }
    }
}


/// A subset of the data available on the server which can be passed through threads.
/// This data should more or less be constant, or at least thread safe.
#[derive(Clone, Debug)]
pub struct ServerThreadData {
    host: String,
    ports: PortMap,
    synchronizer_to_out_tx: Sender<VRTPPacket>,
    client_events_in_tx: Sender<ClientMessage>,
    cur_user_id: Arc<Mutex<UserIDType>>,
    extra_ports: Arc<HashMap<String, u16>>,
}

pub struct Server {
    pub host: String,
    /// Port responsible for handling new incoming connections (TCP).
    pub port_map: PortMap,
    // maybe a map connecting their joinhandles
    pub clients: Arc<RwLock<HashMap<UserIDType, ServerUserData>>>,
    // pub performers: Arc<RwLock<Vec<ServerUserData>>>,
    // pub audience: Arc<RwLock<Vec<ServerUserData>>>,
    pub cur_user_id: Arc<Mutex<UserIDType>>,

    extra_ports: Arc<HashMap<String, u16>>,

    // These channels are here because they need to be cloned by all new clients as common channels.
    /// Output channel coming from each synchronizer and going to the high-priority output thread.
    pub synchronizer_to_out_tx: Sender<VRTPPacket>, // TODO add the type here
    /// Hidden internal channel that receives the message inside the output thread
    synchronizer_to_out_rx: Receiver<VRTPPacket>,

    /// Client events should be sent from their constituent threads through this
    pub client_event_in_tx: Sender<ClientMessage>,
    /// Input channel for client events, which should be managed by a global event thread
    client_event_in_rx: Receiver<ClientMessage>,

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
            clients: Arc::new(RwLock::new(HashMap::new())),
            synchronizer_to_out_tx: sync_out_tx,
            synchronizer_to_out_rx: sync_out_rx,
            client_event_in_rx: client_in_rx,
            client_event_in_tx: client_in_tx,
            cur_user_id: Arc::new(Mutex::new(0)),
            extra_ports: Arc::new(HashMap::new())

        }
    }

    /// Start up the host server's connection receiving thread.
    pub async fn start(&mut self) -> io::Result<()> {
        let addr = format!("{0}:{1}", self.host, self.port_map.new_connections_port);
        let listener = TcpListener::bind(&addr).await?;

        println!("Listening on {addr}");

        loop {
            let (socket, incoming_addr) = listener.accept().await?;

            let thread_data = self.create_thread_data();
            let hm_clone = Arc::clone(&self.clients);

            println!("Got a new connection from {incoming_addr}");

            // perform the handshake synchronously, so we don't have to worry about crossing streams?
            // TODO add the ability for multiple streams at once

            tokio::spawn(async move {
                let res = Self::perform_handshake(socket, incoming_addr, thread_data).await;

                if let Err(msg) = res {
                    eprintln!("Attempted handshake with {0} failed: {msg}", incoming_addr);
                    return
                }
                let (user_id, server_data) = res.unwrap();
                let mut write_handle = hm_clone.write().await;
                write_handle.insert(user_id, server_data);


            });



        }

    }


    /// Perform the handshake. If this completes, it should add a new client.
    /// If this also succeeds, then it should spin up the necessary threads for listening
    async fn perform_handshake(mut socket: TcpStream, addr: SocketAddr, server_thread_data: ServerThreadData) -> Result<(UserIDType, ServerUserData), String> {
        let our_user_id;
        let mut handshake_buf: [u8; HANDSHAKE_BUF_SIZE] = [0; HANDSHAKE_BUF_SIZE];
        // lock the user ID and increment it
        // if we don't complete the handshake it's okay, we will just need this ID when we're done
        {
            let mut user_id = server_thread_data.cur_user_id.lock().await;
            *user_id += 1;
            our_user_id = *user_id;
        }
        let handshake_start = HandshakeAck::new(our_user_id, server_thread_data.host);

        let handshake_start_msg = serde_json::to_string(&handshake_start).unwrap();
        let _ = socket.write_all(handshake_start_msg.as_bytes()).await;

        let bytes_read = socket.read(&mut handshake_buf).await;

        if bytes_read.is_err() {
            return Err("Failed to read bytes for some reason!".to_owned());
        }

        let bytes = bytes_read.unwrap();

        let str_bytes = &handshake_buf[0..bytes].to_ascii_lowercase();
        let str = String::from_utf8_lossy(&str_bytes);
        println!("Synack received: {0}", match str {
            Cow::Borrowed(st) => st.to_owned(),
            Cow::Owned(st) => st
        });



        let synack: HandshakeSynAck = match serde_json::from_slice(&handshake_buf[0..bytes]) {
            Ok(synack_js) => synack_js,
            Err(e) => return Err(format!("Synack response appears malformed: {e}").to_owned())
        };

        handshake_buf = [0; HANDSHAKE_BUF_SIZE];

        // perform some sanity checks
        if synack.user_id != our_user_id {
            return Err("Synack returned a user ID that did not match the one we started the handshake with".to_owned());
        }

        // TODO should probably check for duplicate ports/other sanity things here

        // send out our last part of the handshake
        let handshake_finish = HandshakeCompletion {
            extra_ports: server_thread_data.extra_ports.as_ref().clone()
        };

        let handshake_finish_msg = serde_json::to_string(&handshake_finish).unwrap();
        let _ = socket.write_all(handshake_finish_msg.as_bytes()).await;

        // I don't understand how a heart is a spade
        // but somehow the vital connection is made

        let user_data = UserData {
            participant_id: our_user_id,
            remote_port: addr.port(),
            remote_ip_addr: addr.ip(),
            fancy_title: synack.own_identifier
        };

        let (backing_track_send, backing_track_recv) = tokio::sync::oneshot::channel::<BackingTrackData>();
        let (server_event_out_send, server_event_out_recv) = mpsc::channel::<ServerMessage>(MAX_CHAN_SIZE);



        // let ServerUserData
        let server_user_data = ServerUserData {
            user_type: match synack.user_type {
                1 => Audience,
                2 => Performer,
                e => return Err(format!("Invalid user type specified: {e}")),
            },
            base_user_data: user_data,
            backing_track_update_send: backing_track_send,
            server_event_update_send: server_event_out_send
        };

        // return server_user_data

        let mut client_channel_data = ClientChannelData::new(
            server_event_out_recv,
            server_thread_data.client_events_in_tx.clone(),
            backing_track_recv,
        );
        client_channel_data.synchronizer_vrtp_out = Some(server_thread_data.synchronizer_to_out_tx.clone());

        // from here on out, it's up to the client to fill things out.



        println!(
            "New user {0} ({1}, id #{2}) has authenticated as {3}",
            &server_user_data.base_user_data.fancy_title,
            &server_user_data.base_user_data.remote_ip_addr,
            our_user_id,
            match &server_user_data.user_type {
                Audience => "an audience member",
                Performer => "a performer"
            }
        );
        Ok((our_user_id, server_user_data))

    }



    fn create_thread_data(&self) -> ServerThreadData {
        ServerThreadData {
            host: self.host.clone(),
            ports: self.port_map,
            synchronizer_to_out_tx: self.synchronizer_to_out_tx.clone(),
            client_events_in_tx: self.client_event_in_tx.clone(),
            cur_user_id: Arc::clone(&self.cur_user_id),
            extra_ports: self.extra_ports.clone(),

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
