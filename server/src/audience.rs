use protocol::UserData;
use crate::client::{ClientChannelData, ClientPorts, VRLClient};

pub struct AudienceMember {
    user_data: UserData,
    base_channels: ClientChannelData,
    ports: ClientPorts,
}

impl AudienceMember {

    pub fn get_title(&self) -> &str {
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
