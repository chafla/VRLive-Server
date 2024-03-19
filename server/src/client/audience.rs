use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use tokio::net::TcpStream;
use protocol::handshake::ClientPortMap;

use protocol::UserData;

use crate::client::{ClientChannelData, VRLClient};

pub struct AudienceMember {
    user_data: UserData,
    base_channels: ClientChannelData,
    ports: ClientPortMap,
    signaling_channel: TcpStream,
    active: Arc<AtomicBool>,
}

impl AudienceMember {

    pub fn get_title(&self) -> &str {
        &self.user_data.fancy_title
    }
    pub fn new(user_data: UserData, base_channels: ClientChannelData, ports: ClientPortMap, signaling_channel: TcpStream) -> Self {
        Self {
            user_data,
            base_channels,
            ports,
            signaling_channel,
            active: Arc::new(AtomicBool::new(true)),
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
