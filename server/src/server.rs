// use std::sync::mpsc::Receiver;

use std::borrow::Cow;
use std::collections::HashMap;
use std::io;
use std::io::Write;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::sync::Arc;

use log::{debug, error, info, warn};
use rosc::{decoder, OscPacket, encoder};
// use serde_json::Result;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpSocket, TcpStream, UdpSocket};
use tokio::sync::{mpsc::{self, Receiver, Sender}, Mutex, RwLock};
use tokio::sync::mpsc::channel;

use protocol::{osc_messages_out::ServerMessage, UserData, UserIDType};
use protocol::handshake::{HandshakeAck, HandshakeCompletion, HandshakeSynAck};
use protocol::osc_messages_in::ClientMessage;
use protocol::osc_messages_out::PerformerServerMessage;
use protocol::UserType;

use protocol::backing_track::BackingTrackData;

use crate::{MAX_CHAN_SIZE, VRTPPacket};
use crate::client::{ClientChannelData, ClientPorts, VRLClient};
use crate::client::performer;
use crate::client::audience;

use crate::client::audience::AudienceMember;

const HANDSHAKE_BUF_SIZE: usize = 2048;
const LISTENER_BUF_SIZE: usize = 2048;

const AUDIENCE_BUFFER_CHAN_SIZE: usize = 2048;

const MULTICAST_ADDR_STRING: &str = "226.226.226.226";


/// Messages which can be sent over the client message socket.
enum ClientChannelMessage {
    /// A socket representing a new initialization.
    /// Should be sent once and only once.
    /// TODO we could do something fancy here with typed channels!
    Socket(TcpSocket),
    Message(ClientMessage)
}

enum UserImpl {
    PerformerImpl(performer::Performer),
    AudienceImpl(AudienceMember)
}


/// Data tracked per client.
#[derive(Debug, Clone)]
pub struct ServerUserData {
    user_type: protocol::UserType,
    base_user_data: UserData,
    // Store some channels that need to be accessible from the outside for sending data in
    /// Send on this channel to update thread on new backing tracks.
    backing_track_update_send: Sender<BackingTrackData>,
    /// Send on this channel to update thread with new server events.
    server_event_update_send: Sender<ServerMessage>,

    // tcp channels
    // these should theoretically be oneshot channels,
    // but those are annoying to clone

    // TODO move these out to their own data structure

    /// Send a connected socket on this channel when a new client event channel is established.
    client_event_socket_send: Sender<TcpStream>,
    /// Send a connected socket on this channel when a new backing track connection is established
    backing_track_socket_send: Sender<TcpStream>,
    /// Send a connected socket on this channel when a new server event channel is established.
    server_event_socket_send: Sender<TcpStream>,
}

// TODO lay this out!
#[derive(Copy, Clone, Debug)]
pub struct PortMap {
    new_connections_port: u16,
    audience_mocap_in_port: u16,
    performer_mocap_in_port: u16,
    audio_in_port: u16,
    client_event_port: u16,
    backing_track_port: u16,
    server_event_in_port: u16,
    audience_mocap_out_port: u16
}

impl PortMap {
    pub fn new(new_connections_port: u16) -> Self {
        warn!("Portmap uses base ports!");
        Self {
            new_connections_port,
            performer_mocap_in_port: 5653,
            audio_in_port: 5654,
            client_event_port: 5655,
            backing_track_port: 5656,
            server_event_in_port: 5657,
            audience_mocap_in_port: 5658,
            audience_mocap_out_port: 5659,
        }
    }
}


/// A subset of the data available on the server which can be passed through threads.
/// This data should more or less be constant, or at least thread safe.
#[derive(Clone, Debug)]
pub struct ServerThreadData {
    /// The host of the server.
    host: String,
    /// The ports available on the server.
    ports: PortMap,
    /// The channel that should be cloned by everyone looking to send something to the
    synchronizer_to_out_tx: Sender<VRTPPacket>,
    client_events_in_tx: Sender<ClientMessage>,
    /// The current minimum user ID.
    cur_user_id: Arc<Mutex<UserIDType>>,
    extra_ports: Arc<HashMap<String, u16>>,
}

pub struct Server {
    host: String,
    /// Map storing the ports we will be using
    port_map: PortMap,
    /// Any additional ports that we might be requesting.
    /// Stored separately from the port map so it can stay as copy
    extra_ports: Arc<HashMap<String, u16>>,
    /// Clients
    clients: Arc<RwLock<HashMap<UserIDType, ServerUserData>>>,
    /// Separate association associating them by IP rather than their ID
    clients_by_ip: Arc<RwLock<HashMap<String, ServerUserData>>>,
    /// Our current lowest user ID, used as a canonical base for adding more.
    cur_user_id: Arc<Mutex<UserIDType>>,

    // These channels are here because they need to be cloned by all new clients as common channels.
    /// Output channel coming from each synchronizer and going to the high-priority output thread.
    synchronizer_to_out_tx: Sender<VRTPPacket>, // TODO add the type here
    /// Hidden internal channel that receives the message inside the output thread
    synchronizer_to_out_rx: Receiver<VRTPPacket>,

    /// Client events should be sent from their constituent threads through this
    client_event_in_tx: Sender<ClientMessage>,
    /// Input channel for client events, which should be managed by a global event thread
    client_event_in_rx: Receiver<ClientMessage>,
    /// Whether we are using UDP multicast for our listeners.
    use_multicast: bool,

    // due to how mpsc channels work, it's probably easier to include the music channel in 

}

// pub struct 

impl Server {
    pub fn new(host: String, port_map: PortMap, use_multicast: bool) -> Self {

        let (sync_out_tx, sync_out_rx) = mpsc::channel::<VRTPPacket>(MAX_CHAN_SIZE);
        let (client_in_tx, client_in_rx) = mpsc::channel::<ClientMessage>(MAX_CHAN_SIZE);
        Server {
            host,
            port_map,
            clients: Arc::new(RwLock::new(HashMap::new())),
            clients_by_ip: Arc::new(RwLock::new(HashMap::new())),
            synchronizer_to_out_tx: sync_out_tx,
            synchronizer_to_out_rx: sync_out_rx,
            client_event_in_rx: client_in_rx,
            client_event_in_tx: client_in_tx,
            cur_user_id: Arc::new(Mutex::new(0)),
            extra_ports: Arc::new(HashMap::new()),
            use_multicast

        }
    }

    fn get_host(&self) -> &str {
        return &self.host;
    }


    pub async fn main_server_loop(&mut self) {

        loop {
            // TODO
        }

    }

    /// Start up the host server's connection receiving thread.
    pub async fn start(&mut self) -> io::Result<()> {


        self.start_client_listeners().await;
        // if this fails we're giving up
        self.new_connection_listener().await.unwrap();
        self.start_audience_mocap_listeners().await;

        self.main_server_loop().await;
        Ok(())

    }

    async fn start_audience_mocap_listeners(&self) {

        let (mocap_send, mocap_recv) = channel::<OscPacket>(AUDIENCE_BUFFER_CHAN_SIZE);
        let mocap_in_addr = SocketAddrV4::new("0.0.0.0".parse().unwrap(), self.port_map.audience_mocap_in_port);
        tokio::spawn(async move {
            Self::audience_mocap_listener(&mocap_in_addr, mocap_send).await;
        });

        let mocap_out_ip = if self.use_multicast {
            let res: Ipv4Addr = MULTICAST_ADDR_STRING.parse().unwrap();
            assert!(res.is_multicast(), "Must be multcast address");
            res
        } else {"0.0.0.0".parse().unwrap()};

        let mocap_out_addr= SocketAddrV4::new(mocap_out_ip, self.port_map.audience_mocap_out_port);

        tokio::spawn(async move {
            Self::audience_mocap_dispatch(&mocap_out_addr, mocap_recv).await.unwrap();
        });


    }

    async fn start_listener(&self, fancy_title: &'static str, port: u16, channel_getter: fn(&ServerUserData) -> Sender<TcpStream>) {
        let host = self.host.clone();
        let host_map = Arc::clone(&self.clients_by_ip);
        let _ = tokio::spawn(async move {
            Self::client_connection_listener(port, host, host_map, channel_getter, &fancy_title.to_owned()).await
        });
    }

    async fn start_client_listeners(&self) {
        self.start_listener(
            "client event",
            self.port_map.client_event_port,
            | user_data: &ServerUserData | user_data.client_event_socket_send.clone(),
        ).await;

        self.start_listener(
            "backing track",
            self.port_map.backing_track_port,
            | user_data: &ServerUserData | user_data.backing_track_socket_send.clone(),
        ).await;

        self.start_listener(
            "server event in",
            self.port_map.server_event_in_port,
            | user_data: &ServerUserData | user_data.server_event_socket_send.clone(),
        ).await;


    }

    async fn new_connection_listener(&self) -> io::Result<()> {
        let addr = format!("{0}:{1}", self.host, self.port_map.new_connections_port);
        let listener = TcpListener::bind(&addr).await?;

        let clients = Arc::clone(&self.clients);
        let clients_by_ip = Arc::clone(&self.clients_by_ip);
        info!("Listening for new connections on {addr}");
        let thread_data = self.create_thread_data();

        tokio::spawn(async move {
            loop {
                let (socket, incoming_addr) = match listener.accept().await {
                    Ok(p) => p,
                    Err(e) => {
                        error!("Failed when accepting a listener: {e}");
                        continue;
                    }
                };

                // todo this should probably come out of create_server_data, find a good way to replicate that
                let thread_data = thread_data.clone();
                let clients_local = Arc::clone(&clients);
                let clients_by_ip_local = Arc::clone(&clients_by_ip);

                info!("Got a new connection from {incoming_addr}");

                // perform the handshake synchronously, so we don't have to worry about crossing streams?
                // TODO add the ability for multiple streams at once

                tokio::spawn(async move {
                    let res = Self::perform_handshake(socket, incoming_addr, thread_data).await;

                    if let Err(msg) = res {
                        error!("Attempted handshake with {0} failed: {msg}", incoming_addr);
                        return
                    }

                    let (user_id, server_data) = res.unwrap();


                    {
                        // sample messages!
                        let _ = server_data.server_event_update_send.send(ServerMessage::Performer(PerformerServerMessage::Ready(true))).await;
                    }

                    let mut write_handle = clients_local.write().await;
                    write_handle.insert(user_id, server_data.clone());

                    let mut write_handle = clients_by_ip_local.write().await;
                    write_handle.insert(server_data.base_user_data.remote_ip_addr.to_string(), server_data);
                });
            }
        });

        Ok(())
    }

    async fn audience_mocap_dispatch(socket_addr: &SocketAddrV4, mut listener_channel: Receiver<OscPacket>) -> io::Result<()> {


        debug!("Ready to transmit audience mocap on {socket_addr}");

        let sock = UdpSocket::bind(socket_addr).await?;

        loop {
            let pkt = match listener_channel.recv().await {
                None => break,
                Some(data) => encoder::encode(&data).unwrap()
            };

            sock.send(&pkt).await?;


        }

        Ok(())


    }

    /// Take in motion capture from audience members and sent it out as soon as we can.
    /// Audience members don't really need to worry about sending to specific "clients",
    /// instead they can just forward their data on here, and it will be repeated to any audience members listening.
    async fn audience_mocap_listener(socket_addr: &SocketAddrV4, listener_channel: Sender<OscPacket>) {
        // https://github.com/henninglive/tokio-udp-multicast-chat/blob/master/src/main.rs

        // assert!(multicast_socket_addr.ip().is_multicast(), "Must be multcast address");

        info!("Listening for audience mocap on {0}", &socket_addr);
        let sock = UdpSocket::bind(&socket_addr).await.unwrap();

        let mut listener_buf : [u8; LISTENER_BUF_SIZE] = [0; LISTENER_BUF_SIZE];

        loop {
            let (bytes_read, _) = match sock.recv_from(&mut listener_buf).await {
                Err(e) => {
                    error!("failed to read from listener buffer: {e}");
                    continue;
                }

                Ok(b) => b
            };

            // just forward the raw packets, do as little processing as possible
            let datagram_data = &listener_buf[0..bytes_read];

            let (_, pkt) = match decoder::decode_udp(datagram_data) {
                Err(e) => {
                    error!("Audience mocap listener received something that doesn't seem to be OSC: {e}");
                    continue;
                },
                Ok(r) => r
            };

            let _ = listener_channel.send(pkt).await;
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
        debug!("Synack received: {0}", match str {
            Cow::Borrowed(st) => st.to_owned(),
            Cow::Owned(st) => st
        });

        let synack: HandshakeSynAck = match serde_json::from_slice(&handshake_buf[0..bytes]) {
            Ok(synack_js) => synack_js,
            Err(e) => return Err(format!("Synack response appears malformed: {e}").to_owned())
        };

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
            remote_ip_addr: addr.ip(),
            fancy_title: synack.own_identifier
        };

        let ports = ClientPorts::new(
            synack.server_event_port,
            synack.backing_track_port,
            Some(synack.extra_ports),
        );

        let (backing_track_send, backing_track_recv) = mpsc::channel::<BackingTrackData>(MAX_CHAN_SIZE);
        let (server_event_out_send, server_event_out_recv) = mpsc::channel::<ServerMessage>(MAX_CHAN_SIZE);

        let (client_event_socket_out, client_event_socket_in) = mpsc::channel::<TcpStream>(MAX_CHAN_SIZE);
        let (backing_track_socket_tx, backing_track_socket_rx) = mpsc::channel::<TcpStream>(MAX_CHAN_SIZE);
        let (server_event_socket_out, server_event_socket_in) = mpsc::channel::<TcpStream>(MAX_CHAN_SIZE);

        // let ServerUserData
        let server_user_data = ServerUserData {
            user_type: match synack.user_type {
                1 => UserType::Audience,
                2 => UserType::Performer,
                e => return Err(format!("Invalid user type specified: {e}")),
            },
            base_user_data: user_data,
            backing_track_update_send: backing_track_send,
            server_event_update_send: server_event_out_send,
            client_event_socket_send: client_event_socket_out,
            backing_track_socket_send: backing_track_socket_tx,
            server_event_socket_send: server_event_socket_out
        };

        // return server_user_data

        let mut client_channel_data = ClientChannelData::new(
            server_event_out_recv,
            server_thread_data.client_events_in_tx.clone(),
            backing_track_recv,
            client_event_socket_in,
            backing_track_socket_rx,
            server_event_socket_in,
        );
        client_channel_data.synchronizer_vrtp_out = Some(server_thread_data.synchronizer_to_out_tx.clone());




        let server_user_data_inner = server_user_data.clone();
        // pass off the client to handle themselves
        // I want to make this trait-based, but it seems like the types in use (particularly with async?) cause some kinda recursion in the type checker
        let _ = tokio::spawn(async move {
            match server_user_data_inner.user_type {
                UserType::Audience => {
                    let mut mem = AudienceMember::new(
                        server_user_data_inner.base_user_data.clone(),
                        client_channel_data,
                        ports,
                        socket
                    );
                    mem.start_main_channels().await;
                },
                UserType::Performer => {
                    let mut aud = performer::Performer::new(
                        server_user_data_inner.base_user_data.clone(),
                        client_channel_data,
                        ports,
                        socket
                    ).await;
                    aud.start_main_channels().await;
                }
            };
        });

        // from here on out, it's up to the client to fill things out.



        info!(
            "New user {0} ({1}, id #{2}) has authenticated as {3}",
            &server_user_data.base_user_data.fancy_title,
            &server_user_data.base_user_data.remote_ip_addr,
            our_user_id,
            match &server_user_data.user_type {
                UserType::Audience => "an audience member",
                UserType::Performer => "a performer"
            }
        );
        Ok((our_user_id, server_user_data))

    }

    /// Function for forwarding a TCP socket channel onto the client.
    /// The client should have a corresponding channel that they will be listening on.
    async fn client_connection_listener(
        listening_port: u16, host: String, users_by_ip: Arc<RwLock<HashMap<String, ServerUserData>>>,
        channel_getter: impl Fn(&ServerUserData) -> Sender<TcpStream>, listener_label: &str,

    ) -> io::Result<()>
    {
        let addr = format!("{0}:{1}", host, listening_port);
        let listener = TcpListener::bind(&addr).await?;

        info!("Listening for {listener_label} on {addr}");

        loop {
            let (socket, incoming_addr) = listener.accept().await?;
            let found_user_lock = users_by_ip.read().await;
            let found_user = found_user_lock.get(&incoming_addr.ip().to_string());
            if found_user.is_none() {
                warn!("Received a {listener_label} connection request from {0}, but they have not initiated a handshake!", &incoming_addr.to_string());
                continue;
            }
            else {
                info!("{listener_label} connection established for {0}", &incoming_addr);
            }
            let found_user = found_user.unwrap();
            let _ = channel_getter(found_user).send(socket).await;
        }

    }
    // async fn performer_audio_listener(
    //     listening_addr: &SocketAddrV4, users_by_ip: Arc<RwLock<HashMap<String, ServerUserData>>>,
    //     audio_channel: Sender<()>
    // ) {
    //     info!("Listening for performer audio on {listening_addr}");
    //
    //     let sock = UdpSocket::bind(listening_addr).await?;
    //
    //     loop {
    //         let mut bytes_in = bytes::BytesMut::with_capacity(2048);
    //         let (bytes_read, incoming_addr) = match sock.recv_from(bytes_in.as_mut()).await {
    //             Err(e) => {
    //                 error!("failed to read from listener buffer: {e}");
    //                 continue;
    //             }
    //
    //
    //
    //             Ok(b) => b
    //         };
    //
    //         let bytes_in = bytes_in.freeze();
    //
    //         // just forward the raw packets, do as little processing as possible
    //         // let datagram_data = &listener_buf[0..bytes_read];
    //
    //         // TODO
    //         // this is going to be problematic.
    //         // If we have a read lock on this data structure that is held every single time RTP data is coming in over
    //         // the stream, we're going to get deadlocked FAST, and the writers (new clients being added) will cause this
    //         // all to slow to a slog.
    //         let performer_listener = match users_by_ip.read().await.get(&incoming_addr.ip().to_string()) {
    //             Some(data) => {
    //                 if let
    //             }
    //         }
    //
    //         let (_, pkt) = match decoder::decode_udp(datagram_data) {
    //             Err(e) => {
    //                 error!("Audience mocap listener received something that doesn't seem to be OSC: {e}");
    //                 continue;
    //             },
    //             Ok(r) => r
    //         };
    //     }
    //
    //
    //
    //
    // }



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
}
