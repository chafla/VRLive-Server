use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use bytes;
use bytes::Bytes;
use log::{debug, warn};
use rosc::decoder::decode_udp;
use rosc::OscPacket;
use tokio::sync::mpsc::Sender;
use tokio::sync::Mutex;
use webrtc::api::APIBuilder;
use webrtc::api::interceptor_registry::register_default_interceptors;
use webrtc::api::media_engine::{MediaEngine, MIME_TYPE_OPUS};
use webrtc::data_channel::data_channel_init::RTCDataChannelInit;
use webrtc::data_channel::data_channel_message::DataChannelMessage;
use webrtc::data_channel::RTCDataChannel;
use webrtc::ice_transport::ice_candidate::RTCIceCandidate;
use webrtc::ice_transport::ice_server::RTCIceServer;
use webrtc::interceptor::registry::Registry;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState;
use webrtc::peer_connection::RTCPeerConnection;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use webrtc::rtp_transceiver::rtp_codec::RTCRtpCodecCapability;
use webrtc::track::track_local::track_local_static_rtp::TrackLocalStaticRTP;
use webrtc::track::track_local::TrackLocal;


// pub async fn register_performer_audio_tracks(conn: &mut WebRTPConnection, title: &str) -> Result<()> {
//     let audio_track = performer_audio_track(title);
//
//
//     let chans: Vec<Arc<dyn TrackLocal + Send + Sync>> = vec![Arc::new(audio_track)];
//     conn.register_tracks(&chans[..]).await?;
//
//     Ok(())
//
// }

// pub fn performer_session_description() -> RTCSessionDescription {
//
// }

// https://github.com/webrtc-rs/webrtc/blob/master/examples
pub async fn register_performer_mocap_data_channel(conn: &mut WebRTPConnection, mocap_data_out: Sender<OscPacket>) -> Result<Arc<RTCDataChannel>> {
    let options = RTCDataChannelInit {
        max_retransmits: Some(0),
        ..Default::default()
    };
    // create a new data channel specifically designed to handle incoming motion capture data.
    // this can happen in isolation as it only impacts the results of this specific data channel,
    // without touching anything else.
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
    pending_candidates: Arc<Mutex<Vec<RTCIceCandidate>>>
}

impl WebRTPConnection {

    pub fn peer_connection_ref(&self) -> &Arc<RTCPeerConnection> {
        &self.peer_connection
    }

    pub fn peer_connection_as_arc(&self) -> Arc<RTCPeerConnection> {
        Arc::clone(&self.peer_connection)
    }
    pub async fn register_tracks(&mut self, tracks: &[Arc<dyn TrackLocal + Send + Sync>]) -> Result<()> {
        for track in tracks.iter() {
            self.peer_connection.add_track(Arc::clone(track)).await?;
        }

        Ok(())
    }

    pub async fn new(identity: &str) -> Self {
        Self {
            identity: identity.to_owned(),
            peer_connection: Self::create_peer_connection().await.unwrap(),
            pending_candidates: Arc::new(Mutex::new(vec![]))
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
        let peer_connection = &self.peer_connection;

        let (done_tx, _done_rx) = tokio::sync::mpsc::channel::<()>(1);

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
                    let _d2 = Arc::clone(&d);
                    let d_label2 = d_label.clone();
                    let d_id2 = d_id;

                    d.on_close(Box::new(move || {
                        debug!("Data channel closed");
                        Box::pin(async {})
                    }));

                    d.on_open(Box::new(move || {
                        debug!("Data channel '{d_label2}'-'{d_id2}' open. Random messages will now be sent to any connected DataChannels every 5 seconds");

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
                        debug!("Message from DataChannel '{d_label}': '{msg_str}'");
                        Box::pin(async {})
                    }));
                })
            }));

        // This is what gets called when someone first tries to connect with us:
        // they get marked as an ICE candidate
        let ice_pc = Arc::downgrade(&peer_connection);
        let outer_pending_candidates = Arc::clone(&self.pending_candidates);
        peer_connection.on_ice_candidate(Box::new(move |candidate| {
            let ice_inner_pc = ice_pc.clone();
            let inner_pending_candidates = Arc::clone(&outer_pending_candidates);
            Box::pin(async move {
                // we have someone new! Let's go!
                if let Some(c) = candidate {
                    // we also haven't dropped our list tracking peer candidates yet
                    if let Some(pc) = ice_inner_pc.upgrade() {
                        let desc = pc.remote_description().await;
                        if desc.is_none() {
                            let mut pending_candidates = inner_pending_candidates.lock().await;
                            pending_candidates.push(c);
                        }
                        // else if let Err(err) = unimplemented!() {
                        //     // this unimplemented function should be the mechanism that actually signals the clients
                        //     // meaning: this should probably be in the handshake to some degree.
                        //     // clients need to communicate
                        //     panic!("{}", err)
                        // }
                    }

                }
            })

        }));

        Ok(())

        // at this point the channel has been configured, but has not been started.
        // Ok(())


    }

    pub async fn create_offer(&mut self) -> Result<RTCSessionDescription> {
        let offer = self.peer_connection.create_offer(None).await.unwrap();
        self.peer_connection.set_local_description(offer).await?;
        // TODO we should find out how to get ice candidates here, as per 4) on https://developer.mozilla.org/en-US/docs/Web/API/WebRTC_API/Connectivity
        Ok(self.peer_connection.local_description().await.unwrap())

        // match offer {
        //     Ok(o) => Some(o),
        //     // TODO this probably shouldn't panic!
        //     Err(e) => panic!("{e}")
        // }
    }


    // pub async fn listen(&mut self) -> Result<()> {
    //     let offer = self.peer_connection.create_offer(None).await?;
    //     // Create channel that is blocked until ICE Gathering is complete
    //     // let mut gather_complete = self.peer_connection.gathering_complete_promise().await;
    // }
// Sets the LocalDescription, and starts our UDP listeners
//     peer_connection.set_local_description(offer).await?;
}

// pub fn handle_incoming_rtp() {
//     decode()
// }

