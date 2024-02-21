use std::sync::Arc;
use tokio::sync::mpsc::Receiver;
use bytes::Bytes;
use log::warn;
use rosc::OscPacket;
use tokio::sync::mpsc::Sender;
use webrtc::track::track_local::TrackLocal;
use protocol::rtp::{performer_audio_track, register_performer_mocap_data_channel, WebRTPConnection};
use protocol::UserData;
use crate::client::{ClientChannelData, ClientPorts, VRLClient};
use crate::VRTPPacket;

pub struct Performer {
    user_data: UserData,
    base_channels: ClientChannelData,
    ports: ClientPorts,
    incoming_connection: Arc<WebRTPConnection>,
    outgoing_connection: Arc<WebRTPConnection>,

    // internal channels
    sync_channels: Option<(Receiver<OscPacket>, Receiver<Bytes>)>,
}

const PERFORMER_OFFER: &'static str = "hi1";

impl Performer {

    pub fn get_title(&self) -> &str {
        &self.user_data.fancy_title
    }
    pub async fn new(
        user_data: UserData, mut base_channels: ClientChannelData, ports: ClientPorts
    ) -> Self {

        let (osc_tx, osc_rx) = tokio::sync::mpsc::channel(2048);
        let (audio_tx, audio_rx) = tokio::sync::mpsc::channel(2048);

        let incoming = Self::create_incoming_connection(
            &user_data.fancy_title,
            osc_tx,
            audio_tx,
        ).await.unwrap();

        let outgoing = Self::create_outgoing_connection(
            &user_data.fancy_title,
            base_channels.synchronizer_vrtp_out.take().unwrap(),

        ).await.unwrap();

        Self {
            user_data,
            base_channels,
            ports,
            sync_channels: Some((osc_rx, audio_rx)),
            incoming_connection: Arc::new(incoming),
            outgoing_connection: Arc::new(outgoing)
        }
    }

    async fn start_connections() {

    }

    async fn create_incoming_connection(title: &str, osc_to_sync: Sender<OscPacket>, audio_to_sync: Sender<Bytes>) -> anyhow::Result<WebRTPConnection> {
        let mut conn = WebRTPConnection::new("Performer incoming").await;
        register_performer_mocap_data_channel(&mut conn, osc_to_sync).await?;

        let audio_track = performer_audio_track(title);
        let chans: Vec<Arc<dyn TrackLocal + Send + Sync>> = vec![Arc::new(audio_track)];
        conn.register_tracks(&chans[..]).await?;
        // todo figure out how to register the audio events
        Ok(conn)
    }

    async fn create_outgoing_connection(title: &str, sync_to_out: Sender<VRTPPacket>) -> anyhow::Result<WebRTPConnection> {
        let mut conn = WebRTPConnection::new("Performer out").await;
        warn!("Outgoing connection unimplemented");
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
