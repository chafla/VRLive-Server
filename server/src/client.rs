use std::net::{SocketAddr, SocketAddrV4};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use async_trait::async_trait;
use bytes::{Bytes};
use log::{debug, error, info, trace, warn};
use rosc::{encoder, OscPacket};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpStream, UdpSocket};
use tokio::sync::mpsc::{Receiver, Sender};

use protocol::{OSCDecodable, OSCEncodable, UserData, VRLTCPPacket};
use protocol::backing_track::BackingTrackData;
use protocol::handshake::ClientPortMap;
use protocol::heartbeat::HeartbeatStatus;
use protocol::heartbeat::HeartbeatStatus::Hangup;
use protocol::osc_messages_in::ClientMessage;
use protocol::osc_messages_out::ServerMessage;
use protocol::vrl_packet::{OscData, VRTPPacket};

// use crate::client::synchronizer::OscData;
// use protocol::syn

pub mod audience;
pub mod performer;
mod streaming;
// pub mod synchronizer;


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
    pub audience_mocap_out: Option<Receiver<OscData>>,

    // These channels are the receiving end of TCP sockets,
    // populated on new connections
    /// Get our client event socket from the server
    pub client_event_socket_chan: Option<Receiver<TcpStream>>,
    pub backing_track_socket_chan: Option<Receiver<TcpStream>>,
    pub server_event_socket_chan: Option<Receiver<TcpStream>>,
    pub heartbeat_socket_chan: Option<Receiver<TcpStream>>,

    /// This channel is for receiving data from the server's dispatch thread to send out.
    pub from_sync_out_chan: Option<Receiver<VRTPPacket>>,

    // channels dependent on the synchronizer, thus may exist depending on the type of client that we are

    // Performer clients manage their own synchronizer threads.
    // This means that they may exist, or may not if we're an audience member.
    // pub synchronizer_osc_in: Option<Receiver<VRLOSCPacket>>,
    // pub synchronizer_audio_in: Option<Receiver<AudioPacket>>,
    // This channel is for performers to pass their own VRTP data onto the server dispatch thread.
    pub synchronizer_vrtp_out: Option<Sender<VRTPPacket>>,
}

#[async_trait]
/// Trait defining the necessary behavior for a client of our server.
pub trait VRLClient {

    fn ports(&self) -> &ClientPortMap;

    fn channels(&self) -> &ClientChannelData;

    fn channels_mut(&mut self) -> &mut ClientChannelData;

    fn user_data(&self) -> &UserData;

    fn active(&self) -> &Arc<AtomicBool>;

    // async fn start_custom_channels(&mut self);

    /// Start all of the main channel tasks.
    async fn start_main_channels(&mut self) {
        // server events
        {
            let server_event_sender = self.channels_mut().server_events_out.take().unwrap();

            let server_sock_chan = self.channels_mut().server_event_socket_chan.take().unwrap();
            let user_data = self.user_data().clone();

            tokio::spawn(async move {
                let handler = move |msg: ServerMessage, label: String| {
                    dbg!(&msg);
                    let msg = msg.encode();
                    let msg = OscPacket::Message(msg);
                    dbg!(&msg);

                    let packet = encoder::encode(&msg);

                    if let Err(e) = &packet {
                        warn!("{label} Failed to encode osc message {0:?}: {e}", &msg);
                        return Err("ah".into());
                    }

                    let message_out = VRLTCPPacket::new(
                        "BUNDLE",
                        Bytes::new(),  // empty,
                        Bytes::from(packet.unwrap())
                    );

                    Ok(message_out.into())
                };
                Self::tcp_stream_event_sender(server_sock_chan, server_event_sender, handler, &user_data.fancy_title).await
            });
        }

        // create the UDP senders
        {
            let synchronizer_recv = self.channels_mut().from_sync_out_chan.take().unwrap();
            let mocap_recv = self.channels_mut().audience_mocap_out.take().unwrap();
            // Send from whatever port, we'll be blasting directly to the client anyway
            let sync_addr = SocketAddr::new(self.user_data().remote_ip_addr, self.ports().vrtp_data);
            let mocap_addr = SocketAddr::new(self.user_data().remote_ip_addr, self.ports().audience_motion_capture);

            tokio::spawn(async move {
                Self::client_transmitter(synchronizer_recv, sync_addr, "Synchronizer to client").await;
            });

            tokio::spawn(async move {
                Self::client_transmitter(mocap_recv, mocap_addr, "Audience mocap to client").await;
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
                Self::backing_track_sender(backing_track_sock, backing_track_chan, "backing track sender").await
            });
        }
    }

    /// Utility transmitter that consumes data from a channel and transmits it out over a UDP socket.
    async fn client_transmitter<T>(mut incoming_data_chan: Receiver<T>, target_addr: SocketAddr, label: &'static str)
        where
            T: Send + Sync + Into<Bytes> + std::fmt::Debug
    {
        let sock = UdpSocket::bind(SocketAddrV4::new("0.0.0.0".parse().unwrap(), 0)).await.unwrap();
        // sock.connect(&target_addr).await.unwrap();
        info!("Client transmitter for {label} is now active.");
        let mut sent_once = false;
        loop {
            let msg = match incoming_data_chan.recv().await {
                None => {
                    error!("Client transmitter for {label}'s send channel appears closed, killing.");
                    break;
                },
                Some(m) => m
            };

            if !sent_once {
                sent_once = true;
                info!("Sent a {label} packet to {}", &target_addr)
            }

            trace!("{label} client transmitter sending data out to {}!", &target_addr);

            // dbg!(&msg);
            let msg_bytes: Bytes = msg.into();
            
            if (msg_bytes.len() == 0) {
                
            }

            if let Err(e) = sock.send_to(msg_bytes.as_ref(), target_addr).await {
                error!("{label} transmitter failed to send: {e}");
            }
        }
    }



    async fn heartbeat_listener(mut stream_sock: Receiver<TcpStream>, heartbeat_status_out: Sender<HeartbeatStatus>, label: &str) {
        let client_stream = stream_sock.recv().await;
        if client_stream.is_none() {
            warn!("{label} Heartbeat listener was given a dead TCP stream!");
            return;
        } else {
            debug!("{label} Heartbeat listener is up and ready")
        }
        let mut client_stream = client_stream.unwrap();
        let mut sock_buf: [u8; 1024];

        loop {
            sock_buf = [0; 1024];
            tokio::select! {
                biased;
                new_sock = stream_sock.recv() => {
                    match new_sock {
                        None => {
                            warn!("{label} Client stream is closed, shutting down event sender");
                            break;
                        },
                        Some(s) => {
                            info!("Server event stream has been updated!");
                            client_stream = s;
                        }
                    }
                },
                incoming_bytes = client_stream.read(&mut sock_buf) => {
                    // the fact we got something is nice
                    if let Err(e) = incoming_bytes {
                        warn!("{label} heartbeat returned an error: {e}!");
                        heartbeat_status_out.send(Hangup(e.to_string())).await.unwrap();
                        break;
                    }
                    let incoming_msg = &sock_buf[0..incoming_bytes.unwrap()];
                    // todo process this to read disconnects


                }
            }
        }

        debug!("Shutting down server event sender");

    }

    // note that, yes, a lot of this code is duplicated, mostly because async closures are unstable

    /// Thread which receives events from a channel and forwards them onto a tcp stream
    async fn tcp_stream_event_sender<T, F>(mut stream_sock: Receiver<TcpStream>, mut sender_in: Receiver<T>, handler: F, label: &str)
        where
            T: Sync + Send,
            F: Fn(T, String) -> Result<Bytes, String> + Send + Sync
    {
        let client_stream = stream_sock.recv().await;
        if client_stream.is_none() {
            warn!("{label} Server event listener never received a stream before the channel closed.");
            return;
        } else {
            debug!("Server event listener is up and ready")
        }
        let mut client_stream = client_stream.unwrap();

        loop {
            tokio::select! {
                biased;
                new_sock = stream_sock.recv() => {
                    match new_sock {
                        None => {
                            warn!("{label} Client stream is closed, shutting down event sender");
                            break;
                        },
                        Some(s) => {
                            info!("Server event stream has been updated!");
                            client_stream = s;
                        }
                    }
                },
                server_msg = sender_in.recv() => {
                    // our server has been killed!
                    if let None = server_msg {
                        debug!("Server event listener for {0} has been destroyed.", &label);
                        break;
                    }

                    let processed_data = match handler(server_msg.unwrap(), label.to_string()) {
                        Err(e) => {
                            error!("{label} failed to unwrap client data: {e}");
                            continue;
                        }
                        Ok(d) => d
                    };

                    let sent = client_stream.write(&processed_data).await;

                     match sent {
                        Ok(b) => {
                            debug!("{label} Sent a packet of {b} bytes");
                        }
                        Err(e) => {
                            warn!("{label} Failed to send packet: {e}");
                            // if we fail sending, we'll just have to mark this as terminated.
                            // loop back around to try for another listener.
                            continue;
                        }
                    }



                    // let sent = client_stream.write(&packet).await;


                }
            }
        }

        debug!("Shutting down server event sender");

    }

    // async fn synchronizer_loop(mocap_in: Receiver<OscPacket>, audio_in: Receiver<Bytes>)

    /// Thread handling input for any client events.
    async fn client_event_listener(label: &'static str, client_events_out: Sender<ClientMessage>, mut stream_sock: Receiver<TcpStream>) {

        // loop: if we lose connection, we can just have the client give us a new handle.
        // unless that one's dead too.
        let mut client_stream = match stream_sock.recv().await {
            None => {
                warn!("{label} Server event listener never received a stream before the channel closed.");
                return;
            },
            Some(s) => s
        };



        let mut client_event_buf: [u8; 1024];
        loop {
            client_event_buf = [0; 1024];
            tokio::select! {
                new_sock = stream_sock.recv() => {
                    match new_sock {
                        None => {
                            warn!("{label} Client stream is closed, shutting down event sender");
                            break;
                        },
                        Some(s) => {
                            info!("{label} stream has been updated!");
                            client_stream = s;
                        }
                    }
                },
                incoming_bytes = client_stream.read(&mut client_event_buf) => {
                    let incoming_bytes = match incoming_bytes {
                        Err(e) => {
                            warn!("{label} stream failed on read ({e}). May be closed?");
                            break;
                        },
                        Ok(b) => b
                    };
                    if incoming_bytes == 0 {
                        // we probably lost connection, let's wait for it to clear
                        tokio::time::sleep(Duration::from_millis(50)).await;
                    }
                    let read_bytes = &client_event_buf[0..incoming_bytes];
                    trace!("{label} got {incoming_bytes} bytes");

                    let res = rosc::decoder::decode_tcp_vec(read_bytes);
                    if res.is_err() {
                        warn!("{label} channel got an invalid data stream of {incoming_bytes}!");
                        continue;
                    }
                    let (rest, packets) = res.unwrap();
                    if !rest.is_empty() {
                        debug!("{label} message had trailing data: {0:?}", rest)
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
            }

        }

        info!("{label} shutting down");

    }

    // async fn combined_performer_data_out(&self, mut stream_channel: Receiver<Bytes>, client_socket: SocketAddrV4) {
    //     'outer: loop {
    //         // we really don't care what things look like on the receiving end
    //         // all that we care about is blasting a ton of UDP packets at them as quickly as possible
    //
    //     }
    // }
    //
    // async fn synchronized_data_sender(mut synchronized_data_out: Receiver<VRTPPacket>) {
    //     debug!("Synchronized data sender is up and ready!");
    //     loop {
    //         let incoming_message = match synchronized_data_out.recv().await {
    //             None => {
    //                 warn!("Synchronized data sender is shutting down!");
    //                 break;
    //             },
    //             Some(VRTPPacket::Encoded(b)) => b,
    //             Some(raw@VRTPPacket::Raw(_, _)) => raw.into()
    //         };
    //         // todo stuff
    //     }
    //     // TODO process this data before it's sent
    // }

    // async fn

    /// Thread responsible for updating the backing track as needed.
    async fn backing_track_sender(mut stream_sock: Receiver<TcpStream>, mut backing_track_stream: Receiver<BackingTrackData>, label: &str) {
        let mut client_stream = match stream_sock.recv().await {
            None => {
                warn!("{label} Server event listener never received a stream before the channel closed.");
                return;
            },
            Some(s) => s
        };

        loop {
            tokio::select! {
                new_sock = stream_sock.recv() => {
                    match new_sock {
                        None => {
                            warn!("{label} Client stream is closed, shutting down event sender");
                            break;
                        },
                        Some(s) => {
                            info!("{label} stream has been updated!");
                            client_stream = s;
                        }
                    }
                },
                incoming_msg = backing_track_stream.recv() => {
                    let incoming_backing_track_msg = match incoming_msg {
                        Some(msg) => msg,
                        None => {
                            warn!("Backing track stream source is dead, the other side must have been dropped!");
                            break
                        }
                    };


                    debug!("Writing backing track out to client");
                    match client_stream.write(incoming_backing_track_msg.get_data()).await {
                        Err(e) => {
                            error!("Failed to write backing track out to a client: {e}");
                            client_stream.shutdown().await.unwrap();
                            continue;
                        }
                        Ok(b) => {
                            debug!("Wrote {b} bytes worth of backing track out to client!")
                        }

                    }
                }
            }
        }


        debug!("Backing track task shutting down.")
    }

    /// Heartbeat service.
    /// Used to keep tabs on whether the service is alive or if it needs to be disconnected.
    /// If this connection is lost. then we'll restart the whole thing
    async fn liveliness_monitor() {

    }
}
