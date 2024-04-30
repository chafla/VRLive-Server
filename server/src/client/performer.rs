use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use rosc::OscBundle;
use tokio::sync::mpsc::Receiver;
use tokio::sync::mpsc::Sender;

use protocol::handshake::ClientPortMap;
use protocol::synchronizer::Synchronizer;
use protocol::UserData;
use crate::analytics::AnalyticsData;

use crate::AudioPacket;
use crate::client::{ClientChannelData, VRLClient};

const CLOCK_RATE: f32 = 48000.0;

pub struct Performer
{
    user_data: UserData,
    base_channels: ClientChannelData,
    ports: ClientPortMap,
    // webRTC stuff
    // signaling_channel: TcpStream,
    active: Arc<AtomicBool>,

    // internal channels
    /// Channels necessary for the synchronizer.
    /// These should both be consumed at once.
    #[allow(unused)]
    sync_channels: Option<(Receiver<OscBundle>, Receiver<AudioPacket>)>,
    #[allow(unused)]
    server_analytics_channel: Sender<AnalyticsData>

    // synchronizer: Synchronizer
}

impl Performer {
    pub fn get_title(&self) -> &str {
        &self.user_data.fancy_title
    }


    pub async fn new_rtp(
        user_data: UserData, mut base_channels: ClientChannelData, ports: ClientPortMap,
        audio_to_sync: Receiver<AudioPacket>, mocap_to_sync: Receiver<OscBundle>, server_analytics_channel: Sender<AnalyticsData>
    ) -> Self {
        let mut synchronizer = Synchronizer::new(&base_channels.synchronizer_vrtp_out.take().unwrap(), user_data.participant_id);
        tokio::spawn(async move {
            synchronizer.intake(mocap_to_sync, audio_to_sync, CLOCK_RATE).await;
        });

        Self {
            user_data,
            base_channels,
            ports,
            sync_channels: None,
            active: Arc::new(AtomicBool::new(true)),
            server_analytics_channel
        }
    }
}
impl VRLClient for Performer {
    fn ports(&self) -> &ClientPortMap {
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

    fn active(&self) -> &Arc<AtomicBool> {
        &self.active
    }
}
