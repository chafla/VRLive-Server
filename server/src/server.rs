// use std::sync::mpsc::Receiver;

use std::borrow::Cow;
use std::collections::{HashMap};
use std::fmt::Debug;
use std::io;
use std::net::{SocketAddr, SocketAddrV4};
use std::sync::Arc;
use std::sync::atomic::{AtomicU16, Ordering};
use std::time::Instant;
use bytes::Bytes;

use log::{debug, error, info, trace, warn};
use rosc::{decoder, OscBundle, OscPacket};
use rustyline_async;
use rustyline_async::Readline;
use rustyline_async::ReadlineEvent::{Eof, Interrupted, Line};
use simple_moving_average::{SingleSumSMA, SMA};
// use serde_json::Result;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream, UdpSocket};
use tokio::sync::{mpsc::{Receiver, Sender}, Mutex, RwLock};
use tokio::sync::mpsc::channel;
use webrtc::rtp::packet::Packet;
use webrtc::util::Unmarshal;

use protocol::{osc_messages_out::ServerMessage, UserData, UserIDType};
use protocol::backing_track::BackingTrackData;
use protocol::handshake::{AdditionalUser, HandshakeAck, HandshakeCompletion, HandshakeSynAck, ServerPortMap};
use protocol::osc_messages_out::{BackingMessage, PerformerServerMessage, StatusMessage};
use protocol::UserType;
use protocol::vrl_packet::{OscData, VRTPPacket};
use crate::analytics::{AnalyticsData, ThroughputAnalytics};

use crate::client::{ClientChannelData, VRLClient};
use crate::client::audience::AudienceMember;
use crate::client::performer;
use crate::{ClientMessageChannelType, MAX_CHAN_SIZE};
use crate::performance::PerformanceState;

// use crate::client::synchronizer::OscData;

const DEFAULT_CHAN_SIZE: usize = 2048;

const HANDSHAKE_BUF_SIZE: usize = DEFAULT_CHAN_SIZE;
/// Corresponds to the amount of audio data that we will pull over the network for each packet
const AUDIO_LISTENER_BUF_SIZE: usize = 10000;

const MOCAP_LISTENER_BUF_SIZE: usize = 10000;



/// Data stored for each client.
#[derive(Debug, Clone)]
pub struct ServerUserData {
    user_type: UserType,
    base_user_data: UserData,

    // Input channels for clients.
    // These senders correspond to receivers on client threads that forward data onto synchronizers.
    // Note that these channels will be closed for audience members.
    performer_audio_sender: Sender<Packet>,
    performer_mocap_sender: Sender<OscBundle>,


    // Socket channels.
    // These are used by server listeners which accept TCP connections and then pass them onto client listeners,
    // which then become responsible for managing the streams.
    // Note that these may be sent to multiple times, particularly in the case of a re-connection.
    // If another connection comes through from this, then any existing connections should be discarded for the sake of this one.

    /// Send a connected socket on this channel when a new client event channel is established.
    client_event_socket_send: Sender<TcpStream>,
    /// Send a connected socket on this channel when a new backing track connection is established
    backing_track_socket_send: Sender<TcpStream>,
    /// Send a connected socket on this channel when a new server event channel is established.
    server_event_socket_send: Sender<TcpStream>,
    /// Send a connected socket on this channel when a new heartbeat channel is established.
    heartbeat_socket_send: Sender<TcpStream>,
}


/// A subset of the data available on the server which can be passed through threads.
/// This data should more or less be constant, or at least thread safe.
#[derive(Clone, Debug)]
pub struct ServerThreadData {
    /// The host of the server.
    host: String,
    /// The ports available on the server.
    ports: ServerPortMap,
    /// The channel that should be cloned by everyone looking to send something to the
    synchronizer_to_out_tx: Sender<VRTPPacket>,
    client_events_in_tx: Sender<ClientMessageChannelType>,
    /// The current minimum user ID.
    cur_user_id: Arc<Mutex<AtomicU16>>,
    /// Current set of users.
    clients_by_ip: Arc<RwLock<HashMap<String, ServerUserData>>>,
    extra_ports: Arc<HashMap<String, u16>>,
    /// Clients
    clients_by_id: Arc<RwLock<HashMap<UserIDType, ServerUserData>>>,
    late_channels: Option<Arc<ServerRelayChannels<UserIDType>>>,
}

pub struct Server {
    host: String,
    /// Map storing the ports we will be using
    port_map: ServerPortMap,
    /// Any additional ports that we might be requesting.
    /// Stored separately from the port map so it can stay as copy
    extra_ports: Arc<HashMap<String, u16>>,
    /// Clients
    clients: Arc<RwLock<HashMap<UserIDType, ServerUserData>>>,
    /// Separate association associating them by IP rather than their ID
    clients_by_ip: Arc<RwLock<HashMap<String, ServerUserData>>>,
    /// Our current lowest user ID, used as a canonical base for adding more.
    cur_user_id: Arc<Mutex<AtomicU16>>,

    // These channels are here because they need to be cloned by all new clients as common channels.
    /// Output channel coming from each synchronizer and going to the high-priority output thread.
    synchronizer_to_out_tx: Sender<VRTPPacket>,
    /// Hidden internal channel that receives the message inside the output thread
    synchronizer_to_out_rx: Option<Receiver<VRTPPacket>>,

    /// Client events should be sent from their constituent threads through this
    client_event_in_tx: Sender<ClientMessageChannelType>,
    /// Input channel for client events, which should be managed by a global event thread
    client_event_in_rx: Option<Receiver<ClientMessageChannelType>>,

    /// Server event handlers.
    server_event_tx: Sender<ServerMessage>,
    server_event_rx: Option<Receiver<ServerMessage>>,
    /// Mocap handlers.
    general_mocap_tx: Sender<OscData>,
    general_mocap_rx: Option<Receiver<OscData>>,

    /// Backing track handlers
    backing_track_tx: Sender<BackingTrackData>,
    backing_track_rx: Option<Receiver<BackingTrackData>>,
    late_chans: Option<Arc<ServerRelayChannels<UserIDType>>>,

    performance_data: Arc<Mutex<PerformanceState>>,

    analytics_tx: Sender<AnalyticsData>,
    analytics_rx: Option<Receiver<AnalyticsData>>

    // due to how mpsc channels work, it's probably easier to include the music channel in 
}

#[derive(Clone, Debug)]
/// A message to be sent to a server listener, containing some kind of identifier for a new user (U)
/// as well as a type to be sent across the channel for this user (T).
pub enum ListenerMessage<U, T>
    where
    // Represents a user, or some kind of identifier.
        U: PartialEq + Sync + Send + Clone,
    // Represents the type of data that will be sent across the channel for these listeners.
        T: Sync + Send
{
    /// A message of this type indicates that the user is subscribing to the channel.
    /// Includes both their identifier, as well as the sending side of a channel which the server can use to dispatch events to their thread.
    Subscribe(U, Sender<T>),
    /// A message of this kind indicates that the user should be unsubscribed from the channel, and their listener should
    /// not be sent to anymore.
    Unsubscribe(U),
}

pub enum HandshakeResult {
    NewConnection(UserIDType, ServerUserData),
    Reconnection(UserIDType, ServerUserData),
}

/// A thread-local struct useful for managing large amounts of listeners without needing to refer to some external, mutexed data structure.

/// The goal behind this whole module setup is that we want servers to be able to dispatch commands in a sort of broadcast-channel
/// kind of way, but without actually using broadcast channels. This allows everyone to have a queue that can be nicely cleaned up
/// and/or restored when connection is lost or gained. It is also not subject to delays from locks, since users can be cleanly
/// added and removed without needing to check a giant mutexed data structure on each iteration.
#[derive(Debug)]
struct Listener<T, U>
    where
        U: PartialEq + Sync + Send + Clone,
        T: Sync + Send
{
    /// Vector holding onto users and their associated channels for sending data.
    /// This is a vector and not a hashmap because we need iteration to be lightning-fast.
    /// We can afford a little bit of time complexity loss on insertion/deletion since those events are relatively infrequent.
    client_channels: Vec<(U, Sender<T>)>,
    /// Channel for receiving updates on users.
    user_update_channel: Receiver<ListenerMessage<U, T>>,
}

impl<T, U> Listener<T, U>
    where
        U: PartialEq + Sync + Send + Clone,
        T: Sync + Send
{
    pub fn new(user_update_channel: Receiver<ListenerMessage<U, T>>) -> Self {
        Self {
            client_channels: vec![],
            user_update_channel,
        }
    }

    /// Insert a new user.
    #[allow(dead_code)]
    pub fn insert(&mut self, user: U, sender: Sender<T>) {
        self.client_channels.push((user, sender));
    }

    /// Remove a user, getting back the user if they were found in the list.
    pub fn remove(&mut self, user: &U) -> Option<(U, Sender<T>)> {
        match self.client_channels.iter().position(|(u, _)| u == user) {
            Some(p) => Some(self.client_channels.remove(p)),
            None => None
        }
    }
}


/// Internal server relay channels.
#[derive(Clone, Debug)]
struct ServerRelayChannels<T>
    where T: Clone + Debug + PartialEq + Send + Sync
{
    mocap_sender: Sender<ListenerMessage<T, OscData>>,
    server_event_sender: Sender<ListenerMessage<T, ServerMessage>>,
    sync_sender: Sender<ListenerMessage<T, VRTPPacket>>,
    backing_track_sender: Sender<ListenerMessage<T, BackingTrackData>>,
}

impl<T> ServerRelayChannels<T>
    where T: Clone + Debug + PartialEq + Send + Sync + 'static
{
    pub async fn subscribe(&self, identifier: T, osc_sender: Sender<OscData>, server_event_sender: Sender<ServerMessage>, sync_sender: Sender<VRTPPacket>, backing_track_sender: Sender<BackingTrackData>) -> anyhow::Result<()> {
        self.mocap_sender.send(ListenerMessage::Subscribe(identifier.clone(), osc_sender)).await?;
        self.server_event_sender.send(ListenerMessage::Subscribe(identifier.clone(), server_event_sender)).await?;
        self.backing_track_sender.send(ListenerMessage::Subscribe(identifier.clone(), backing_track_sender)).await?;
        // if matches!(user_type, UserType::Performer) {
        self.sync_sender.send(ListenerMessage::Subscribe(identifier.clone(), sync_sender)).await?;
        // }
        Ok(())
    }

    pub async fn unsubscribe(&self, identifier: T) -> anyhow::Result<()> {
        // these can't just be cloned because they monomorphize down
        self.mocap_sender.send(ListenerMessage::Unsubscribe(identifier.clone())).await?;
        self.server_event_sender.send(ListenerMessage::Unsubscribe(identifier.clone())).await?;
        self.backing_track_sender.send(ListenerMessage::Unsubscribe(identifier.clone())).await?;
        // if matches!(user_type, UserType::Performer) {
        self.sync_sender.send(ListenerMessage::Unsubscribe(identifier.clone())).await?;
        // }
        Ok(())
    }
}

impl Server {
    pub fn new(host: String, port_map: ServerPortMap) -> Self {
        let (sync_out_tx, sync_out_rx) = channel::<VRTPPacket>(MAX_CHAN_SIZE);
        let (client_in_tx, client_in_rx) = channel::<ClientMessageChannelType>(MAX_CHAN_SIZE);
        let (server_event_tx, server_event_rx) = channel::<ServerMessage>(MAX_CHAN_SIZE);
        let (base_mocap_tx, base_mocap_rx) = channel(MAX_CHAN_SIZE);
        let (backing_track_tx, backing_track_rx) = channel(MAX_CHAN_SIZE);
        let (analytics_tx, analytics_rx) = channel(MAX_CHAN_SIZE);


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
            cur_user_id: Arc::new(Mutex::new(AtomicU16::new(0))),
            extra_ports: Arc::new(HashMap::new()),
            general_mocap_tx: base_mocap_tx,
            general_mocap_rx: Some(base_mocap_rx),
            backing_track_tx,
            backing_track_rx: Some(backing_track_rx),
            late_chans: None,
            analytics_rx: Some(analytics_rx),
            analytics_tx,
            performance_data: Arc::new(Mutex::new(PerformanceState::default())),
        }
    }
    

    pub async fn main_server_loop(&mut self) {

        // ensure the writer doesn't get dropped
        let (mut readline, _writer) = Readline::new("> ".into()).unwrap();
        loop {
            let line = match readline.readline().await {
                Err(e) => {
                    println!("Failed to read line: {e}");
                    continue;
                }
                Ok(Eof | Interrupted) => break,
                Ok(Line(str)) => str
            };

            let (start, rest) = match line.split_once(" ") {
                Some((x, y)) => (x, Some(y)),
                None => (line.as_str(), None)
            };

            let valid_commands = vec!["help", "backing", "message"];

            match (start, rest) {
                ("help" | "h", _) => {
                    println!("Possible commands are: {}", valid_commands.join(", "))
                }
                ("backing" | "b", Some(path)) => {
                    let path = match path {
                        "default" | "d" => "utils\\howd_i_wind_up_here.mp3",
                        _ => path
                    };
                    self.change_backing_track(path).await;
                }
                ("reset", _) => {
                    println!("Resetting server, dropping all listeners and zeroing things out.")
                }
                ("message" | "msg" | "m", Some(msg)) => {
                    match msg {
                        "ready" => self.server_event_tx.send(ServerMessage::Performer(PerformerServerMessage::Ready(true))).await.unwrap(),
                        "backing" => {
                            let perf_data = self.performance_data.lock().await;
                            match perf_data.backing_track() {
                                None => warn!("No backing track loaded! Try sending one first."),
                                Some(st) => {
                                    let msg = if st.is_playing() {
                                        BackingMessage::Stop
                                    } else {
                                        BackingMessage::Start(0)
                                    };
                                    // ensure we're not still holding onto this lock
                                    drop(perf_data);
                                    if let Err(e) = self.set_backing_track_state(msg).await {
                                        warn!("Failed to adjust backing track state: {e}")
                                    }
                                }
                            }
                        }
                        _ => warn!("Unknown server message '{msg}'"),
                    }
                }
                _ => (),
            }
        }

        warn!("Execution was interrupted by user!")
    }

    /// Update the backing track.
    pub async fn change_backing_track(&mut self, path: &str) {
        match BackingTrackData::open(path).await {
            Err(e) => {
                error!("Failed to load backing track at {path}: {e}");
            }
            Ok(data) => {
                info!("Sending backing track to client(s)");
                let mut perf_data = self.performance_data.lock().await;
                perf_data.update_backing_track(path);
                self.backing_track_tx.send(data).await.unwrap()
            }
        }
    }

    pub async fn set_backing_track_state(&self, msg: BackingMessage) -> Result<(), &str> {
        let mut pd = self.performance_data.lock().await;
        match pd.backing_track_mut() {
            None => return Err("No backing track has been selected yet!"),
            Some(bt) => {
                match (bt.is_playing(), &msg) {
                    (true, BackingMessage::Start(_)) | (false, BackingMessage::Stop) => return Err("Track is already in this state."),
                    (b, _) => {
                        // we're reversing it here since they're not equal
                        bt.mark_playing(!b);
                        warn!("Updating backing track to be {}", if b {"stopped"} else {"playing"});
                    }
                };

                self.server_event_tx.send(ServerMessage::Backing(msg)).await.unwrap();
            }
        };
        Ok(())
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

        let analytics_chan = self.analytics_rx.take().expect("Analytics receiver should not have been consumed yet!");
        tokio::spawn(async move {
            Self::analytics_listener(analytics_chan).await;
        });

        self.late_chans = Some(Arc::new(ServerRelayChannels {
            mocap_sender: mocap_chan,
            sync_sender: sync_chan,
            server_event_sender: event_chan,
            backing_track_sender: backing_track_chan,
        }));

        self.new_connection_listener(Arc::clone(&self.late_chans.as_ref().unwrap())).await.unwrap();
        self.start_performer_audio_listener().await;
        self.start_performer_mocap_listener().await;
        self.start_client_event_listener().await;

        self.main_server_loop().await;
        Ok(())
    }

    /// Drop a user entirely, removing all of their data and unsubscribing them from everything.
    async fn drop_user(user_id: UserIDType, server_thread_data: &ServerThreadData) {
        let uid_ref = user_id.clone();

        // remove them from our global data structures so we can drop their channels, letting them clean up nicely

        let mut write_lock = server_thread_data.clients_by_id.write().await;
        let user_data = write_lock.remove(&uid_ref);

        if user_data.is_none() {
            warn!("Tried to drop user {user_id}, but it looks like they don't exist in our clients!");
            return;
        }
        let user_data = user_data.unwrap();

        drop(write_lock);

        // use a block so we can just silently throw away the lock and the data structure in the map
        // once they fall out of scope
        {
            // remove them from the other array as well
            let mut write_lock = server_thread_data.clients_by_ip.write().await;
            let res = write_lock.remove(&user_data.base_user_data.remote_ip_addr.to_string());

            if res.is_none() {
                warn!("User {user_id} was found in clients, but not clients_by_ip! Something's probably pretty wrong, and we might not be able to fully clean up after them.")
            }
        }

        // drop(read_lock);
        if let Some(c) = &server_thread_data.late_channels {
            // make sure they're removed from the listener channels
            if let Err(e) = c.unsubscribe(user_id).await {
                error!("Something went wrong when unsubscribing channels for {user_id}: {e}");
            }
        }

        drop(user_data);
    }

    /// The main function that you'll want to call to start up an internal message router.
    pub async fn start_handler<T, U>(&self, event_channel: Receiver<T>, label: &'static str) -> Sender<ListenerMessage<U, T>>
        where
            T: Sync + Send + Debug + Clone + 'static,
            U: Sync + Send + PartialEq + Debug + Clone + 'static
    {
        let (send, recv) = channel(DEFAULT_CHAN_SIZE);
        debug!("Starting up server internal message router for {label}");
        tokio::spawn(async move {
            Self::server_relay(label, recv, event_channel).await
        });

        send
    }

    async fn start_audience_mocap_listeners(&self, mocap_send: Sender<OscData>) {
        let mocap_in_addr = SocketAddrV4::new("0.0.0.0".parse().unwrap(), self.port_map.audience_mocap_in);
        let clients_by_ip = Arc::clone(&self.clients_by_ip);
        tokio::spawn(async move {
            Self::audience_mocap_listener(&mocap_in_addr, mocap_send, clients_by_ip).await;
        });
    }

    async fn start_performer_audio_listener(&self) {
        let audio_in_addr = SocketAddrV4::new("0.0.0.0".parse().unwrap(), self.port_map.performer_audio_in);
        let users_by_ip = Arc::clone(&self.clients_by_ip);
        tokio::spawn(async move {
            Self::performer_audio_listener(&audio_in_addr, users_by_ip).await.unwrap();
        });
    }

    async fn start_performer_mocap_listener(&self) {
        let audio_in_addr = SocketAddrV4::new("0.0.0.0".parse().unwrap(), self.port_map.performer_mocap_in);
        let users_by_ip = Arc::clone(&self.clients_by_ip);
        let analytics_chan = self.analytics_tx.clone();
        tokio::spawn(async move {
            Self::performer_mocap_listener(&audio_in_addr, users_by_ip, analytics_chan).await.unwrap();
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
    /// These will listen for inbound connections on a given port, and then pass along the TCP socket to the client.
    async fn start_client_listeners(&self) {
        self.start_listener(
            "client event",
            self.port_map.client_event_conn_port,
            |user_data: &ServerUserData| user_data.client_event_socket_send.clone(),
        ).await;

        self.start_listener(
            "backing track",
            self.port_map.backing_track_conn_port,
            |user_data: &ServerUserData| user_data.backing_track_socket_send.clone(),
        ).await;


        // Yes, this does need to be here, since this is about establishing the socket connection.
        self.start_listener(
            "server event in",
            self.port_map.server_event_conn_port,
            |user_data: &ServerUserData| user_data.server_event_socket_send.clone(),
        ).await;
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
        debug!("Internal server listener for {label} has started.");
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

    /// Listener waiting for new incoming connections.
    async fn new_connection_listener(&self, late_channels: Arc<ServerRelayChannels<UserIDType>>) -> io::Result<()> {
        let addr = format!("{0}:{1}", self.host, self.port_map.new_connections);
        let listener = TcpListener::bind(&addr).await?;

        let clients = Arc::clone(&self.clients);
        let clients_by_ip = Arc::clone(&self.clients_by_ip);
        info!("Listening for new connections on {addr}");
        let thread_data = self.create_thread_data();
        // let late_channels = Arc::clone(&late_channels);

        let server_event_sender = self.server_event_tx.clone();
        let analytic_sender = self.analytics_tx.clone();

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
                // let server_event_sender_inner = server_event_sender_inner.clone();
                // todo this should probably come out of create_server_data, find a good way to replicate that
                let thread_data = thread_data.clone();
                let clients_local = Arc::clone(&clients);
                let clients_by_ip_local = Arc::clone(&clients_by_ip);

                info!("Got a new connection from {incoming_addr}");
                // this kinda really sucks LOL
                let server_event_sender_inner_inner = server_event_sender_inner.clone();
                let analytic_sender_inner_inner = analytic_sender.clone();

                tokio::spawn(async move {
                    // if let Some(v) = clients_by_ip_local.read().await.get(&incoming_addr.ip().to_string()) {
                    //     // this looks like a reconnection.
                    //     // TODO purge serveruserdata on a disconnection so it forces a full reconnect when they choose to disconnect
                    //     Self::handle_reconnect(v).await;
                    //     info!("{incoming_addr} has reconnected.");
                    //     return;
                    // }
                    let chans = Arc::clone(&late_channels_outer);
                    let res = Self::perform_handshake(socket, incoming_addr, thread_data, chans, analytic_sender_inner_inner.clone()).await;

                    match res {
                        Err(e) => {
                            error!("Attempted handshake with {0} failed: {e}", incoming_addr);
                            return;
                        }
                        Ok(HandshakeResult::NewConnection(user_id, server_data)) => {
                            // let user_type = server_data.user_type.clone();
                            let new_user_event = ServerMessage::Status(StatusMessage::UserAdd(user_id, server_data.user_type));
                            server_event_sender_inner_inner.send(new_user_event).await.unwrap();
                            let mut write_handle = clients_local.write().await;
                            write_handle.insert(user_id, server_data.clone());

                            let mut write_handle = clients_by_ip_local.write().await;
                            write_handle.insert(server_data.base_user_data.remote_ip_addr.to_string(), server_data);

                            // inform folks that a new user has joined the fray
                            // TODO finally make use of heartbeat for a disconnect event
                            // TODO COME UP WITH A WAY TO SEND ALL EXISTING USER IDS WHEN NEEDED
                        }
                        Ok(HandshakeResult::Reconnection(user_id, user_data)) => {
                            dbg!(&user_data.user_type);
                            info!("User {user_id} has reconnected!");
                            warn!("Note that this should raise UserReconnect, but doesn't for the sake of MVP!");
                            let new_user_event = ServerMessage::Status(StatusMessage::UserAdd(user_id, user_data.user_type));
                            server_event_sender_inner_inner.send(new_user_event).await.unwrap();
                        }
                    }
                });
            }
        });

        Ok(())
    }

    async fn start_client_event_listener(&mut self) {
        let listener_channel = self.client_event_in_rx.take().expect("Client event receiver was consumed before the server could use it!");
        let users_by_id_clone = Arc::clone(&self.clients);
        
        tokio::spawn(async move {
            Self::client_event_listener(listener_channel, users_by_id_clone).await;
        });
    }
    
    async fn client_event_listener(mut listener_channel: Receiver<ClientMessageChannelType>, users_by_id: Arc<RwLock<HashMap<UserIDType, ServerUserData>>>) {
        debug!("Server's client event listener is ready!");
        loop {
            let (user_id, msg) = match listener_channel.recv().await {
                Some(msg) => msg,
                None => break
            };
            
            let user_lock = users_by_id.read().await;
            let user = match user_lock.get(&user_id) {
                Some(u) => u,
                None => {
                    warn!("Got a client message from someone who doesn't seem to be tracked, user id {user_id}");
                    continue;
                }
            };
            info!("Server received client message {:?} from user {user_id} with type {:?}", msg, user.user_type);
            warn!("We currently don't have any handling for client messages!")
        }
        
        
    }

    /// Take in motion capture from audience members and sent it out as soon as we can.
    /// Audience members don't really need to worry about sending to specific "clients",
    /// instead they can just forward their data on here, and it will be repeated to any audience members listening.
    async fn audience_mocap_listener(socket_addr: &SocketAddrV4, listener_channel: Sender<OscData>, users_by_ip: Arc<RwLock<HashMap<String, ServerUserData>>>) {
        // https://github.com/henninglive/tokio-udp-multicast-chat/blob/master/src/main.rs

        // assert!(multicast_socket_addr.ip().is_multicast(), "Must be multcast address");

        info!("Listening for audience mocap on {0}", &socket_addr);
        let sock = UdpSocket::bind(&socket_addr).await.unwrap();

        let mut listener_buf: [u8; MOCAP_LISTENER_BUF_SIZE] = [0; MOCAP_LISTENER_BUF_SIZE];

        loop {
            let (bytes_read, addr_in) = match sock.recv_from(&mut listener_buf).await {
                Err(e) => {
                    error!("failed to read from listener buffer: {e}");
                    continue;
                }

                Ok(b) => b
            };

            // just forward the raw packets, do as little processing as possible
            let datagram_data = &listener_buf[0..bytes_read];

            let user_id = {
                let lock = &users_by_ip.read().await;
                let audience_member_client = lock.get(&addr_in.ip().to_string());
                match audience_member_client {
                    None => {
                        continue;
                    }
                    Some(d) => d.base_user_data.participant_id
                }
            };


            let pkt = match decoder::decode_udp(datagram_data) {
                Err(e) => {
                    error!("Audience mocap listener received something that doesn't seem to be OSC: {e}");
                    continue;
                }
                Ok((_, OscPacket::Message(msg))) => {
                    warn!("Mocap listener expected bundle got message {msg:?}");
                    continue;
                }
                Ok((_, OscPacket::Bundle(b))) => b
            };
            
            let osc_data = OscData::new(pkt, user_id);

            let _ = listener_channel.send(osc_data).await;
        }
    }

    /// Perform the handshake. If this completes, it should add a new client.
    /// If this also succeeds, then it should spin up the necessary threads for listening
    async fn perform_handshake(mut socket: TcpStream, addr: SocketAddr, server_thread_data: ServerThreadData, registration_channels: Arc<ServerRelayChannels<UserIDType>>, analytics_sender: Sender<AnalyticsData>) -> Result<HandshakeResult, String> {
        let our_user_id;
        let mut existing_user_type: Option<UserType> = None;
        let mut handshake_buf: [u8; HANDSHAKE_BUF_SIZE] = [0; HANDSHAKE_BUF_SIZE];

        // check to see if this is reconnected.
        {
            // scope block so we drop this lock when we're done.
            let lock = server_thread_data.clients_by_ip.read().await;
            let existing_user_data = lock.get(&addr.ip().to_string());
            match existing_user_data {
                Some(data) => {
                    // if they're reconnecting, use their same ID to get everything lined up as it should be.
                    our_user_id = data.base_user_data.participant_id;
                    existing_user_type = Some(data.user_type);
                }
                None => {
                    let user_id = server_thread_data.cur_user_id.lock().await;
                    our_user_id = user_id.fetch_add(1, Ordering::Relaxed);
                }
            };
        }

        // lock the user ID and increment it
        // if we don't complete the handshake it's okay, we will just need this ID when we're done

        let handshake_start = HandshakeAck::new(our_user_id, server_thread_data.host.clone(), None);

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

        // I don't understand how a heart is a spade
        // but somehow the vital connection is made

        let user_data = UserData {
            participant_id: our_user_id,
            remote_ip_addr: addr.ip(),
            fancy_title: synack.own_identifier,
        };

        // for reconnections, we still want to allow the handshake to complete.
        // we just don't want to have to re-create our existing data structures.

        if let Some(user_type) = existing_user_type {
            if user_type != synack.user_type.into() {
                // they're changing their user type, oh no
                // re-initialize everything
                warn!("User {our_user_id} has changed from {:?} to {:?}, clearing their stuff.", user_type, UserType::from(synack.user_type));
                Self::drop_user(our_user_id, &server_thread_data).await;
            }
        }

        // even if they're reconnecting, we need to handshake
        
        let mut all_other_users = vec![];
        // gather all of the other users
        {
            let user_lock = server_thread_data.clients_by_ip.read().await;
            for (_, data) in user_lock.iter() {
                if data.base_user_data.participant_id == our_user_id {
                    warn!("Found ourselves in the user ID list!");
                }
                all_other_users.push(AdditionalUser {
                    user_id: data.base_user_data.participant_id,
                    user_type: data.user_type.into(),
                });
            }
        }
        
        // send out our last part of the handshake
        let handshake_finish = HandshakeCompletion {
            extra_ports: server_thread_data.extra_ports.as_ref().clone(),
            other_users: all_other_users,
        };

        let handshake_finish_msg = serde_json::to_string(&handshake_finish).unwrap();
        let _ = socket.write_all(handshake_finish_msg.as_bytes()).await;

        if let Some(user_type) = existing_user_type {
            if user_type == synack.user_type.into() {
                // they're reconnecting as the same kind of user, don't change anything
                let lock = &server_thread_data.clients_by_id.read().await;
                let user_data = lock.get(&our_user_id).unwrap();
                // dbg!(&user_data);
                return Ok(HandshakeResult::Reconnection(our_user_id, user_data.clone()));
            }
        }


        let (backing_track_send, backing_track_recv) = channel::<BackingTrackData>(MAX_CHAN_SIZE);
        let (server_event_out_send, server_event_out_recv) = channel::<ServerMessage>(MAX_CHAN_SIZE);
        let (audience_mocap_out_send, audience_mocap_out_recv) = channel::<OscData>(MAX_CHAN_SIZE);
        let (out_from_sync_tx, out_from_sync_rx) = channel::<VRTPPacket>(MAX_CHAN_SIZE);

        let (client_event_socket_out, client_event_socket_in) = channel::<TcpStream>(MAX_CHAN_SIZE);
        let (backing_track_socket_tx, backing_track_socket_rx) = channel::<TcpStream>(MAX_CHAN_SIZE);
        let (server_event_socket_out, server_event_socket_in) = channel::<TcpStream>(MAX_CHAN_SIZE);
        let (heartbeat_socket_out, heartbeat_socket_in) = channel::<TcpStream>(MAX_CHAN_SIZE);

        let (performer_audio_tx, performer_audio_rx) = channel(MAX_CHAN_SIZE);
        let (performer_mocap_tx, performer_mocap_rx) = channel(MAX_CHAN_SIZE);


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
            heartbeat_socket_send: heartbeat_socket_out,
            performer_audio_sender: performer_audio_tx,
            performer_mocap_sender: performer_mocap_tx,
        };

        // return server_user_data

        // oh boy I'm a little worried about stress testing this
        let client_channel_data = ClientChannelData {
            server_events_out: Some(server_event_out_recv),
            client_events_in: server_thread_data.client_events_in_tx.clone(),
            backing_track_in: Some(backing_track_recv),
            client_event_socket_chan: Some(client_event_socket_in),
            backing_track_socket_chan: Some(backing_track_socket_rx),
            server_event_socket_chan: Some(server_event_socket_in),
            heartbeat_socket_chan: Some(heartbeat_socket_in),
            audience_mocap_out: Some(audience_mocap_out_recv),
            from_sync_out_chan: Some(out_from_sync_rx),
            synchronizer_vrtp_out: Some(server_thread_data.synchronizer_to_out_tx.clone()),

        };

        let server_user_data_inner = server_user_data.clone();
        // pass off the client to handle themselves
        // I want to make this trait-based, but it seems like the types in use (particularly with async?) cause some kinda recursion in the type checker
        let new_analytics_sender = analytics_sender.clone();
        let _ = tokio::spawn(async move {
            match server_user_data_inner.user_type {
                UserType::Audience => {
                    let mut mem = AudienceMember::new(
                        server_user_data_inner.base_user_data.clone(),
                        client_channel_data,
                        synack.ports,
                        new_analytics_sender
                    );
                    mem.start_main_channels().await;
                }
                UserType::Performer => {
                    let mut aud = performer::Performer::new_rtp(
                        server_user_data_inner.base_user_data.clone(),
                        client_channel_data,
                        synack.ports,
                        performer_audio_rx,
                        performer_mocap_rx,
                        new_analytics_sender
                    ).await;
                    aud.start_main_channels().await;
                }
            };
        });

        // Register the server listeners
        registration_channels.subscribe(our_user_id.clone(), audience_mocap_out_send, server_event_out_send, out_from_sync_tx, backing_track_send).await.unwrap();

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
        Ok(HandshakeResult::NewConnection(our_user_id, server_user_data))
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
            } else {
                info!("{listener_label} connection established for {0}", &incoming_addr);
            }
            let found_user = found_user.unwrap();
            let _ = channel_getter(found_user).send(socket).await;
        }
    }

    async fn performer_mocap_listener(
        listening_addr: &SocketAddrV4, users_by_ip: Arc<RwLock<HashMap<String, ServerUserData>>>, analytics_channel: Sender<AnalyticsData>
    ) -> io::Result<()> {
        info!("Listening for performer mocap on {listening_addr}");

        let sock = UdpSocket::bind(listening_addr).await?;
        // let mut bytes_in;
        let mut listener_buf: [u8; MOCAP_LISTENER_BUF_SIZE];// = [0; LISTENER_BUF_SIZE];
        let mut last_notice = Instant::now();
        loop {
            // bytes_in = bytes::BytesMut::with_capacity(4096);
            listener_buf = [0; MOCAP_LISTENER_BUF_SIZE];
            let (bytes_read, incoming_addr) = match sock.recv_from(&mut listener_buf).await {
                Err(e) => {
                    error!("failed to read from listener buffer: {e}");
                    continue;
                }
                Ok(b) => b
            };

            // let bytes_in = bytes_in.freeze();
            trace!("Got {bytes_read} from {0}", &incoming_addr);

            // try, but don't be too upset if it doesn't work
            let _ = analytics_channel.send(AnalyticsData::Throughput(ThroughputAnalytics::PerformerOSCBytesIn(bytes_read))).await;

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
                        debug!("Got performer mocap data from someone who we haven't handshaked with!");
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
                }
                Ok((_, OscPacket::Message(m))) => {
                    warn!("Expected bundle, got single message in performer mocap {m:?}");
                    continue;
                }
                Ok((_, OscPacket::Bundle(bndl))) => bndl
            };

            if let Err(e) = performer_listener.send(pkt).await {
                error!("Failed to pass performer mocap data to the client: {e}");
            }
        }
    }

    async fn analytics_listener(mut chan_in: Receiver<AnalyticsData>) {
        debug!("Spinning up analytics listener.");
        
        let mut performer_bytes_in = SingleSumSMA::<_, usize, 100>::new();
        
        let notification_window_ms = 10000;
        let mut last_notification = Instant::now();

        loop {
            match chan_in.recv().await {
                None => break,
                Some(msg) => {
                    match msg {
                        AnalyticsData::Throughput(i) => match i {
                            ThroughputAnalytics::PerformerOSCBytesIn(b) => performer_bytes_in.add_sample(b),
                            _ => unimplemented!()
                        },
                        _ => unimplemented!()
                    }
                }
            }
            
            if Instant::now().duration_since(last_notification).as_millis() > notification_window_ms {
                // notify!
                debug!("Analytics (over last {} seconds):", notification_window_ms / 1000);
                debug!("Average performer mocap bytes: {}", performer_bytes_in.get_average());
                
                last_notification = Instant::now();
            }
        }

        debug!("Analytics listener shutting down.")
    }


    async fn performer_audio_listener(
        listening_addr: &SocketAddrV4, users_by_ip: Arc<RwLock<HashMap<String, ServerUserData>>>,
    ) -> io::Result<()> {
        info!("Listening for performer audio on {listening_addr}");

        let sock = UdpSocket::bind(listening_addr).await?;
        let mut listener_buf: [u8; AUDIO_LISTENER_BUF_SIZE];
        let mut last_notice = Instant::now();
        loop {
            listener_buf = [0; AUDIO_LISTENER_BUF_SIZE];
            let (bytes_read, incoming_addr) = match sock.recv_from(&mut listener_buf).await {
                Err(e) => {
                    error!("failed to read from listener buffer: {e}");
                    continue;
                }
                Ok(b) => b
            };

            trace!("Got {bytes_read} audio bytes from {0}", &incoming_addr);
            
            let performer_listener = match users_by_ip.read().await.get(&incoming_addr.ip().to_string()) {
                None => {
                    if (Instant::now() - last_notice).as_secs() > 5 {
                        warn!("Got performer audio data from someone who we haven't handshaked with {}!", incoming_addr);
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

            // pull the read data into a new buffer, ensuring that we trim out any padding/additional bytes
            // I've tried to just pull this into a regular bytesmut but it causes network issues with recv
            let mut bytes_data = Bytes::copy_from_slice(&listener_buf[0..bytes_read]);

            let pkt = match Packet::unmarshal(&mut bytes_data) {
                Err(e) => {
                    error!("Audience audio listener received something that doesn't seem to be OSC: {e}");
                    continue;
                }
                Ok(r) => r
            };

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
            clients_by_ip: Arc::clone(&self.clients_by_ip),
            clients_by_id: Arc::clone(&self.clients),
            late_channels: match &self.late_chans {
                Some(c) => Some(Arc::clone(c)),
                None => None
            },
        }
    }
}
