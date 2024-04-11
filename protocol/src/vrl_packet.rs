use std::cmp::Ordering;
use std::mem::{size_of};

use bytes::{BufMut, Bytes, BytesMut};
use log::warn;
use rosc::{OscBundle, OscPacket, OscTime};
use rosc::encoder::encode;
use webrtc::rtp::packet::Packet;

use crate::UserIDType;
use crate::vrm_packet::{convert_to_vrm_do_nothing};

/// Metadata about the current RTP stream.
#[derive(Copy, Clone, Debug)]
pub struct RTPStreamInfo {
    /// Timestamps for RTP headers can have a random start, so this stores t=0.
    /// This should be in unix time
    zero_time: u32,
    /// Clock rates are variable as well. Should give us a good safe conversion factor
    clock_rate: f32
}


impl RTPStreamInfo {
    pub fn new(unix_time_ns: u32, clock_rate: f32) -> Self {
        Self {
            zero_time: unix_time_ns,
            clock_rate
        }
    }
}

/// Wrapper for an RTP packet, used to apply sorting traits
#[derive(Clone, Debug)]
pub struct RTPPacket {
    /// The actual packet that we're working with
    packet: Packet,
    /// Some metadata about the stream in general
    meta: RTPStreamInfo
}

impl RTPPacket {
    pub fn new(packet: Packet, meta: RTPStreamInfo) -> Self {
        Self {
            packet,
            meta
        }
    }

    pub fn sample_rate(&self) -> u32 {
        48000  // FIXME AAARGGHH THIS SHOULD NOT BE HARDCODED BUT SHOULD INSTEAD COME FROM THE HANDSHAKE
    }

    /// Raw timestamp since zero time
    /// TODO figure out how this corresponds to the sample rate of the audio
    pub fn timestamp(&self) -> u32 {
        self.packet.header.timestamp + self.meta.zero_time
    }

    pub fn osc_timestamp(&self) -> OscTime {
        // get the current clip's duration in seconds

        // TODO
        // assuming the timestamp is counted in terms of samples since the song began
        // though since we mostly control the 
        // let current_song_duration = self.timestamp() as f64 / self.sample_rate() as f64;

        return (0, 0).into()


    }

    pub fn packet(&self) -> &Packet {
        &self.packet
    }

    pub fn metadata(&self) -> &RTPStreamInfo {
        &self.meta
    }
}

impl PartialEq for RTPPacket {
    fn eq(&self, other: &Self) -> bool {
        self.packet.header.timestamp == other.packet.header.timestamp
    }
}

impl Eq for RTPPacket {}

impl PartialOrd for RTPPacket {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for RTPPacket {
    fn cmp(&self, other: &Self) -> Ordering {
        self.packet.header.timestamp.cmp(&other.packet.header.timestamp)
    }
}


/// Outer wrapper for organizing data emerging from the synchronizer
#[derive(Clone, Debug)]
pub enum SynchronizerPacket {
    Mocap(OscData),
    /// An RTP packet
    Audio(RTPPacket)
}

/// Wrapper for an OSC bundle.
/// Used to apply a sort-order to them based on timestamps.
#[derive(Clone, Debug)]
pub struct OscData {
    /// The user who is responsible for the creation of this bundle.
    pub user_id: UserIDType,
    pub bundle: OscBundle
}

impl OscData {
    pub fn bundle(&self) -> &OscBundle {
        &self.bundle
    }
    
    pub fn set_user(&mut self, user: UserIDType) {
        self.user_id = user;
    }
    
    pub fn new(bundle: OscBundle, user_id: UserIDType) -> Self {
        Self {
            user_id,
            bundle
        }
    }
}

impl From<OscData> for Bytes {
    fn from(value: OscData) -> Bytes {
        // TODO in a more perfect world, we would have encoded the User ID into the message address, but we would need to rework a substantial portion of what we have now to support that
        let uid =  value.user_id;
        let pkt = VRTPPacket::Raw(vec![value], None, uid);
        // let packet = OscPacket::Bundle(value.bundle);
        pkt.into()
    }
}

impl PartialEq for OscData {
    fn eq(&self, other: &Self) -> bool {
        self.bundle.timetag.eq(&other.bundle.timetag)
    }
}

// default
impl Eq for OscData { }

impl Ord for OscData {
    fn cmp(&self, other: &Self) -> Ordering {
        self.bundle.timetag.cmp(&other.bundle.timetag)
    }
}

impl PartialOrd for OscData {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}


#[derive(Debug, Clone)]
pub enum VRTPPacket {
    Encoded(Bytes),
    Raw(Vec<OscData>, Option<RTPPacket>, UserIDType)
}

impl From<VRTPPacket> for Bytes {
    fn from(value: VRTPPacket) -> Self {
        match value {
            VRTPPacket::Encoded(b) => b,
            VRTPPacket::Raw(osc_messages, rtp, user_id) => {

                // let mut first_timestamp: Option<OscTime> = None;

                let audio_size = match &rtp {
                    Some(pkt) => pkt.packet.payload.len(),
                    None => 0,
                };

                let mut first_timestamp: Option<OscTime> = None;

                if let Some(pkt) = &rtp {
                    // TODO deal with this sample rate
                    first_timestamp = Some(pkt.osc_timestamp())
                }

                let mut bytes_out = BytesMut::with_capacity(5000);
                // let osc_bytes = encode(&osc).unwrap();
                let mut osc_bytes = BytesMut::new();
                let mut bundle_packets : Vec<OscPacket> = vec![];
                let already_vrm = false;
                for osc in osc_messages.into_iter() {
                    if first_timestamp.is_none() {
                        first_timestamp = Some(*&osc.bundle.timetag)
                    }
                    let pkt = OscPacket::Bundle(osc.bundle);

                    bundle_packets.push(pkt);
                }


                if bundle_packets.len() == 0 && rtp.is_none() {
                    return Bytes::new()  // empty
                }

                if first_timestamp.is_none() {
                    // we don't have any timestamps???
                    // TODO find out how we best want to handle this???
                    panic!("No timestamp, but we have packets of some kind!")
                }
                let outer_bundle = OscBundle {
                    timetag: first_timestamp.unwrap(),
                    content: bundle_packets
                };
                let outer_bundle = convert_to_vrm_do_nothing(&outer_bundle);
                let bundle_packed = encode(&OscPacket::Bundle(outer_bundle)).unwrap();
                osc_bytes.put(bundle_packed.as_slice());
                let osc_size = osc_bytes.len();

                // 4 from
                // 2 - 16 bit
                let pkt_size =
                    audio_size
                        + osc_size
                        + size_of::<u16>()  // audio size's size
                        + size_of::<u16>()  // osc size's size
                        + size_of::<u32>()  // pkt_size proper
                        + size_of::<u16>();  // size of user id

                // provide two bytes for the size of our total payload

                // TODO find a way to ensure packets don't exceed mtu
                // assert!(pkt_size < 1400);



                // let mut bytes_out = BytesMut::with_capacity(pkt_size);
                // preface with the total sizes
                bytes_out.put_u32(pkt_size as u32);
                bytes_out.put_u16(osc_size as u16);
                bytes_out.put_u16(audio_size as u16);
                bytes_out.put_u16(user_id as u16);

                // dbg!(&osc_bytes);

                bytes_out.put(osc_bytes);

                if let Some(pkt) = &rtp {
                    bytes_out.put(&pkt.packet.payload[..]);
                }

                assert_eq!(pkt_size, bytes_out.len());


                if bytes_out.len() > 1400 {
                    warn!("An outgoing packet was over 1400 bytes in size (actual size: {}) and may be fragmented!", bytes_out.len())
                }


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
                // bytes_out.freeze();
                bytes_out.freeze()
            }
        }
    }
}

// pub struct CompressedOscMessage {
//
// }

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn try_decode_packet() {
        // todo
    }
    // fn try_create_server_message() {
    //     let message = ServerMessage::Performer(PerformerServerMessage::Ready(true));
    //
    //     let encoded = message.to_message(vec!["".to_owned()]);
    //     let unencoded = ServerMessage::from_osc_message(&encoded);
    //     dbg!(&unencoded);
    //     if let Some(server_msg) = &unencoded {
    //         let reencoded = server_msg.to_message(vec!["".to_owned()]);
    //         let reunencoded = ServerMessage::from_osc_message(&reencoded);
    //         assert_eq!(reencoded.addr, encoded.addr);
    //         assert_eq!(&reunencoded, &unencoded);
    //     }
    //     else {
    //         panic!("Object was not re-encoded")
    //     }
    // }
}