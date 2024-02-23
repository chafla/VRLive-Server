use std::collections::HashMap;
use std::net::SocketAddrV4;
use std::sync::{Arc, RwLock};

use async_trait::async_trait;
use bytes::Bytes;
use log::{debug, error, warn};
use rosc::{encoder, OscPacket};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc::{Receiver, Sender};

use protocol::{OSCDecodable, OSCEncodable, UserData};
use protocol::backing_track::BackingTrackData;
use protocol::osc_messages_in::ClientMessage;
use protocol::osc_messages_out::ServerMessage;
use protocol::vrl_packet::VRLOSCPacket;

use crate::{AudioPacket, VRTPPacket};

pub mod audience;
pub mod performer;
// mod peer_connection;
mod streaming;


/// Client-specific channel data.
/// Stored out in a separate struct for organization, especially since this data will be common to all client types.
pub struct ClientChannelData {
    // channels that are somewhat managed by the server

    /// Get events from the server's main event thread, to send out to a user.
    /// This is an option as it must be taken, otherwise it will tie up the whole data structure.
    pub server_events_out: Option<Receiver<ServerMessage>>,
    /// Get backing track data to send out to our connected server.
    pub backing_track_in: Option<Receiver<BackingTrackData>>,
    /// Pass events from our client to the main server
    pub client_events_in: Sender<ClientMessage>,
    /// Get mocap events to transmit out to our clients
    pub audience_mocap_out: Option<Receiver<OscPacket>>,
    /// Get our client event socket from the server
    pub client_event_socket_chan: Option<Receiver<TcpStream>>,
    pub backing_track_socket_chan: Option<Receiver<TcpStream>>,
    pub server_event_socket_chan: Option<Receiver<TcpStream>>,
    /// Data inbound from the synchronizer
    pub from_sync_out_chan: Option<Receiver<Bytes>>,

    // channels dependent on the synchronizer, thus may exist depending on the type of client that we are

    // Performer clients manage their own synchronizer threads.
    // This means that they may exist, or may not if we're an audience member.
    pub synchronizer_osc_in: Option<Receiver<VRLOSCPacket>>,
    pub synchronizer_audio_in: Option<Receiver<AudioPacket>>,
    /// Sending data from the synchronizer to the server's output thread.
    pub synchronizer_vrtp_out: Option<Sender<VRTPPacket>>,
}

// impl ClientChannelData {
//     pub fn new(server_events_out: Receiver<ServerMessage>,
//                client_events_in: Sender<ClientMessage>, backing_track_in: Receiver<BackingTrackData>,
//                event_socket_chan: Receiver<TcpStream>, backing_track_socket_chan: Receiver<TcpStream>,
//         server_event_socket_chan: Receiver<TcpStream>, mocap_chan: Receiver<OscPacket>
//     ) -> Self {
//         Self {
//             server_events_out: Some(server_events_out),
//             client_events_in: client_events_in,
//             backing_track_in: Some(backing_track_in),
//             client_event_socket_chan: Some(event_socket_chan),
//             backing_track_socket_chan: Some(backing_track_socket_chan),
//             server_event_socket_chan: Some(server_event_socket_chan),
//             synchronizer_osc_in: None,
//             synchronizer_audio_in: None,
//             synchronizer_vrtp_out: None,
//             audience_mocap_out: Some(mocap_chan)
//         }
//     }
// }

/// Remote ports available on the client.
#[derive(Clone, Debug)]
pub struct ClientPorts {
    /// Port that server events will be sent to
    server_event_port: u16,
    /// Port that new backing tracks will be sent to
    backing_track_port: u16,
    /// Port that audience mocap will be sent to
    audience_mocap_port: u16,
    /// Port that will be receiving VRTP packets
    vrtp_port: u16,
    /// Any supplemental ports that the client should be listening on.
    extra_ports: Arc<RwLock<HashMap<String, u16>>>
}

impl ClientPorts {

    pub fn new(server_event_port: u16, backing_track_port: u16, vrtp_port: u16, audience_mocap_port: u16, extra_ports: Option<HashMap<String, u16>>) -> Self {

        Self {
            server_event_port,
            backing_track_port,
            vrtp_port,
            audience_mocap_port,
            extra_ports: Arc::new(RwLock::new(extra_ports.unwrap_or(HashMap::new())))
        }
    }
}



#[async_trait]
/// Trait defining the necessary behavior for a client of our server.
pub trait VRLClient {

    fn ports(&self) -> &ClientPorts;

    fn channels(&self) -> &ClientChannelData;

    fn channels_mut(&mut self) -> &mut ClientChannelData;

    fn user_data(&self) -> &UserData;

    /// Start all of the main channel tasks.
    async fn start_main_channels(&mut self) {
        // server events
        {
            let server_event_sender = self.channels_mut().server_events_out.take().unwrap();

            let server_sock_chan = self.channels_mut().server_event_socket_chan.take().unwrap();
            let server_port = self.ports().server_event_port;
            let user_data = self.user_data().clone();

            tokio::spawn(async move {
                Self::server_event_sender(server_sock_chan, server_event_sender).await
            });
        }

        {
            let client_event_chan = self.channels().client_events_in.clone();
            let client_sock_chan = self.channels_mut().client_event_socket_chan.take().unwrap();
            tokio::spawn(async move {
                Self::client_event_listener("Client event", client_event_chan, client_sock_chan).await
            });

            let backing_track_chan = self.channels_mut().backing_track_in.take().unwrap();
            let backing_track_sock = self.channels_mut().backing_track_socket_chan.take().unwrap();
            tokio::spawn(async move {
                Self::backing_track_sender(backing_track_sock, backing_track_chan).await
            });
        }
    }

    /// Thread handling output for any server events.
    async fn server_event_sender(mut event_sock: Receiver<TcpStream>, mut sender_in: Receiver<ServerMessage>) {
        'outer: loop {
            let client_stream = event_sock.recv().await;
            if client_stream.is_none() {
                debug!("Server event listener shutting down.");
                break;
            } else {
                debug!("Server event listener is up and ready")
            }
            let mut client_stream = client_stream.unwrap();

            debug!("Server event sender has gotten a client stream!");

            loop {
                let res = sender_in.recv().await;

                // our server has been killed!
                if let None = res {
                    // debug!("Server event listener for {0} has been destroyed.", &user_data.fancy_title);
                    break 'outer;
                }

                let msg = res.unwrap().encode();
                let msg = OscPacket::Message(msg);

                let packet = encoder::encode(&msg);

                if let Err(e) = &packet {
                    warn!("Failed to encode osc message {0:?}: {e}", &msg);
                    continue;
                }

                let packet = packet.unwrap();

                let sent = client_stream.write(&packet).await;

                match sent {
                    Ok(_) => {
                        debug!("Sent a packet {0:?}", &packet);
                    }
                    Err(e) => warn!("Failed to send packet: {e}")
                }
            }
        }

        debug!("Shutting down server event sender");

    }

    /// Thread handling input for any client events.
    /// This will become the new "main" thread for the server keeping it alive.
    async fn client_event_listener(stream_title: &'static str, client_events_out: Sender<ClientMessage>, mut client_socket: Receiver<TcpStream>) {

        // loop: if we lose connection, we can just have the client give us a new handle.
        // unless that one's dead too.
        loop {
            let client_stream = client_socket.recv().await;
            if client_stream.is_none() {
                debug!("{stream_title} listener shutting down.");
                break;
            }
            else {
                debug!("{stream_title} listener is up and ready")
            }
            let mut client_stream = client_stream.unwrap();

            debug!("{stream_title} track handler has received a stream!");

            let mut client_event_buf: [u8; 1024];

            'connection: loop {
                client_event_buf = [0; 1024];
                let recv = client_stream.read(&mut client_event_buf).await;
                let incoming_bytes = match recv {
                    Err(e) => {
                        warn!("{stream_title} stream failed on read ({e}). May be closed?");
                        break 'connection;
                    },
                    Ok(b) => {
                        if b == 0 {
                            warn!("{stream_title} stream seems to be giving EOF, terminating.");
                            break 'connection;
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

            debug!("we're dead!");
            // select! {
            //
            // }
        }
    }

    async fn combined_performer_data_out(&self, mut stream_channel: Receiver<Bytes>, client_socket: SocketAddrV4) {
        'outer: loop {
            // we really don't care what things look like on the receiving end
            // all that we care about is blasting a ton of UDP packets at them as quickly as possible

        }
    }

    /// Thread responsible for updating the backing track as needed.
    async fn backing_track_sender(mut sock_channel: Receiver<TcpStream>, mut backing_track_stream: Receiver<BackingTrackData>) {
        'outer: loop {
            let client_stream = sock_channel.recv().await;
            if client_stream.is_none() {
                debug!("Backing track sender shutting down.");
                break;
            }
            else {
                debug!("Backing track sender is up and ready")
            }
            let mut client_stream = client_stream.unwrap();

            debug!("Backing track sender has received a stream!");

            loop {
                // block and wait
                let incoming_backing_track_msg = match backing_track_stream.recv().await {
                    Some(msg) => msg,
                    None => {
                        warn!("Backing track stream source is dead, the other side must have been dropped!");
                        break 'outer
                    }
                };


                debug!("Writing backing track out to client");
                match client_stream.write(incoming_backing_track_msg.get_data()).await {
                    Err(e) => {
                        error!("Failed to write backing track out to a client: {e}");
                        client_stream.shutdown().await.unwrap();
                        continue 'outer;
                    }
                    Ok(b) => {
                        debug!("Wrote {b} bytes worth of backing track out to client!")
                    }

                }

            }
        }

        debug!("Backing track task shutting down.")
    }
}
