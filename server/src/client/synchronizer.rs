use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use log::warn;
use rosc::{OscBundle, OscPacket};
use tokio::sync::mpsc::{Sender, Receiver};
use webrtc::rtp::packet::Packet as Packet;
use crate::VRTPPacket;

/// Metadata about the current RTP stream.
#[derive(Copy, Clone, Debug)]
pub struct RTPMeta {
    /// Timestamps for RTP headers can have a random start, so this stores t=0.
    /// This should be in unix time
    zero_time: SystemTime,
    /// Clock rates are variable as well. Should give us a good safe conversion factor
    clock_rate: f32
}


impl RTPMeta {
    pub fn new(unix_time_ns: u64, clock_rate: f32) -> Self {
        Self {
            zero_time: UNIX_EPOCH + Duration::from_nanos(unix_time_ns),
            clock_rate
        }
    }
}

#[derive(Clone, Debug)]
pub struct RTPPacket {
    packet: Packet,
    meta: RTPMeta
}

impl RTPPacket {
    pub fn new(packet: Packet, meta: RTPMeta) -> Self {
        Self {
            packet,
            meta
        }
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


#[derive(Clone, Debug)]
pub enum SynchronizerPacket {
    Mocap(OscData),
    /// An RTP packet, storing both its initial timestamp a
    Audio(RTPPacket)
}
//
// impl SynchronizerPacket {
//     pub fn timestamp(&self) -> SystemTime {
//         match self {
//             SynchronizerPacket::Mocap(bundle) => bundle.timetag.into(),
//             // TODO determine what the proper conversion factor would be here
//             SynchronizerPacket::Audio((meta, pkt)) => {
//                 // time offset since the zero time
//                 let time_offset_secs = pkt.header.timestamp as f32 / meta.clock_rate;
//                 let time_offset_nanos = time_offset_secs * 1e6;
//                 let y = meta.zero_time + Duration::from_nanos(time_offset_nanos as u64);
//                 return y
//             }
//         }
//     }
// }
//
// impl PartialEq for SynchronizerPacket {
//     fn eq(&self, other: &Self) -> bool {
//         match (self, other) {
//             (SynchronizerPacket::Mocap(b1), SynchronizerPacket::Mocap(b2)) => {
//                 return b1.timetag == b2.timetag
//             },
//             (a1@SynchronizerPacket::Audio((_)), a2@SynchronizerPacket::Audio(_)) => {
//                 a1.timestamp() == a2.timestamp()
//             }
//         }
//     }
// }
//
// impl Eq for SynchronizerPacket {
//
//
// }
//
// impl PartialOrd for SynchronizerPacket {
//     // fn cmp(&self, other: &Self) -> Ordering {
//     //     return self.timestamp().cmp(other.timestamp())
//     // }
//
//     fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
//         return self.timestamp().partial_cmp(&other.timestamp())
//     }
// }
//
//
// impl Ord for SynchronizerPacket {
//     fn cmp(&self, other: &Self) -> Ordering {
//         return self.timestamp().cmp(&other.timestamp())
//     }
// }

#[derive(Clone, Debug)]
pub struct OscData {
    packet: OscBundle
}

impl From<OscBundle> for OscData {
    fn from(packet: OscBundle) -> Self {
        Self { packet }
    }
}

impl PartialEq for OscData {
    fn eq(&self, other: &Self) -> bool {
        self.packet.timetag.eq(&other.packet.timetag)
    }
}

// default
impl Eq for OscData { }

impl Ord for OscData {
    fn cmp(&self, other: &Self) -> Ordering {
        self.packet.timetag.cmp(&other.packet.timetag)
    }
}

impl PartialOrd for OscData {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.packet.timetag.partial_cmp(&other.packet.timetag)
    }
}

// impl PartialOrd for OscData {
//     fn cmp(&self, other: &Self) -> Ordering {
//         todo!()
//     }
// }

pub struct Synchronizer {
    // mocap_in: Receiver<OscBundle>,
    // audio_in: Receiver<RTPPacket>,
    // combined_out: Sender<VRTPPacket>,
    /// Two heaps, sorted by timestamps.
    /// Should automatically keep our data sorted as it is ingested.
    audio_heap: BinaryHeap<RTPPacket>,
    mocap_heap: BinaryHeap<OscData>,
    /// The time at which the synchronizer was started. Used for re-syncing.
    zero_time: u32
}

impl Synchronizer {

    pub fn new() -> Self {
        Self {
            // mocap_in,
            // audio_in,
            // combined_out,
            audio_heap: BinaryHeap::new(),
            mocap_heap: BinaryHeap::new(),
            zero_time: 0
        }
    }

    pub async fn start(mocap_in: Receiver<OscBundle>, audio_in: Receiver<Packet>, combined_out: Sender<VRTPPacket>) {

    }

    /// Take in the data and handle it appropriately
    pub async fn intake(&mut self, mut mocap_in: Receiver<OscBundle>, mut audio_in: Receiver<Packet>, audio_clock_rate: f32) {
        let mut rtp_meta_base: Option<RTPMeta> = None;
        let mut last_handled_timestamp = SystemTime::now();
        loop {
            let sync_packet = tokio::select! {
                mocap = mocap_in.recv() => {
                    let data = match mocap {
                        None => {
                            warn!("Sync mocap in shutting down");
                            break
                        }
                        Some(d) => {
                            OscData::from(d)
                        }
                    };
                    SynchronizerPacket::Mocap(data)
                },
                audio = audio_in.recv() => {
                    match audio {
                        None => {
                            warn!("Sync audio in shutting down");
                            break
                        }
                        Some(d) => {
                            let working_meta = match rtp_meta_base {
                                Some(m) => m,
                                None => {
                                    let new_base = RTPMeta::new(
                                            d.header.timestamp as u64,
                                            audio_clock_rate
                                        );
                                    rtp_meta_base = Some(new_base);
                                    new_base
                                }

                            };
                            SynchronizerPacket::Audio(RTPPacket::new(
                                d,
                                working_meta
                            ))
                        }
                    }
                },

            };

            dbg!(&sync_packet);

            // if self.audio_heap.len() >



            // match sync_packet




        }

    }

}