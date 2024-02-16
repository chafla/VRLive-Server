use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, RwLock};

use log::{debug, warn};
use rosc::{encoder, OscPacket};
use tokio::io::AsyncWriteExt;
use tokio::net::TcpSocket;
use tokio::sync::mpsc::{Receiver, Sender};

use protocol::{OSCEncodable, UserData};
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
        let server_port = self.ports.server_event_port;
        let user_data = self.user_data.clone();

        tokio::spawn(async move {
            Self::server_event_sender(server_event_sender.unwrap(), server_port, user_data).await
        });
    }

    /// Task responsible for sending out server events.
    async fn server_event_sender(mut sender_in: Receiver<ServerMessage>, server_event_port: u16, user_data: UserData) {
        let sock = TcpSocket::new_v4().unwrap();
        let target_addr = SocketAddr::new(user_data.remote_ip_addr, server_event_port);
        let mut stream = match sock.connect(target_addr).await {
            Ok(stream) => stream,
            Err(e) => {
                warn!("Failed to connect to remote host: {e}");
                return;
            }
        };

        loop {
            let res = sender_in.recv().await;

            // our server has been killed!
            if let None = res {
                debug!("Sender for {0} has been destroyed.", user_data.fancy_title);
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

    /// Get events from the server's main event thread, to send out to a user.
    /// This is an option as it must be taken, otherwise it will tie up the whole data structure.
    pub server_events_out: Option<Receiver<ServerMessage>>,
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
            server_events_out: Some(server_events_out),
            client_events_in,
            backing_track_in,
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