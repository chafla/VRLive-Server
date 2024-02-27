use bytes::{Bytes, BytesMut};
use rosc::encoder::encode;
use rosc::{OscBundle, OscPacket};
use webrtc::rtp::packet::Packet;
use crate::client::synchronizer::{OscData, RTPPacket};
// use crate::client::

pub mod server;
pub mod client;


/// Maximum buffer size for the channel before we start to block
const MAX_CHAN_SIZE: usize = 2 << 12;

type AudioPacket = Packet;  // TODO

#[derive(Debug, Clone)]
pub enum VRTPPacket {
    Encoded(Bytes),
    Raw(Vec<OscData>, RTPPacket)
}

impl Into<Bytes> for VRTPPacket {
    fn into(self) -> Bytes {
        match self {
            VRTPPacket::Encoded(b) => b,
            VRTPPacket::Raw(osc, rtp) => {
                let mut bytes_out = BytesMut::with_capacity(1400 + 16);
                // let osc_bytes = encode(&osc).unwrap();
                // let packets = vec![];

                // TODO WORK OUT PROTOCOL FOR THIS

                // let min_data_size = osc_bytes.len() + audio.len() + timestamp.to_ne_bytes().len();
                //
                // // all the space we need + a bit of a buffer
                //
                // assert!(bytes_out.len() < 1400);  // it needs to be less than the standard mtu or we're in trouble
                //
                // // if it's larger we definitely have problems we need to clear up
                // // anyway, preface with a u16 denoting the number of bytes the OSC message will take up
                // bytes_out.put_u16(osc_bytes.len() as u16);
                // // followed by the number of bytes the data itself will take
                // bytes_out.put_u16(audio.len() as u16);
                // // and lastly the timestamp in f32 form
                // bytes_out.put_f32(timestamp);
                // // now the data
                // // osc first
                // bytes_out.put(osc_bytes.as_slice());
                // // followed soon after by the audio
                // bytes_out.put(audio);

                // it's set, freeze it and punt it
                bytes_out.freeze()
            }
        }
    }
}
// type VRTPPacket = (Vec<OscData>, RTPPacket);
