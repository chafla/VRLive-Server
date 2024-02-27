// use std::sync::mpsc::Receiver;

use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt::Debug;
use std::io;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::sync::Arc;
use std::time::{Instant, SystemTime};

use bytes::Bytes;
use log::{debug, error, info, trace, warn};
use rosc::{decoder, OscBundle, OscPacket};
use rustyline_async;
use rustyline_async::Readline;
use rustyline_async::ReadlineEvent::{Eof, Interrupted, Line};
// use serde_json::Result;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpSocket, TcpStream, UdpSocket};
use tokio::sync::{mpsc::{self, Receiver, Sender}, Mutex, RwLock};
use tokio::sync::mpsc::channel;
use webrtc::rtp::packet::Packet;
use webrtc::util::Unmarshal;

use protocol::{osc_messages_out::ServerMessage, UserData, UserIDType};
use protocol::backing_track::BackingTrackData;
use protocol::handshake::{HandshakeAck, HandshakeCompletion, HandshakeSynAck};
use protocol::osc_messages_in::ClientMessage;
use protocol::osc_messages_out::PerformerServerMessage;
use protocol::UserType;

use crate::{MAX_CHAN_SIZE, VRTPPacket};
use crate::client::{ClientChannelData, ClientPorts, VRLClient};
use crate::client::audience::AudienceMember;
use crate::client::performer;
use crate::client::synchronizer::OscData;

const DEFAULT_CHAN_SIZE: usize = 2048;

const HANDSHAKE_BUF_SIZE: usize = DEFAULT_CHAN_SIZE;
const LISTENER_BUF_SIZE: usize = DEFAULT_CHAN_SIZE;

const AUDIENCE_BUFFER_CHAN_SIZE: usize = DEFAULT_CHAN_SIZE;

const AUDIENCE_MOCAP_MULTICAST_ADDR: &str = "226.226.226.226";
const VRTP_MULTICAST_ADDR: &str = "226.226.226.227";


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


/// Data tracked on the server per client.
#[derive(Debug, Clone)]
pub struct ServerUserData {
    user_type: protocol::UserType,
    base_user_data: UserData,
    // Store some channels that need to be accessible from the outside for sending data in
    /// Send on this channel to update thread on new backing tracks.
    // backing_track_update_send: Sender<BackingTrackData>,
    /// Send on this channel to update thread with new server events.
    // server_event_update_send: Sender<ServerMessage>,
    performer_audio_sender: Sender<Packet>,
    performer_mocap_sender: Sender<OscBundle>,

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
    audience_mocap_out_port: u16,
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
            audience_mocap_in_port: 9000,
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
    synchronizer_to_out_rx: Option<Receiver<VRTPPacket>>,

    /// Client events should be sent from their constituent threads through this
    client_event_in_tx: Sender<ClientMessage>,
    /// Input channel for client events, which should be managed by a global event thread
    client_event_in_rx: Option<Receiver<ClientMessage>>,

    /// Server event handlers.
    server_event_tx: Sender<ServerMessage>,
    server_event_rx: Option<Receiver<ServerMessage>>,
    /// Mocap handlers.
    general_mocap_tx: Sender<OscData>,
    general_mocap_rx: Option<Receiver<OscData>>,

    /// Backing track handlers
    backing_track_tx: Sender<BackingTrackData>,
    backing_track_rx: Option<Receiver<BackingTrackData>>,
    /// Whether we are using UDP multicast for our listeners.
    use_multicast: bool,
    late_chans: Option<Arc<LateServerChans<UserIDType>>>,

    // due to how mpsc channels work, it's probably easier to include the music channel in 

}

struct ListenerChannels {
    server_event: Sender<(UserIDType, Receiver<ServerMessage>)>,

}

#[derive(Clone, Debug)]
pub enum ListenerMessage <U, T>
    where
        U: PartialEq + Sync + Send + Clone,
        T: Sync + Send
{
    Subscribe(U, Sender<T>),
    Unsubscribe(U)
}

/// A thread-local struct useful for managing large amounts of listeners, without needing to do obnoxious things with
/// arcs of mutexes of whatever.
/// One of these should be created per thread that needs to dispatch to a large amount of senders (read: clients).
#[derive(Debug)]
struct Listener<T, U>
    where
        U: PartialEq + Sync + Send + Clone,
        T: Sync + Send
{
    /// Vector holding onto users and their associated channels for sending data.
    client_channels: Vec<(U, Sender<T>)>,
    /// Channel for receiving updates on users.
    user_update_channel: Receiver<ListenerMessage<U, T>>
}

impl <T, U> Listener<T, U>
    where
        U: PartialEq + Sync + Send + Clone,
        T: Sync + Send
{
    pub fn new(user_update_channel: Receiver<ListenerMessage<U, T>>) -> Self {
        Self {
            client_channels: vec![],
            user_update_channel
        }
    }

    pub fn insert(&mut self, user: U, sender: Sender<T>) {
        self.client_channels.push((user, sender));
    }

    pub fn remove(&mut self, user: &U) -> Option<(U, Sender<T>)> {
        match self.client_channels.iter().position(|(u, _)| u == user) {
            Some(p) => Some(self.client_channels.remove(p)),
            None => None
        }
        // self.client_channels = self.client_channels.into_mut().filter(|(u, _)| user != *u).collect()
    }
}


/// Server channels that will not be present on init, and serve the important purpose of being relays between server and
/// all of the clients in use.
#[derive(Clone, Debug)]
struct LateServerChans<T>
    where T: Clone + Debug + PartialEq + Send + Sync
{
    mocap_sender: Sender<ListenerMessage<T, OscData>>,
    server_event_sender: Sender<ListenerMessage<T, ServerMessage>>,
    sync_sender: Sender<ListenerMessage<T, VRTPPacket>>,
    backing_track_sender: Sender<ListenerMessage<T, BackingTrackData>>
}

impl <T> LateServerChans<T>
    where T: Clone + Debug + PartialEq + Send + Sync + 'static
{
    pub async fn subscribe(&self, user_type: &UserType, identifier: T, osc_sender: Sender<OscData>, server_event_sender: Sender<ServerMessage>, sync_sender: Sender<VRTPPacket>, backing_track_sender: Sender<BackingTrackData>) -> anyhow::Result<()> {
        self.mocap_sender.send(ListenerMessage::Subscribe(identifier.clone(), osc_sender)).await?;
        self.server_event_sender.send(ListenerMessage::Subscribe(identifier.clone(), server_event_sender)).await?;
        self.backing_track_sender.send(ListenerMessage::Subscribe(identifier.clone(), backing_track_sender)).await?;
        if matches!(user_type, UserType::Performer) {
            self.sync_sender.send(ListenerMessage::Subscribe(identifier.clone(), sync_sender)).await?;

        }
        Ok(())
    }

    pub async fn unsubscribe(&self, user_type: &UserType, identifier: &'static T) -> anyhow::Result<()> {
        // these can't just be cloned because they monomorphize down
        self.mocap_sender.send(ListenerMessage::Unsubscribe(identifier.clone())).await?;
        self.server_event_sender.send(ListenerMessage::Unsubscribe(identifier.clone())).await?;
        self.backing_track_sender.send(ListenerMessage::Unsubscribe(identifier.clone())).await?;
        if matches!(user_type, UserType::Performer) {
            self.sync_sender.send(ListenerMessage::Unsubscribe(identifier.clone())).await?;
        }
        Ok(())
    }
}

// pub struct 

impl Server {
    pub fn new(host: String, port_map: PortMap, use_multicast: bool) -> Self {

        let (sync_out_tx, sync_out_rx) = mpsc::channel::<VRTPPacket>(MAX_CHAN_SIZE);
        let (client_in_tx, client_in_rx) = mpsc::channel::<ClientMessage>(MAX_CHAN_SIZE);
        let (server_event_tx, server_event_rx) = channel::<ServerMessage>(MAX_CHAN_SIZE);
        let (base_mocap_tx, base_mocap_rx) = channel(MAX_CHAN_SIZE);
        let (backing_track_tx, backing_track_rx) = channel(MAX_CHAN_SIZE);


        Server {
            host,
            port_map,
            clients: Arc::new(RwLock::new(HashMap::new())),
            clients_by_ip: Arc::new(RwLock::new(HashMap::new())),
            synchronizer_to_out_tx: sync_out_tx,
            synchronizer_to_out_rx: Some(sync_out_rx),
            client_event_in_rx: Some(client_in_rx),
            client_event_in_tx: client_in_tx,
            server_event_rx: Some(server_event_rx),
            server_event_tx,
            cur_user_id: Arc::new(Mutex::new(0)),
            extra_ports: Arc::new(HashMap::new()),
            general_mocap_tx: base_mocap_tx,
            general_mocap_rx: Some(base_mocap_rx),
            backing_track_tx,
            backing_track_rx: Some(backing_track_rx),
            late_chans: None,

            use_multicast

        }
    }

    fn get_host(&self) -> &str {
        return &self.host;
    }


    pub async fn main_server_loop(&mut self) {

        let (mut readline, mut shared_writer) = Readline::new("> ".into()).unwrap();
        loop {
            let line = match readline.readline().await {
                Err(e) => {
                    println!("Failed to read line: {e}");
                    continue;
                },
                Ok(Eof | Interrupted) => break,
                Ok(Line(str)) => str
            };

            let (start, rest) = match line.split_once(" ") {
                Some((x, y)) => (x, Some(y)),
                None => (line.as_str(), None)
            };

            match (start, rest) {
                ("backing", Some(path)) => {
                    match BackingTrackData::open(path).await {
                        Err(e) => {
                            error!("Failed to load backing track at {path}: {e}");
                        }
                        Ok(data) => self.backing_track_tx.send(data).await.unwrap()
                    }


                },
                _ => (),
            }

            // TODO
        }

        warn!("Execution was interrupted by user!")

    }

    pub async fn change_backing_track() {

    }

    /// Start up the host server's connection receiving thread.
    pub async fn start(&mut self) -> io::Result<()> {


        self.start_client_listeners().await;
        // if this fails we're giving up

        self.start_audience_mocap_listeners(self.general_mocap_tx.clone()).await;

        // Create our internal handler relays.
        // As noted elsewhere (perhaps confusingly), these relays function to create broadcast channels from one general input source (the sender side of the listenermessage pair)
        // which route messages through to their clients (the channels they're given).
        // This works relatively well will server events.
        // start the synchronizer
        let chan = self.synchronizer_to_out_rx.take().unwrap();
        let sync_chan = self.start_handler::<VRTPPacket, UserIDType>(chan, "synchronizer").await;
        // and the server events
        let chan = self.server_event_rx.take().unwrap();
        let event_chan = self.start_handler::<ServerMessage, UserIDType>(chan, "server events").await;
        // and the mocap
        let chan = self.general_mocap_rx.take().unwrap();
        let mocap_chan = self.start_handler::<OscData, UserIDType>(chan, "general (audience) mocap").await;
        // finally, backing tracks
        let chan = self.backing_track_rx.take().unwrap();
        let backing_track_chan = self.start_handler::<BackingTrackData, UserIDType>(chan, "backing track").await;

        self.late_chans = Some(Arc::new(LateServerChans {
            mocap_sender: mocap_chan,
            sync_sender: sync_chan,
            server_event_sender: event_chan,
            backing_track_sender: backing_track_chan
        }));

        self.new_connection_listener(Arc::clone(&self.late_chans.as_ref().unwrap())).await.unwrap();
        self.start_performer_audio_listener().await;
        self.start_performer_mocap_listener().await;

        self.main_server_loop().await;
        Ok(())

    }

    fn get_ip(&self, multicast_ip: &str, default_ip: &str) -> Ipv4Addr {
        if self.use_multicast {
            let res: Ipv4Addr = multicast_ip.parse().unwrap();
            assert!(res.is_multicast(), "Must be multcast address");
            res
        } else {default_ip.parse().unwrap()}
    }

    /// The main function that you'll want to call to start up an internal message router.
    pub async fn start_handler<T, U>(&self, event_channel: Receiver<T>, label: &'static str) -> Sender<ListenerMessage<U, T>>
        where
            T: Sync + Send + Debug + Clone + 'static,
            U: Sync + Send + PartialEq + Debug + Clone + 'static
    {
        let (send, recv) = channel(DEFAULT_CHAN_SIZE);
        info!("Starting up server internal message router for {label}");
        tokio::spawn(async move {
            Self::server_relay(label, recv, event_channel).await
        });

        send
    }

    async fn start_audience_mocap_listeners(&self, mocap_send: Sender<OscData>) {

        let mocap_in_addr = SocketAddrV4::new("0.0.0.0".parse().unwrap(), self.port_map.audience_mocap_in_port);
        tokio::spawn(async move {
            Self::audience_mocap_listener(&mocap_in_addr, mocap_send).await;
        });


    }

    async fn start_performer_audio_listener(&self) {
        let audio_in_addr = SocketAddrV4::new(("127.0.0.1").parse().unwrap(), self.port_map.audio_in_port);
        let users_by_ip = Arc::clone(&self.clients_by_ip);
        tokio::spawn(async move {
            Self::performer_audio_listener(&audio_in_addr, users_by_ip).await.unwrap();
        });
    }

    async fn start_performer_mocap_listener(&self) {
        let audio_in_addr = SocketAddrV4::new(("127.0.0.1").parse().unwrap(), self.port_map.performer_mocap_in_port);
        let users_by_ip = Arc::clone(&self.clients_by_ip);
        tokio::spawn(async move {
            Self::performer_mocap_listener(&audio_in_addr, users_by_ip).await.unwrap();
        });
    }

    async fn start_listener(&self, fancy_title: &'static str, port: u16, channel_getter: fn(&ServerUserData) -> Sender<TcpStream>) {
        let host = self.host.clone();
        let host_map = Arc::clone(&self.clients_by_ip);
        let _ = tokio::spawn(async move {
            Self::client_connection_listener(port, host, host_map, channel_getter, &fancy_title.to_owned()).await
        });
    }

    /// Start new listeners for client TCP connections.
    /// These will listen for inbound connections on a given port and pass them along to worker threads that will handle them from there.
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

        // // TODO this needs to be completely torn up: this is a listener not a sender
        // // I'm honestly a little upset I missed
        self.start_listener(
            "server event in",
            self.port_map.server_event_in_port,
            | user_data: &ServerUserData | user_data.server_event_socket_send.clone(),
        ).await;

        // self.start_listener(
        //     "server event in",
        //     self.port_map.server_event_in_port,
        //     | user_data: &ServerUserData | user_data.server_event_socket_send.clone(),
        // ).await;


    }

    /// Create a listener that waits to receive updates from one channel, and relays them onto a collection of other channels.
    /// This is designed for the general use case of having one event source, and needing to relay that onto event channels of all users.
    /// For example, server events.
    /// On client init, clients will "give" the server a sender to reach their thread that ultimately dispatches server events over their TCP channel.
    /// If the client dies and its channel gets dropped, we astutely handle that and remove them from the list here.
    async fn server_relay<U, T>(label: &'static str, user_update_channel: Receiver<ListenerMessage<U, T>>, mut main_update_channel: Receiver<T>)
        where
            T: Sync + Send + Debug + Clone,
            U: Sync + Send + PartialEq + Debug + Clone
    {
        let mut listener = Listener::new(user_update_channel);
        let mut bad_ids = vec![];
        info!("Internal server listener for {label} has started.");
        // pin the future so it doesn't actually get "cancelled", meaning that the channel doesn't get closed
        // let recv_fut = main_update_channel.recv();
        // tokio::pin!(recv_fut);
        loop {

            tokio::select! {
                biased;
                main_update = main_update_channel.recv() => match main_update {
                    None => {
                        warn!("Internal server listener for {label} is shutting down due to the other side being dropped");
                        break
                    },  // again, our upstream closed
                    Some(d) => {
                        trace!("Internal server listener got data for {label}");
                        for (id, chan) in &mut listener.client_channels {
                            if chan.send(d.clone()).await.is_err() {
                                // hang onto the IDs that don't have active channels: these clients have probably been dropped
                                bad_ids.push(id.clone());
                            }
                        }
                        if !bad_ids.is_empty() {
                            bad_ids.iter().for_each(|id| {listener.remove(id);});
                            bad_ids.clear();
                        }
                    }
                },
                user_update = listener.user_update_channel.recv() => {
                    match user_update {
                        None => break,
                        Some(msg) => match msg {
                            ListenerMessage::Subscribe(res, sender) => {
                                debug!("User {0:?} has been subscribed for {label}", &res);
                                listener.client_channels.push((res, sender));

                            },
                            // this is a linear op but should be happening infrequently enough
                            ListenerMessage::Unsubscribe(res) => {
                                debug!("User {0:?} has been unsubscribed for {label}", &res);
                                listener.remove(&res);
                            }
                        }
                    }
                }
            }

        }

        warn!("{label} internal listener shutting down!");
    }

    async fn new_connection_listener(&self, late_channels: Arc<LateServerChans<UserIDType>>) -> io::Result<()> {
        let addr = format!("{0}:{1}", self.host, self.port_map.new_connections_port);
        let listener = TcpListener::bind(&addr).await?;

        let clients = Arc::clone(&self.clients);
        let clients_by_ip = Arc::clone(&self.clients_by_ip);
        info!("Listening for new connections on {addr}");
        let thread_data = self.create_thread_data();
        // let late_channels = Arc::clone(&late_channels);

        let server_event_sender = self.server_event_tx.clone();

        tokio::spawn(async move {
            let late_chans_inner = Arc::clone(&late_channels);
            let server_event_sender_inner = server_event_sender.clone();
            loop {
                let (socket, incoming_addr) = match listener.accept().await {
                    Ok(p) => p,
                    Err(e) => {
                        error!("Failed when accepting a listener: {e}");
                        continue;
                    }
                };
                let late_channels_outer = Arc::clone(&late_chans_inner);
                let server_event_sender_inner = server_event_sender_inner.clone();
                // todo this should probably come out of create_server_data, find a good way to replicate that
                let thread_data = thread_data.clone();
                let clients_local = Arc::clone(&clients);
                let clients_by_ip_local = Arc::clone(&clients_by_ip);

                info!("Got a new connection from {incoming_addr}");

                // perform the handshake synchronously, so we don't have to worry about crossing streams?
                // TODO add the ability for multiple streams at once

                tokio::spawn(async move {
                    let chans = Arc::clone(&late_channels_outer);
                    let res = Self::perform_handshake(socket, incoming_addr, thread_data, chans).await;

                    if let Err(msg) = res {
                        error!("Attempted handshake with {0} failed: {msg}", incoming_addr);
                        return
                    }

                    let (user_id, server_data) = res.unwrap();


                    {
                        server_event_sender_inner.send(ServerMessage::Performer(PerformerServerMessage::Ready(true))).await;
                        // sample messages!
                        // self.dispatch_server_event()
                        // let _ = server_data.server_event_update_send.send(ServerMessage::Performer(PerformerServerMessage::Ready(true))).await;
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

    // /// Dispatch data to a UDP socket at a given location.
    // async fn udp_data_dispatch<T, F>(socket_addr: &SocketAddrV4, mut listener_channel: Receiver<T>, data_to_bytes: F) -> io::Result<()>
    //     where
    //         T: Send + Sync,
    //         F: Fn(T) -> Bytes,
    // {
    //
    //
    //     debug!("Ready to transmit synchronized data on {socket_addr}");
    //
    //     let sock = UdpSocket::bind(socket_addr).await?;
    //
    //     loop {
    //         let pkt = match listener_channel.recv().await {
    //             None => break,
    //             Some(data) => data_to_bytes(data)
    //         };
    //
    //         sock.send(&pkt).await?;
    //
    //
    //     }
    //
    //     Ok(())
    //
    //
    // }

    /// Take in motion capture from audience members and sent it out as soon as we can.
    /// Audience members don't really need to worry about sending to specific "clients",
    /// instead they can just forward their data on here, and it will be repeated to any audience members listening.
    async fn audience_mocap_listener(socket_addr: &SocketAddrV4, listener_channel: Sender<OscData>) {
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

            let pkt = match decoder::decode_udp(datagram_data) {
                Err(e) => {
                    error!("Audience mocap listener received something that doesn't seem to be OSC: {e}");
                    continue;
                },
                Ok((_, OscPacket::Message(msg))) => {
                    warn!("Mocap listener expected bundle got message {msg:?}");
                    continue;
                },
                Ok((_, OscPacket::Bundle(b))) => b
            };

            // debug!("Echoing audience mocap!");

            let osc_data = OscData::from(pkt);

            let _ = listener_channel.send(osc_data).await;
        }
    }

    async fn dispatch_server_event(&self, message: &ServerMessage) {
        if self.server_event_tx.send(message.clone()).await.is_err() {
            panic!("Failed to send server message!")
        }
    }


    /// Perform the handshake. If this completes, it should add a new client.
    /// If this also succeeds, then it should spin up the necessary threads for listening
    async fn perform_handshake(mut socket: TcpStream, addr: SocketAddr, server_thread_data: ServerThreadData, registration_channels: Arc<LateServerChans<UserIDType>>) -> Result<(UserIDType, ServerUserData), String> {
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
            synack.vrtp_mocap_port,
            synack.audience_motion_capture_port,
            Some(synack.extra_ports),
        );

        let (backing_track_send, backing_track_recv) = mpsc::channel::<BackingTrackData>(MAX_CHAN_SIZE);
        let (server_event_out_send, server_event_out_recv) = mpsc::channel::<ServerMessage>(MAX_CHAN_SIZE);
        let (audience_mocap_out_send, audience_mocap_out_recv) = mpsc::channel::<OscData>(MAX_CHAN_SIZE);
        let (out_from_sync_tx, out_from_sync_rx) = mpsc::channel::<VRTPPacket>(MAX_CHAN_SIZE);

        let (client_event_socket_out, client_event_socket_in) = mpsc::channel::<TcpStream>(MAX_CHAN_SIZE);
        let (backing_track_socket_tx, backing_track_socket_rx) = mpsc::channel::<TcpStream>(MAX_CHAN_SIZE);
        let (server_event_socket_out, server_event_socket_in) = mpsc::channel::<TcpStream>(MAX_CHAN_SIZE);

        let (performer_audio_tx, performer_audio_rx) = mpsc::channel(MAX_CHAN_SIZE);
        let (performer_mocap_tx, performer_mocap_rx) = mpsc::channel(MAX_CHAN_SIZE);

        // let ServerUserData
        let server_user_data = ServerUserData {
            user_type: match synack.user_type {
                1 => UserType::Audience,
                2 => UserType::Performer,
                e => return Err(format!("Invalid user type specified: {e}")),
            },
            base_user_data: user_data,
            // backing_track_update_send: backing_track_send,
            // server_event_update_send: server_event_out_send,
            client_event_socket_send: client_event_socket_out,
            backing_track_socket_send: backing_track_socket_tx,
            server_event_socket_send: server_event_socket_out,
            performer_audio_sender: performer_audio_tx,
            performer_mocap_sender: performer_mocap_tx
        };

        // return server_user_data

        // oh boy I'm a little worried about stress testing this
        let mut client_channel_data = ClientChannelData {
            server_events_out: Some(server_event_out_recv),
            client_events_in: server_thread_data.client_events_in_tx.clone(),
            backing_track_in: Some(backing_track_recv),
            client_event_socket_chan: Some(client_event_socket_in),
            backing_track_socket_chan: Some(backing_track_socket_rx),
            server_event_socket_chan: Some(server_event_socket_in),
            audience_mocap_out: Some(audience_mocap_out_recv),
            from_sync_out_chan: Some(out_from_sync_rx),
            synchronizer_osc_in: None,
            synchronizer_audio_in: None,
            synchronizer_vrtp_out: Some(server_thread_data.synchronizer_to_out_tx.clone()),

        };
        // client_channel_data.synchronizer_vrtp_out =




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
                    let mut aud = performer::Performer::new_rtp(
                        server_user_data_inner.base_user_data.clone(),
                        client_channel_data,
                        ports,
                        socket,
                        performer_audio_rx,
                        performer_mocap_rx
                    ).await;
                    aud.start_main_channels().await;
                }
            };
        });

        registration_channels.subscribe(&server_user_data.user_type, our_user_id.clone(), audience_mocap_out_send, server_event_out_send, out_from_sync_tx, backing_track_send).await.unwrap();
        // registration_channels.server_event_sender.send(ListenerMessage::Subscribe(our_user_id, server_event_out_send)).await.unwrap();

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



    // todo this is copy pasted
    async fn performer_mocap_listener(
        listening_addr: &SocketAddrV4, users_by_ip: Arc<RwLock<HashMap<String, ServerUserData>>>,
    ) -> io::Result<()> {
        info!("Listening for performer mocap on {listening_addr}");

        let sock = UdpSocket::bind(listening_addr).await?;
        // let mut bytes_in;
        let mut listener_buf: [u8; LISTENER_BUF_SIZE];// = [0; LISTENER_BUF_SIZE];
        let mut last_notice = Instant::now();
        loop {
            // bytes_in = bytes::BytesMut::with_capacity(4096);
            listener_buf = [0; LISTENER_BUF_SIZE];
            let (bytes_read, incoming_addr) = match sock.recv_from(&mut listener_buf).await {
                Err(e) => {
                    error!("failed to read from listener buffer: {e}");
                    continue;
                }
                Ok(b) => b
            };

            // let bytes_in = bytes_in.freeze();
            trace!("Got {bytes_read} from {0}", &incoming_addr);

            // just forward the raw packets, do as little processing as possible
            // let datagram_data = &listener_buf[0..bytes_read];

            // this is going to be problematic.
            // If we have a read lock on this data structure that is held every single time RTP data is coming in over
            // the stream, we're going to get deadlocked FAST, and the writers (new clients being added) will cause this
            // all to slow to a slog.
            // dbg!(&users_by_ip.read().await);
            let performer_listener = match users_by_ip.read().await.get(&incoming_addr.ip().to_string()) {
                None => {
                    if (Instant::now() - last_notice).as_secs() > 5 {
                        trace!("Got performer mocap data from someone who we haven't handshaked with!");
                        last_notice = Instant::now();
                    }

                    continue;
                }
                Some(data) => {
                    if !matches!(data.user_type, UserType::Performer) {
                        warn!("An audience member tried to send mocap data to performer channel!");
                        continue;
                    }
                    data.performer_mocap_sender.clone()
                }
            };

            let datagram_data = &listener_buf[0..bytes_read];

            let pkt = match decoder::decode_udp(datagram_data) {
                Err(e) => {
                    error!("Audience mocap listener received something that doesn't seem to be OSC: {e}");
                    continue;
                },
                Ok((_, OscPacket::Message(m))) => {
                    warn!("Expected bundle, got single message in performer mocap {m:?}");
                    continue;
                },
                Ok((_, OscPacket::Bundle(bndl))) => bndl
            };

            // debug!("Echoing audience mocap!");

            // let _ = performer_listener.send(pkt).await;

            // dbg!(&pkt);

            if let Err(e) = performer_listener.send(pkt).await {
                error!("Failed to pass performer mocap data to the client: {e}");
            }
        }
    }

    async fn performer_audio_listener(
        listening_addr: &SocketAddrV4, users_by_ip: Arc<RwLock<HashMap<String, ServerUserData>>>,
    ) -> io::Result<()> {
        info!("Listening for performer audio on {listening_addr}");

        let sock = UdpSocket::bind(listening_addr).await?;
        // let mut bytes_in;
        let mut listener_buf : [u8; LISTENER_BUF_SIZE];// = [0; LISTENER_BUF_SIZE];
        let mut last_notice = Instant::now();
        loop {
            // bytes_in = bytes::BytesMut::with_capacity(4096);
            listener_buf = [0; LISTENER_BUF_SIZE];
            let (bytes_read, incoming_addr) = match sock.recv_from(&mut listener_buf).await {
                Err(e) => {
                    error!("failed to read from listener buffer: {e}");
                    continue;
                }
                Ok(b) => b
            };

            // let bytes_in = bytes_in.freeze();
            trace!("Got {bytes_read} from {0}", &incoming_addr);

            // just forward the raw packets, do as little processing as possible
            // let datagram_data = &listener_buf[0..bytes_read];

            // this is going to be problematic.
            // If we have a read lock on this data structure that is held every single time RTP data is coming in over
            // the stream, we're going to get deadlocked FAST, and the writers (new clients being added) will cause this
            // all to slow to a slog.
            // dbg!(&users_by_ip.read().await);
            let performer_listener = match users_by_ip.read().await.get(&incoming_addr.ip().to_string()) {
                None => {
                    if (Instant::now() - last_notice).as_secs() > 5 {
                        trace!("Got performer mocap data from someone who we haven't handshaked with!");
                        last_notice = Instant::now();
                    }
                    continue;
                }
                Some(data) => {
                    if !matches!(data.user_type, UserType::Performer) {
                        warn!("An audience member tried to send vocal data!");
                        continue;
                    }
                    data.performer_audio_sender.clone()
                }
            };

            let pkt = match Packet::unmarshal(&mut listener_buf.as_ref()) {
                Err(e) => {
                    error!("Audience audio listener received something that doesn't seem to be OSC: {e}");
                    continue;
                },
                Ok(r) => r
            };

            // dbg!(&pkt);

            if let Err(e) = performer_listener.send(pkt).await {
                error!("Failed to pass performer mocap data to the client: {e}");
            }

        }




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
}
