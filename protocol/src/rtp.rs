use std::sync::Arc;
use std::time::Duration;
use log::{debug, error, info, warn};
/**
* Mechanism for handling incoming RTP traffic.
*/

use webrtc::rtp;
use anyhow::Result;
use webrtc::rtp::packet::Packet;
use webrtc::rtp::codecs::opus;
use webrtc::rtp::packetizer::Depacketizer;
use webrtc::util::Unmarshal;
use bytes;
use bytes::{Bytes, BytesMut};
use rosc::{OscPacket, decoder};
use rosc::decoder::decode_udp;
use tokio::sync::mpsc::Sender;
use webrtc::api::APIBuilder;
use webrtc::api::interceptor_registry::register_default_interceptors;
use webrtc::api::media_engine::{MediaEngine, MIME_TYPE_OPUS};
use webrtc::data_channel::data_channel_init::RTCDataChannelInit;
use webrtc::data_channel::data_channel_message::DataChannelMessage;
use webrtc::data_channel::RTCDataChannel;
use webrtc::ice_transport::ice_server::RTCIceServer;
use webrtc::interceptor::registry::Registry;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::{math_rand_alpha, RTCPeerConnection};
use webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState;
use webrtc::rtp_transceiver::rtp_codec::RTCRtpCodecCapability;
use webrtc::stats::RTCStatsType::DataChannel;
use webrtc::track::track_local::track_local_static_rtp::TrackLocalStaticRTP;
use webrtc::track::track_local::TrackLocal;

pub fn performer_audio_track(identifier: &str) -> TrackLocalStaticRTP {
    TrackLocalStaticRTP::new(
        RTCRtpCodecCapability {
            mime_type: MIME_TYPE_OPUS.to_owned(),
            ..Default::default()
        },
        "performer_audio".to_owned(),
        identifier.to_owned()
    )
}

// https://github.com/webrtc-rs/webrtc/blob/master/examples
pub async fn register_performer_mocap_data_channel(conn: &mut WebRTPConnection, mocap_data_out: Sender<OscPacket>) -> Result<Arc<RTCDataChannel>> {
    let mut options = RTCDataChannelInit::default();
    options.max_retransmits = Some(0);  // if it's lost it's lost
    let data_channel = conn.peer_connection.create_data_channel("performer mocap", Some(options)).await?;
    data_channel.on_message(Box::new(move |message| {
        debug!("Incoming mocap data channel open.");
        let data = message.data;
        let data_out = mocap_data_out.clone();

        Box::pin(async move {
            match decode_udp(data.as_ref()) {
                Err(e) => warn!("Failed to decode incoming mocap data: {e}"),
                Ok((_, pkt)) => match data_out.send(pkt).await {
                    Ok(_) => (),
                    Err(e) => warn!("Got err {e} when sending mocap")
                }
            }
        })

    }));

    let id = conn.identity.clone();

    data_channel.on_buffered_amount_low(Box::new(move || {
        warn!("Buffer is low in {id}!");
        Box::pin(async {})
    })).await;

    Ok(data_channel)
}

pub struct WebRTPConnection {
    identity: String,
    peer_connection: Arc<RTCPeerConnection>,
}

impl WebRTPConnection {
    pub async fn register_tracks(&mut self, tracks: &[Arc<dyn TrackLocal + Send + Sync>]) -> Result<()> {
        for track in tracks.iter() {
            self.peer_connection.add_track(Arc::clone(track)).await?;
        }

        Ok(())
    }

    pub async fn new(identity: &str) -> Self {
        Self {
            identity: identity.to_owned(),
            peer_connection: Self::create_peer_connection().await.unwrap()
        }
    }

    async fn create_peer_connection() -> Result<Arc<RTCPeerConnection>> {
        let mut media_engine = MediaEngine::default();

        media_engine.register_default_codecs()?;

        let mut registry = Registry::new();
        registry = register_default_interceptors(registry, &mut media_engine)?;

        let api = APIBuilder::new()
            .with_media_engine(media_engine)
            .with_interceptor_registry(registry)
            .build();

        // I'm sus on this. I don't like that it has to hit Google to function properly.

        // A STUN server is used to get an external network address.
        // TURN servers are used to relay traffic if direct (peer to peer) connection fails.
        let config = RTCConfiguration {
            ice_servers: vec![RTCIceServer {
                urls: vec!["stun:stun.l.google.com:19302".to_owned()],
                ..Default::default()
            }],
            ..Default::default()
        };

        Ok(Arc::new(api.new_peer_connection(config).await?))
    }

    pub async fn configure_peer_connection(&mut self) -> Result<()> {
        // https://github.com/webrtc-rs/webrtc/blob/master/examples/examples/data-channels/data-channels.rs
        let mut peer_connection = &mut self.peer_connection;

        let (done_tx, mut done_rx) = tokio::sync::mpsc::channel::<()>(1);

        // Set the handler for Peer connection state
        // This will notify you when the peer has connected/disconnected
        peer_connection.on_peer_connection_state_change(Box::new(move |s: RTCPeerConnectionState| {
            debug!("Peer Connection State has changed: {s}");

            match s {
                RTCPeerConnectionState::Connected => {
                    debug!("Peer connection has been established!");
                },
                RTCPeerConnectionState::Failed => {
                    // Wait until PeerConnection has had no network activity for 30 seconds or another failure. It may be reconnected using an ICE Restart.
                    // Use webrtc.PeerConnectionStateDisconnected if you are interested in detecting faster timeout.
                    // Note that the PeerConnection may come back from PeerConnectionStateDisconnected.
                    debug!("Peer Connection has gone to failed exiting");
                    let _ = done_tx.try_send(());
                },
                _ => ()
            }

            Box::pin(async {})
        }));

        // peer_connection.on_track()

        // Register data channel creation handling
        peer_connection
            .on_data_channel(Box::new(move |d: Arc<RTCDataChannel>| {
                let d_label = d.label().to_owned();
                let d_id = d.id();
                debug!("New DataChannel {d_label} {d_id}");

                // Register channel opening handling
                Box::pin(async move {
                    // move data into the box
                    let d2 = Arc::clone(&d);
                    let d_label2 = d_label.clone();
                    let d_id2 = d_id;

                    d.on_close(Box::new(move || {
                        println!("Data channel closed");
                        Box::pin(async {})
                    }));

                    d.on_open(Box::new(move || {
                        println!("Data channel '{d_label2}'-'{d_id2}' open. Random messages will now be sent to any connected DataChannels every 5 seconds");

                        Box::pin(async move {
                            // let mut result = Result::<usize>::Ok(0);
                            // while result.is_ok() {
                            //     let timeout = tokio::time::sleep(Duration::from_secs(5));
                            //     tokio::pin!(timeout);
                            //
                            //     tokio::select! {
                            //     _ = timeout.as_mut() =>{
                            //         let message = math_rand_alpha(15);
                            //         println!("Sending '{message}'");
                            //         result = d2.send_text(message).await.map_err(Into::into);
                            //     }
                            // };
                            // }
                        })
                    }));

                    // Register text message handling
                    d.on_message(Box::new(move |msg: DataChannelMessage| {
                        let msg_str = String::from_utf8(msg.data.to_vec()).unwrap();
                        println!("Message from DataChannel '{d_label}': '{msg_str}'");
                        Box::pin(async {})
                    }));
                })
            }));

        Ok(())

        // at this point the channel has been configured, but has not been started.
        // Ok(())


    }

    pub async fn listen(&mut self) {

    }

}

// pub fn handle_incoming_rtp() {
//     decode()
// }

/// Decode an incoming opus packet from RTP
fn decode(packet_data: &Bytes) -> Result<Bytes, ()> {
    // let mut pkt = match Packet::unmarshal(&mut packet_data) {
    //     Ok(raw) => raw,
    //     Err(e) => {
    //         error!("Failed to decode incoming rtp packet: {e}");
    //         return Err(format!("Failed to decode incoming rtp packet: {e}"));
    //     }
    // };



    // let data_bytes = bytes::Bytes::from(packet_data);





    let mut opus_packet = opus::OpusPacket::default();

    let packet = opus_packet.depacketize(packet_data);

    if let Ok(pkt) = packet {
        Ok(pkt)
    }
    else {
        Err(())
    }

}