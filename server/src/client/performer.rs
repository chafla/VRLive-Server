use std::sync::Arc;

use rosc::OscBundle;
use tokio::net::TcpStream;
use tokio::sync::mpsc::Receiver;
use tokio::sync::mpsc::Sender;
use webrtc::api::media_engine::MIME_TYPE_OPUS;
use webrtc::rtp_transceiver::rtp_codec::RTCRtpCodecCapability;
use webrtc::track::track_local::track_local_static_rtp::TrackLocalStaticRTP;
use webrtc::track::track_local::TrackLocal;

use protocol::UserData;

use crate::{AudioPacket, VRTPPacket};
use crate::client::{ClientChannelData, ClientPorts, VRLClient};
use crate::client::streaming::braindead_simple_rtp::{RTPSenderOut, SynchronizerData};
use crate::client::streaming::peer_connection::{register_performer_mocap_data_channel, WebRTPConnection};
use crate::client::synchronizer::Synchronizer;

const CLOCK_RATE: f32 = 48000.0;

pub struct Performer
{
    user_data: UserData,
    base_channels: ClientChannelData,
    ports: ClientPorts,
    streaming_connection: Option<Arc<WebRTPConnection>>,
    rtp_stream: Option<Arc<RTPSenderOut<SynchronizerData>>>,
    // webRTC stuff
    signaling_channel: TcpStream,

    // internal channels
    /// Channels necessary for the synchronizer.
    /// These should both be consumed at once.
    sync_channels: Option<(Receiver<OscBundle>, Receiver<AudioPacket>)>,

    // synchronizer: Synchronizer
}

const PERFORMER_OFFER: &'static str = "hi1";

impl Performer {

    pub fn get_title(&self) -> &str {
        &self.user_data.fancy_title
    }

    // pub async fn new_rtp(
    //     user_data: UserData, base_channels: ClientChannelData, ports: ClientPorts, signaling_channel: TcpStream
    // )

    pub async fn new_rtp(
        user_data: UserData, mut base_channels: ClientChannelData, ports: ClientPorts, signaling_channel: TcpStream,
        audio_to_sync: Receiver<AudioPacket>, mocap_to_sync: Receiver<OscBundle>
    ) -> Self {


        let mut synchronizer = Synchronizer::new(&base_channels.synchronizer_vrtp_out.take().unwrap());
        // let sync_out = base_channels.synchronizer_vrtp_out.take().unwrap();
        tokio::spawn(async move {
            synchronizer.intake(mocap_to_sync, audio_to_sync, CLOCK_RATE).await;
        });

        Self {
            user_data,
            base_channels,
            ports,
            signaling_channel,
            sync_channels: None,
            streaming_connection: None,
            rtp_stream: None,
            // synchronizer: Synchronizer::new()
        }
    }
    pub async fn new_rtc(
        user_data: UserData, base_channels: ClientChannelData, ports: ClientPorts, signaling_channel: TcpStream
    ) -> Self {

        let (osc_tx, osc_rx) = tokio::sync::mpsc::channel(2048);
        let (audio_tx, audio_rx) = tokio::sync::mpsc::channel(2048);

        let incoming = Self::create_incoming_connection(
            &user_data.fancy_title,
            osc_tx,
            audio_tx,
        ).await.unwrap();

        // let outgoing = Self::create_outgoing_connection(
        //     &user_data.fancy_title,
        //     base_channels.synchronizer_vrtp_out.take().unwrap(),
        //
        // ).await.unwrap();

        Self {
            user_data,
            base_channels,
            ports,
            signaling_channel,
            sync_channels: Some((osc_rx, audio_rx)),
            streaming_connection: Some(Arc::new(incoming)),
            rtp_stream: None,
            // synchronizer: Synchronizer::new()
        }
    }

    /// Start the webRTC connection
    async fn establish_webrtc_connection(&self) {

    }


    pub fn get_audio_track(identifier: &str) -> TrackLocalStaticRTP {
        TrackLocalStaticRTP::new(
            RTCRtpCodecCapability {
                mime_type: MIME_TYPE_OPUS.to_owned(),
                ..Default::default()
            },
            "performer_audio".to_owned(),
            identifier.to_owned()
        )
    }

    /// Create and configure the tracks that we are going to be listening on.
    /// These are two separate steps:
    /// First, create tracks with known IDs and register those tracks by adding them to the peer connection.
    /// Then, in the callback for tracks, we'll want to respond to the tracks that we intend on using accordingly.

    async fn setup_incoming_tracks(&self, webrtc_connection: Arc<WebRTPConnection>) {
        // What we could do is keep track of all the tracks that we've created, and store an associated callback fn alongside them.
        // We could then add a general conn.on_track handler that matches if the details of the new track are the same as one that we're already aware of.
        webrtc_connection.peer_connection_ref().on_track(Box::new(move |track, rx, tx| {
            // handle incoming tracks
            // see docs for what these terms mean
            // ID is more of the general "kind" of stream (not unique, like 'audio' or 'video')
            // stream_id needs to be unique and describe the stream specifically
            match (track.id().as_str(), track.stream_id().as_str()) {
                // TODO pull these out into consts somewhere
                ("audio", "performer") => todo!(),
                (_, _) => todo!()
            }

            Box::pin(async move {})

        }));
    }

    async fn synchronize(&self) {

    }

    /// Set up the tracks which the performer will be dispatching
    // async fn setup_outgoing_tracks(&self) {
    //     self.
    // }

    async fn create_incoming_connection(title: &str, osc_to_sync: Sender<OscBundle>, audio_to_sync: Sender<AudioPacket>) -> anyhow::Result<WebRTPConnection> {
        let mut conn = WebRTPConnection::new("Performer incoming").await;
        register_performer_mocap_data_channel(&mut conn, osc_to_sync).await?;

        let audio_track = Self::get_audio_track(title);
        let chans: Vec<Arc<dyn TrackLocal + Send + Sync>> = vec![Arc::new(audio_track)];
        conn.register_tracks(&chans[..]).await?;

        // todo figure out how to register the audio events
        // fundamentally, that's going to be a callback on on_track, which occurs in response to a new track spawning.
        // we need to work out, based on the track's metadata, what we're going to be doing with it.
        Ok(conn)
    }

    async fn create_outgoing_connection(title: &str, sync_to_out: Sender<VRTPPacket>) -> anyhow::Result<WebRTPConnection> {
        let mut conn = WebRTPConnection::new("Performer out").await;
        todo!();
        Ok(conn)

    }
}

impl VRLClient for Performer {
    fn ports(&self) -> &ClientPorts {
        &self.ports
    }

    fn channels(&self) -> &ClientChannelData {
        &self.base_channels
    }

    fn channels_mut(&mut self) -> &mut ClientChannelData {
        &mut self.base_channels
    }

    fn user_data(&self) -> &UserData {
        &self.user_data
    }
}
