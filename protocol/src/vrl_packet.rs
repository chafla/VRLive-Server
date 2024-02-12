use rosc::OscBundle;

use crate::VRLUser;

pub struct RawVRLOSCPacket {
    source_ip: u32,
    source_port: u16,
    payload: Vec<u32>,
}


/// Basic representation for a bundle we'll be handling.
pub struct VRLOSCPacket {
    source: VRLUser,
    destination: VRLUser,
    payload: OscBundle,
}

impl From<RawVRLOSCPacket> for VRLOSCPacket {
    fn from(value: RawVRLOSCPacket) -> Self {
        todo!()
    }
}

