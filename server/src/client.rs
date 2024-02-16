use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, RwLock};

use log::{debug, warn};
use rosc::{encoder, OscPacket};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpSocket, TcpStream};
use tokio::select;
use tokio::sync::mpsc::{Receiver, Sender};

use protocol::{OSCDecodable, OSCEncodable, UserData};
use protocol::osc_messages_in::ClientMessage;
use protocol::osc_messages_out::ServerMessage;
use protocol::vrl_packet::VRLOSCPacket;

use crate::{AudioPacket, BackingTrackData, VRTPPacket};

pub struct AudienceMember {
    pub user_data: UserData,
    pub base_channels: ClientChannelData,
    pub ports: ClientPorts,
}

impl AudienceMember {

    pub fn get_title<'a>(&'a self) -> &'a str {
        &self.user_data.fancy_title
    }
    pub fn new(user_data: UserData, base_channels: ClientChannelData, ports: ClientPorts) -> Self {
        Self {
            user_data,
            base_channels,
            ports,
        }
    }
}

impl VRLClient for AudienceMember {
    async fn start_main_channels(mut self) {

        let server_event_sender = self.base_channels.server_events_out.take();

        if server_event_sender.is_none() {
            panic!("Server event sender was yoinked before it was needed.")
        }

        // server events
        {

            let server_port = self.ports.server_event_port;
            let user_data = self.user_data.clone();

            tokio::spawn(async move {
                Self::server_event_sender(server_event_sender.unwrap(), server_port, user_data).await
            });
        }

        {
            let client_event_chan = self.base_channels.client_events_in;
            let client_sock_chan = self.base_channels.client_event_socket_chan;
            tokio::spawn(async move {
                Self::client_event_listener(client_event_chan, client_sock_chan).await
            });
        }
    }

    /// Task responsible for sending out server events.
    async fn server_event_sender(mut sender_in: Receiver<ServerMessage>, server_event_port: u16, user_data: UserData) {
        debug!("Attempting to connect to server event channel on {0}", &user_data.fancy_title);
        let sock = TcpSocket::new_v4().unwrap();
        let target_addr = SocketAddr::new(user_data.remote_ip_addr, server_event_port);
        let mut stream = match sock.connect(target_addr).await {
            Ok(stream) => stream,
            Err(e) => {
                warn!("Failed to connect to remote host: {e}");
                return;
            }
        };

        debug!("Spinning up server event handler for {0}", &user_data.fancy_title);

        loop {
            let res = sender_in.recv().await;

            // our server has been killed!
            if let None = res {
                debug!("Sender for {0} has been destroyed.", &user_data.fancy_title);
                return;
            }

            let msg = res.unwrap().encode();
            let msg = OscPacket::Message(msg);

            let packet = encoder::encode(&msg);

            if let Err(e) = &packet {
                warn!("Failed to encode osc message {0:?}: {e}", &msg);
                continue;
            }

            let packet = packet.unwrap();

            let sent = stream.write(&packet).await;

            match sent {
                Ok(_) => {
                    debug!("Sent a packet {0:?}", &packet);

                }
                Err(e) => warn!("Failed to send packet: {e}")
            }
        }
    }

    /// Task responsible for listening for incoming client events.
    async fn client_event_listener(mut client_events_out: Sender<ClientMessage>, mut client_socket: Receiver<TcpStream>) {

        // loop: if we lose connection, we can just have the client give us a new handle.
        // unless that one's dead too.
        loop {
            let client_stream = client_socket.recv().await;
            if client_stream.is_none() {
                debug!("Client event listener shutting down.");
                break;
            }
            else {
                debug!("Client event listener is up and ready")
            }
            let mut client_stream = client_stream.unwrap();

            let mut client_event_buf: [u8; 1024];

            'connection: loop {
                client_event_buf = [0; 1024];
                let recv = client_stream.read(&mut client_event_buf).await;
                let incoming_bytes = match recv {
                    Err(e) => {
                        warn!("Client stream failed on read ({e}). May be closed?");
                        break 'connection;
                    },
                    Ok(b) => {
                        if b == 0 {
                            continue
                        }
                        b
                    }
                };
                let read_bytes = &client_event_buf[0..incoming_bytes];
                debug!("Received client event bytes: {0:?}", read_bytes);

                let res = rosc::decoder::decode_tcp_vec(read_bytes);
                if res.is_err() {
                    warn!("Client event channel got an invalid data stream!");
                    continue;
                }
                let (rest, packets) = res.unwrap();
                if !rest.is_empty() {
                    debug!("Client message had trailing data: {0:?}", rest)
                }
                let packets = packets.iter().filter_map(|pkt| {
                    match pkt {
                        OscPacket::Message(msg) => ClientMessage::from_osc_message(msg),
                        OscPacket::Bundle(_) => unimplemented!()  // maybe?
                    }
                });
                let packets = Vec::from_iter(packets.into_iter());

                for pkt in packets {
                    let _ = client_events_out.send(pkt).await;
                }

            }
            // select! {
            //
            // }
        }
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

    /// Get events from the server's main event thread, to send out to a user.
    /// This is an option as it must be taken, otherwise it will tie up the whole data structure.
    pub server_events_out: Option<Receiver<ServerMessage>>,
    /// Get backing track data to send out to our connected server.
    pub backing_track_in: Receiver<BackingTrackData>,
    /// Pass events from our client to the main server
    pub client_events_in: Sender<ClientMessage>,
    /// Get our client event socket from the server
    pub client_event_socket_chan: Receiver<TcpStream>,

    // channels dependent on the synchronizer, thus may exist depending on the type of client that we are

    // Performer clients manage their own synchronizer threads.
    // This means that they may exist, or may not if we're an audience member.
    pub synchronizer_osc_in: Option<Receiver<VRLOSCPacket>>,
    pub synchronizer_audio_in: Option<Receiver<AudioPacket>>,
    /// Sending data from the synchronizer to the server's output thread.
    pub synchronizer_vrtp_out: Option<Sender<VRTPPacket>>,
}

impl ClientChannelData {
    pub fn new(server_events_out: Receiver<ServerMessage>, client_events_in: Sender<ClientMessage>, backing_track_in: Receiver<BackingTrackData>, event_socket_chan: Receiver<TcpStream>) -> Self {
        Self {
            server_events_out: Some(server_events_out),
            client_events_in,
            backing_track_in,
            client_event_socket_chan: event_socket_chan,
            synchronizer_osc_in: None,
            synchronizer_audio_in: None,
            synchronizer_vrtp_out: None
        }
    }
}

/// Remote ports available on the client.
#[derive(Clone, Debug)]
pub struct ClientPorts {
    /// Port that server events will be sent to
    server_event_port: u16,
    /// Port that new backing tracks will be sent to
    backing_track_port: u16,
    /// Any supplemental ports that the client should be listening on.
    extra_ports: Arc<RwLock<HashMap<String, u16>>>
}

impl ClientPorts {

    pub fn new(server_event_port: u16, backing_track_port: u16, extra_ports: Option<HashMap<String, u16>>) -> Self {

        Self {
            server_event_port,
            backing_track_port,
            extra_ports: Arc::new(RwLock::new(extra_ports.unwrap_or(HashMap::new())))
        }
    }
}



/// Trait defining the necessary behavior for a client of our server.
pub trait VRLClient {

    /// Start all of the main channel tasks.
    async fn start_main_channels(self);

    /// Thread handling output for any server events.
    async fn server_event_sender(sender_in: Receiver<ServerMessage>, server_event_port: u16, user_data: UserData);

    /// Thread handling input for any client events.
    /// This will become the new "main" thread for the server keeping it alive.
    async fn client_event_listener(client_events_out: Sender<ClientMessage>, client_socket: Receiver<TcpStream>);

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