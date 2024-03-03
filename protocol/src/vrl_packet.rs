use std::cmp::Ordering;

use bytes::{BufMut, Bytes, BytesMut};
use rosc::{OscBundle, OscPacket};
use rosc::encoder::encode;
use webrtc::rtp::packet::Packet;
use webrtc::util::Marshal;

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

    /// Raw timestamp since zero time
    pub fn raw_timestamp(&self) -> u32 {
        self.packet.header.timestamp + self.meta.zero_time
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
        self.packet.header.timestamp.partial_cmp(&other.packet.header.timestamp)
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
    pub bundle: OscBundle
}

impl OscData {
    pub fn bundle(&self) -> &OscBundle {
        &self.bundle
    }
}


impl From<OscBundle> for OscData {
    fn from(packet: OscBundle) -> Self {
        Self { bundle: packet }
    }
}

impl Into<Bytes> for OscData {
    fn into(self) -> Bytes {
        let packet = OscPacket::Bundle(self.bundle);
        Bytes::from(encode(&packet).unwrap())
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
        self.bundle.timetag.partial_cmp(&other.bundle.timetag)
    }
}


#[derive(Debug, Clone)]
pub enum VRTPPacket {
    Encoded(Bytes),
    Raw(Vec<OscData>, RTPPacket)
}

impl TryInto<Bytes> for VRTPPacket {
    type Error = ();

    fn try_into(self) -> Result<Bytes, Self::Error> {
        match self {
            VRTPPacket::Encoded(b) => Ok(b),
            VRTPPacket::Raw(osc_messages, rtp) => {
                let mut bytes_out = BytesMut::with_capacity(1400 + 16);
                // let osc_bytes = encode(&osc).unwrap();
                let mut osc_bytes = BytesMut::new();
                for osc in osc_messages.into_iter() {
                    let pkt = OscPacket::Bundle(osc.bundle);
                    let bundle = encode(&pkt).unwrap();
                    osc_bytes.put(bundle.as_slice())
                }
                let osc_size = osc_bytes.len();


                let mut audio_bytes = BytesMut::new();
                let audio_size = match rtp.packet.marshal_to(&mut audio_bytes) {
                    Ok(s) => s,
                    Err(e) => return Err(())
                };

                let pkt_size = audio_size + osc_size + 2;

                // provide two bytes for the size of our total payload


                assert!(pkt_size < 1400);

                // let mut bytes_out = BytesMut::with_capacity(pkt_size);
                // preface with the total sizes
                bytes_out.put_u16(pkt_size as u16);
                bytes_out.put_u16(osc_size as u16);
                bytes_out.put_u16(audio_size as u16);

                bytes_out.put(osc_bytes);
                bytes_out.put(audio_bytes);



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
                Ok(bytes_out.freeze())
            }
        }
    }
}