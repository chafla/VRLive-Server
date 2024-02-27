use std::cmp::{Ordering, Reverse};
use std::collections::BinaryHeap;
use std::time::SystemTime;

use bytes::Bytes;
use log::{error, info, trace, warn};
use rosc::{OscBundle, OscPacket};
use rosc::encoder::encode;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::time::Instant;
use webrtc::rtp::packet::Packet as Packet;

use crate::VRTPPacket;

/// Metadata about the current RTP stream.
#[derive(Copy, Clone, Debug)]
pub struct RTPMeta {
    /// Timestamps for RTP headers can have a random start, so this stores t=0.
    /// This should be in unix time
    zero_time: u32,
    /// Clock rates are variable as well. Should give us a good safe conversion factor
    clock_rate: f32
}


impl RTPMeta {
    pub fn new(unix_time_ns: u32, clock_rate: f32) -> Self {
        Self {
            zero_time: unix_time_ns,
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
    bundle: OscBundle
}

// impl OscData {
//     fn as_packet(&mut self) -> &mut OscPacket {
//         OscPacket::Bundle(self.bundle)
//     }
// }

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

// impl PartialOrd for OscData {
//     fn cmp(&self, other: &Self) -> Ordering {
//         todo!()
//     }
// }

pub struct Synchronizer {
    // mocap_in: Receiver<OscBundle>,
    // audio_in: Receiver<RTPPacket>,
    combined_out: Sender<VRTPPacket>,
    /// Two heaps, sorted by timestamps.
    /// Should automatically keep our data sorted as it is ingested.
    audio_heap: BinaryHeap<Reverse<RTPPacket>>,
    mocap_heap: BinaryHeap<Reverse<OscData>>,
    /// The time at which the synchronizer was started. Used for re-syncing.
    zero_time: u32
}

impl Synchronizer {

    pub fn new(audio_out: &Sender<VRTPPacket>) -> Self {
        Self {
            // mocap_in,
            // audio_in,
            // combined_out,
            combined_out: audio_out.clone(),
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

        // analytics
        let mut durs_micro = 0;
        let mut n_packets_through = 0;
        let mut last_time = Instant::now();

        let mut last_audio_timestamp = 0;
        let mut audio_duration_ts = 0;
        let mut n_audio_packets = 0;

        let mut last_mocap_timestamp = 0;
        let mut mocap_duration_ts = 0;
        let mut n_mocap_packets = 0;
        loop {
            let sync_packet = tokio::select! {
                biased;  // make sure audio gets handled first
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
                                            d.header.timestamp,
                                            audio_clock_rate
                                        );
                                    rtp_meta_base = Some(new_base);
                                    new_base
                                }

                            };
                            let cur_ts = d.header.timestamp as u64;

                            trace!("dt: {}", cur_ts - last_audio_timestamp);
                            if n_audio_packets % 500 == 0 {
                                audio_duration_ts = 0;  // reset it so we don't stupidly overflow
                            }
                            audio_duration_ts += (cur_ts - last_audio_timestamp);
                            last_audio_timestamp = d.header.timestamp as u64;
                            n_audio_packets += 1;
                            trace!("cur_ts: {cur_ts}");

                            let rtp = RTPPacket::new(
                                d,
                                working_meta
                            );

                            let mut mocap_parts = vec![];
                            if !self.mocap_heap.is_empty() {
                                // time to pull all the mocap packets into one bunch

                                while let Some(Reverse(x)) = self.mocap_heap.pop() {
                                    mocap_parts.push(x)
                                }
                            }

                            if let Err(e) = self.combined_out.send(VRTPPacket::Raw(mocap_parts, rtp)).await {
                                error!("Failed to send to sync out: channel likely closed ({e})");
                                break;
                            }

                            // self.audio_heap.push(Reverse(rtp));

                            // SynchronizerPacket::Audio(rtp)
                        }
                    }
                },

                mocap = mocap_in.recv() => {
                    let data = match mocap {
                        None => {
                            warn!("Sync mocap in shutting down");
                            break
                        }
                        Some(d) => {
                            let osc_pkt = OscData::from(d);
                            self.mocap_heap.push(Reverse(osc_pkt));
                            // osc_pkt
                        }
                    };
                    // SynchronizerPacket::Mocap(data)
                },

            };

            if (n_packets_through % 500 == 0) {
                info!("Current pressure:");
                info!("Audio: {}", self.audio_heap.len());
                info!("Mocap: {}", self.mocap_heap.len())
            }

            n_packets_through += 1;
            let cur_time = Instant::now();
            let dur = cur_time - last_time;
            trace!("Last packet took {}ms", dur.as_micros() as f64 / 100.0);
            durs_micro += dur.as_millis();
            last_time = cur_time;


            // debug!("")
            trace!("Average packet time is currently {}ms", (durs_micro / n_packets_through) as f64);
            if n_audio_packets > 0 {
                trace!("Average audio timestamp difference is {}", (audio_duration_ts / n_audio_packets) as f64);
            }




            // dbg!(&sync_packet);

            // if self.audio_heap.len() >



            // match sync_packet




        }

    }

}