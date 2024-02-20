use log::error;
/**
* Mechanism for handling incoming RTP traffic.
*/

use webrtc::rtp;
use webrtc::rtp::packet::Packet;
use webrtc::rtp::codecs::opus;
use webrtc::rtp::packetizer::Depacketizer;
use webrtc::util::Unmarshal;
use bytes;
use bytes::{Bytes, BytesMut};

pub fn handle_incoming_rtp() {
    decode()
}

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