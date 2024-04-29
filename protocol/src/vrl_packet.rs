use std::cmp::Ordering;
use std::collections::HashSet;
use std::mem::{size_of};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use bytes::{BufMut, Bytes, BytesMut};
use log::{warn};
use rosc::{OscArray, OscBundle, OscMessage, OscPacket, OscTime, OscType};
use rosc::encoder::encode;
use webrtc::rtp::packet::Packet;

use crate::UserIDType;
use crate::vrm_packet::{convert_to_vrm_base, convert_to_vrm_do_nothing, filter_vrm, FILTERED_VRM_BONES};

const ALERT_ON_FRAGMENTATION: bool = false;

const VRL_RTP_PACKET_HEADER_ID: u16 = 5653;

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
    pub fn timestamp(&self) -> u32 {
        self.packet.header.timestamp + self.meta.zero_time
    }
    
    fn calculate_timestamp_offset_seconds(&self) -> f64 {
        self.packet.header.timestamp as f64 / self.sample_rate() as f64 + self.meta.zero_time as f64
    }

    pub fn osc_timestamp(&self) -> OscTime {
        osc_timestamp_from_unix(self.packet.header.timestamp as u64)
    }

    pub fn packet(&self) -> &Packet {
        &self.packet
    }

    pub fn metadata(&self) -> &RTPStreamInfo {
        &self.meta
    }
}

#[allow(dead_code)]
pub fn get_current_timestamp() -> OscTime {
    calculate_osc_timestamp(SystemTime::now().duration_since(UNIX_EPOCH).expect("Time is backwards somehow"))
}

#[allow(dead_code)]
pub fn osc_timestamp_from_unix(timestamp: u64) -> OscTime {
    // get the current clip's duration in seconds
    calculate_osc_timestamp(Duration::from_secs(timestamp))
}

fn calculate_osc_timestamp(duration_since_epoch: Duration) -> OscTime {
    // we're assuming that our RTP timestamps are the time since 1/1/1970, but osc is weird
    // and uses 1/1/1900 as its epoch, so we need to add a few extra seconds.
    let seconds_inbetween = 2_208_988_800f64;  // time between the two dates.
    let frac_secs = duration_since_epoch.as_secs_f64() + seconds_inbetween;
    let res = (frac_secs.trunc() as u32, (frac_secs.fract() * 10e8) as u32).into();
    res
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
    pub bundle: OscBundle,
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

                let mut backing_track_position = -1f32;

                if let Some(pkt) = &rtp {
                    // TODO deal with this sample rate
                    first_timestamp = Some(pkt.osc_timestamp());
                    if pkt.packet.header.extension && pkt.packet.header.extension_profile == VRL_RTP_PACKET_HEADER_ID {
                        // we are working with a proper VRL RTP packet, the backing track position is stored within
                        // let's extract it
                        let vrtp_extension = &pkt.packet.header.extensions[0];
                        let bytes: [u8; 4] = vrtp_extension.payload[0..4].try_into().expect("Were not four bytes in this 4 byte range (somehow");
                        backing_track_position = f32::from_le_bytes(bytes);
                        // dbg!(&backing_track_position);
                    }

                }
                

                let mut bytes_out = BytesMut::with_capacity(5000);
                // let osc_bytes = encode(&osc).unwrap();
                let mut osc_bytes = BytesMut::new();
                let mut bundle_packets : Vec<OscPacket> = vec![];


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
                
                if bundle_packets.len() > 0 {
                    if first_timestamp.is_none() {
                        // we don't have any timestamps???
                        // TODO find out how we best want to handle this???
                        panic!("No timestamp, but we have packets of some kind!")
                    }

                    let outer_bundle = OscBundle {
                        timetag: first_timestamp.unwrap(),
                        content: bundle_packets
                    };

                    // TODO try parsing the message to determine what kind it is
                    let outer_bundle = match convert_to_vrm_base(&outer_bundle) {
                        Err(_) => {
                            // !("{e} Failed to convert from slime bone type, will send raw OSC as-is! VRM!");
                            convert_to_vrm_do_nothing(&outer_bundle)
                        },
                        Ok(b) => b
                    };
                    let filtered_bone_list = HashSet::from(FILTERED_VRM_BONES);
                    let outer_bundle = filter_vrm(outer_bundle, &filtered_bone_list);

                    let bundle_packed = encode(&OscPacket::Bundle(outer_bundle)).unwrap();
                    osc_bytes.put(bundle_packed.as_slice());
                }
                
                
                // 4 from
                // 2 - 16 bit
                let pkt_size =
                        audio_size          // size of the audio payload
                        + osc_bytes.len()          // size of the osc payload
                        + size_of::<u32>()  // size of this variable, pkt_size
                        + size_of::<u16>()  // size of audio_size
                        + size_of::<u16>()  // size of osc_size
                        + size_of::<u16>()  // size of user_id
                        + size_of::<f32>(); // size of backing track offset


                // provide two bytes for the size of our total payload

                // TODO find a way to ensure packets don't exceed mtu
                // assert!(pkt_size < 1400);

                
                // let mut bytes_out = BytesMut::with_capacity(pkt_size);
                // preface with the total sizes
                bytes_out.put_u32(pkt_size as u32);
                bytes_out.put_u16(osc_bytes.len() as u16);
                bytes_out.put_u16(audio_size as u16);
                bytes_out.put_u16(user_id as u16);
                bytes_out.put_f32(backing_track_position);
                
                bytes_out.put(osc_bytes);

                if let Some(pkt) = &rtp {
                    bytes_out.put(&pkt.packet.payload[..]);
                }

                assert_eq!(pkt_size, bytes_out.len());


                if bytes_out.len() > 1400 && ALERT_ON_FRAGMENTATION {
                    warn!("An outgoing packet was over 1400 bytes in size (actual size: {}) and may be fragmented!", bytes_out.len());
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



/// Estimate the size of an OSC bundle, given the spec.
pub fn estimate_bundle_size(bundle: &OscBundle) -> usize {
    let mut size = 0;

    size += 8;  // #bundle
    size += 8;  // timetag

    for pkt in &bundle.content {
        size += 4;  // i32 for bundle element size
        size += match pkt {
            OscPacket::Message(msg) => estimate_message_size(msg),
            OscPacket::Bundle(b) => estimate_bundle_size(b)
        }
        // size += estimate_message_size(message);
    }


    size
}

/// Provide an estimate for the message size.
/// This may not be completely accurate, as I might have failed to account for lengths.
pub fn estimate_message_size(message: &OscMessage) -> usize {
    let mut size = 0;
    let addr_len = message.addr.len();
    size += addr_len + 4 % addr_len;
    let mut type_tag_size = 4;  // starts with a comma
    for arg in &message.args {
        type_tag_size += 4;  // character for each!
        size += estimate_osc_type_size(arg);
    }

    size + type_tag_size
}

fn estimate_osc_array_size(osc_array: &OscArray) -> usize {
    let type_tag_size = 8;  // open, close
    let mut final_size = 0;
    for item in &osc_array.content {
        final_size += match item {
            OscType::Array(a) => {
                estimate_osc_array_size(a)
            },
            etc => estimate_osc_type_size(etc),
        }
    }

    final_size + type_tag_size
}

fn estimate_osc_type_size(osc_type: &OscType) -> usize {
    match osc_type {
        // see https://opensoundcontrol.stanford.edu/spec-1_0.html
        OscType::Time(_) => 8,
        OscType::Int(_) => 4,
        OscType::Float(_) => 4,
        OscType::String(s) => {
            let base_len = s.len();
            base_len + 4 % base_len  // needs to be a multiple of 4
        },
        OscType::Blob(b) => b.len(),
        OscType::Long(_) => 8,
        OscType::Double(_) => 8,
        OscType::Char(_) => 4,
        OscType::Color(_) => 4,
        OscType::Midi(_) => 4,
        OscType::Bool(_) => 0,  // allocates nothing! neato
        // array is a bit busier, and may not be completely accurate (especially if they're nested
        // FIXME
        // add 4 onto the end tho to represent the closing version 
        OscType::Array(a) => 4 + estimate_osc_array_size(a),
        OscType::Nil => 0,
        OscType::Inf => 0,
    }
}


#[cfg(test)]
mod test {
    // use super::*;

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