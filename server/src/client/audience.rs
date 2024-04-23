use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use tokio::net::TcpStream;
use tokio::sync::mpsc::Sender;
use protocol::handshake::ClientPortMap;

use protocol::UserData;
use crate::analytics::{AnalyticsData, AnalyticsEvent};

use crate::client::{ClientChannelData, VRLClient};

pub struct AudienceMember {
    user_data: UserData,
    base_channels: ClientChannelData,
    ports: ClientPortMap,
    signaling_channel: TcpStream,
    active: Arc<AtomicBool>,
    server_analytics_channel: Sender<AnalyticsData>
}

impl AudienceMember {

    pub fn get_title(&self) -> &str {
        &self.user_data.fancy_title
    }
    pub fn new(user_data: UserData, base_channels: ClientChannelData, ports: ClientPortMap, signaling_channel: TcpStream, server_analytics_channel: Sender<AnalyticsData>) -> Self {
        Self {
            user_data,
            base_channels,
            ports,
            signaling_channel,
            active: Arc::new(AtomicBool::new(true)),
            server_analytics_channel
        }
    }
}

impl VRLClient for AudienceMember {
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
