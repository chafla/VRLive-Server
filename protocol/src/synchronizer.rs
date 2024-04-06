use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::time::{Duration};

use log::{debug, error, info, trace, warn};
use rosc::OscBundle;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::time::Instant;
use webrtc::rtp::packet::Packet as Packet;

use crate::vrl_packet::{OscData, RTPPacket, RTPStreamInfo, VRTPPacket};

static AUDIO_TIMEOUT: Duration = Duration::from_millis(100);

/// Maximum heap size before we start purging it to save memory
static MAX_HEAP_SIZE: usize = 2000;

static MAX_MOCAP_PACKETS_PER_PACKET: usize = 1;

/// Mocap packets above this size will be warned about.
static MOCAP_OUTPUT_WARNING_SIZE: usize = 2000;

/// Hard maximum for mocap packet size.
static MOCAP_OUTPUT_HARD_LIMIT: usize = 9860;

static MOCAP_SEND_AFTER_PRESSURE_REACHES: usize = 5;

static WAIT_FOR_AUDIO_TO_SEND: bool = true;
// use crate::VRTPPacket;

pub struct Synchronizer {
    /// The channel used to send combined data out to the main server
    combined_out: Sender<VRTPPacket>,
    /// Two heaps, sorted by timestamps.
    /// Should automatically keep our data sorted as it is ingested.
    /// Note that we wrap these in reverse to make them min-heaps instead of max heaps.
    audio_heap: BinaryHeap<Reverse<RTPPacket>>,
    mocap_heap: BinaryHeap<Reverse<OscData>>,
    /// The time at which the synchronizer was started. Used for re-syncing.
    zero_time: u32,
    /// The ID of the user who owns this synchronizer.
    user_id: u16,
}

impl Synchronizer {
    pub fn new(audio_out: &Sender<VRTPPacket>, user_id: u16) -> Self {
        Self {
            combined_out: audio_out.clone(),
            audio_heap: BinaryHeap::new(),
            mocap_heap: BinaryHeap::new(),
            zero_time: 0,
            user_id,
        }
    }

    // pub async fn start(mocap_in: Receiver<OscBundle>, audio_in: Receiver<Packet>, combined_out: Sender<VRTPPacket>) {
    //
    // }

    pub async fn output(&mut self) {}

    /// Take in the data and handle it appropriately
    pub async fn intake(&mut self, mut mocap_in: Receiver<OscBundle>, mut audio_in: Receiver<Packet>, audio_clock_rate: f32) {
        // RTP meta stores meta information about the stream, such as t=0,
        // so we create it the first time and then just keep re-using it.
        let mut stream_info: Option<RTPStreamInfo> = None;
        // Timestamp of the last stream we handled
        // let mut last_handled_timestamp = SystemTime::now();

        // analytics
        let mut durs_micro = 0;
        let mut n_packets_through = 0;
        let mut last_time = Instant::now();

        // let mut last_audio_timestamp = 0;
        // let mut audio_duration_ts = 0;
        // let mut n_audio_packets = 0;
        // 
        // let mut last_mocap_timestamp = 0;
        // let mut mocap_duration_ts = 0;
        // let mut n_mocap_packets = 0;

        let mut try_to_send = false;
        loop {
            tokio::select! {
                biased;  // make sure audio gets handled first
                audio_m = tokio::time::timeout(AUDIO_TIMEOUT, audio_in.recv()) => {
                    match audio_m {
                        Err(e) => {
                            if !self.mocap_heap.is_empty() {
                                debug!("Audio data timed out after {e}, but mocap pressure exists. sending mocap data anyway");
                                // Timed out, but we didn't get any audio data...
                                // in the meantime, send what mocap data we have
                                try_to_send = true;
                            }
                            
                        },
                        Ok(audio) => {
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
                                    // todo ensure we don't include any mocap messages that are older than a certain threshold
                                    let cur_ts = d.header.timestamp as u64;


                                    // trace!("dt: {}", cur_ts - last_audio_timestamp);
                                    // if n_audio_packets % 500 == 0 {
                                    //     audio_duration_ts = 0;  // reset it so we don't stupidly overflow
                                    // }
                                    // audio_duration_ts += cur_ts - last_audio_timestamp;
                                    // last_audio_timestamp = d.header.timestamp as u64;
                                    // n_audio_packets += 1;
                                    trace!("cur_ts: {cur_ts}");

                                    // End analytics!


                                    let rtp = RTPPacket::new(
                                        d,
                                        working_meta
                                    );

                                    self.audio_heap.push(Reverse(rtp));

                                    // dbg!(&rtp);

                                    // Audio events will probably come in a bit less frequently than mocap.
                                    // This is currently the assumption that I'm making, at least.
                                    // When a new audio packet comes in, we'll clear the existing mocap buffer and send the data
                                    // along with the mocap
                                    try_to_send = true;


                                    // self.audio_heap.push(Reverse(rtp));

                                    // SynchronizerPacket::Audio(rtp)
                                }
                            }
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
                            let osc_pkt = OscData::new(d, self.user_id);
                            // insert the mocap packet.
                            // note that we wrap it in reverse
                            self.mocap_heap.push(Reverse(osc_pkt));
                            // osc_pkt
                        }
                    };
                    if !WAIT_FOR_AUDIO_TO_SEND || MOCAP_SEND_AFTER_PRESSURE_REACHES > self.mocap_heap.len() {
                        try_to_send = true;
                    }
                },

            }
            if try_to_send {
                let audio_packet = match self.audio_heap.pop() {
                    Some(Reverse(p)) => Some(p),
                    None => None,
                };
                // if let Some(Reverse(audio_pkt)) = self.audio_heap.pop() {
                let mut mocap_parts = vec![];
                let mut mocap_count = 0;
                if !self.mocap_heap.is_empty() {
                    // extract all of the mocap packets from the heap in order, and pull them into
                    while let Some(Reverse(x)) = self.mocap_heap.pop() {

                        // dbg!(&x);
                        mocap_parts.push(x);
                        mocap_count += 1;
                        // if (mocap_count > MAX_MOCAP_PACKETS_PER_PACKET) {
                        //     break;
                        // }
                    }
                }

                

                if let Err(e) = self.combined_out.send(VRTPPacket::Raw(mocap_parts, audio_packet, self.user_id)).await {
                    error!("Failed to send to sync out: channel likely closed ({e})");
                    break;
                }
                try_to_send = false;
                // }


            }


            // more analytics
            if n_packets_through % 500 == 0 {
                info!("Current pressure:");
                info!("Audio: {}", self.audio_heap.len());
                info!("Mocap: {}", self.mocap_heap.len())
            }
            
            // if our heaps get too large (likely due to mocap not being consumed),
            // drain the heaps so we can 
            
            if self.audio_heap.len() > MAX_HEAP_SIZE {
                warn!("Audio heap had over {MAX_HEAP_SIZE} items, draining {} items!", self.audio_heap.len());
                self.audio_heap.drain();
            }
            
            if self.mocap_heap.len() > MAX_HEAP_SIZE {
                warn!("Mocap heap had over {MAX_HEAP_SIZE} items, draining {} items!", self.mocap_heap.len());
                self.mocap_heap.drain();
            }

            n_packets_through += 1;
            let cur_time = Instant::now();
            let dur = cur_time - last_time;
            trace!("Last packet took {}ms", dur.as_micros() as f64 / 100.0);
            durs_micro += dur.as_millis();
            last_time = cur_time;


            // trace!("Average packet time is currently {}ms", (durs_micro / n_packets_through) as f64);
            // if n_audio_packets > 0 {
            //     trace!("Average audio timestamp difference is {}", (audio_duration_ts / n_audio_packets) as f64);
            // }
        }
    }
}