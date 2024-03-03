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
    bundle: OscBundle
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

pub struct Synchronizer {
    /// The channel used to send combined data out to the main server
    combined_out: Sender<VRTPPacket>,
    /// Two heaps, sorted by timestamps.
    /// Should automatically keep our data sorted as it is ingested.
    /// Note that we wrap these in reverse to make them min-heaps instead of max heaps.
    audio_heap: BinaryHeap<Reverse<RTPPacket>>,
    mocap_heap: BinaryHeap<Reverse<OscData>>,
    /// The time at which the synchronizer was started. Used for re-syncing.
    zero_time: u32
}

impl Synchronizer {

    pub fn new(audio_out: &Sender<VRTPPacket>) -> Self {
        Self {
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
        // RTP meta stores meta information about the stream, such as t=0,
        // so we create it the first time and then just keep re-using it.
        let mut stream_info: Option<RTPStreamInfo> = None;
        // Timestamp of the last stream we handled
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
                        // Data in is an audio packet
                        Some(d) => {
                            // If we don't have any info on the stream, grab info from this packet
                            let working_meta = match stream_info {
                                Some(m) => m,
                                None => {
                                    // We don't have an existing
                                    let new_base = RTPStreamInfo::new(
                                            d.header.timestamp,
                                            audio_clock_rate
                                        );
                                    stream_info = Some(new_base);
                                    new_base
                                }

                            };
                            // grab some analytics
                            // todo add a server analytics channel with a bunch of enums to track pressure
                            let cur_ts = d.header.timestamp as u64;


                            trace!("dt: {}", cur_ts - last_audio_timestamp);
                            if n_audio_packets % 500 == 0 {
                                audio_duration_ts = 0;  // reset it so we don't stupidly overflow
                            }
                            audio_duration_ts += (cur_ts - last_audio_timestamp);
                            last_audio_timestamp = d.header.timestamp as u64;
                            n_audio_packets += 1;
                            trace!("cur_ts: {cur_ts}");

                            // End analytics!


                            let rtp = RTPPacket::new(
                                d,
                                working_meta
                            );

                            // Audio events will probably come in a bit less frequently than mocap.
                            // This is currently the assumption that I'm making, at least.
                            // When a new audio packet comes in, we'll clear the existing mocap buffer and send the data
                            // along with the mocap
                            let mut mocap_parts = vec![];
                            if !self.mocap_heap.is_empty() {
                                // extract all of the mocap packets from the heap in order, and pull them into
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
                    // mocap data in. Just pull it in and add it to the heap.
                    match mocap {
                        None => {
                            warn!("Sync mocap in shutting down");
                            break
                        }
                        Some(d) => {
                            let osc_pkt = OscData::from(d);
                            // insert the mocap packet.
                            // note that we wrap it in reverse
                            self.mocap_heap.push(Reverse(osc_pkt));
                            // osc_pkt
                        }
                    };
                },

            };

            // more analytics
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


            trace!("Average packet time is currently {}ms", (durs_micro / n_packets_through) as f64);
            if n_audio_packets > 0 {
                trace!("Average audio timestamp difference is {}", (audio_duration_ts / n_audio_packets) as f64);
            }

        }

    }

}