use std::io;

use bytes::{BufMut, Bytes, BytesMut};
use rosc::encoder::encode;
use rosc::OscPacket;
use tokio::net::UdpSocket;
use tokio::sync::mpsc::Receiver;

/// Synchronized data representing the data occuring at one timestep
pub type SynchronizerData = (OscPacket, Bytes, f32);

/// screw it
/// braindead simple RTP sender.
/// Will send data out over the network, RTP be damned
pub struct RTPSenderOut<T: Send + Sync>
{
    socket: UdpSocket,
    data_in: Receiver<T>
}

impl <T: Send + Sync> RTPSenderOut<T> {

    pub fn new(socket: UdpSocket, data_in: Receiver<T>) -> Self {
        Self {
            socket,
            data_in
        }
    }

    pub async fn send(&mut self, data: Bytes) -> io::Result<usize> {
        self.socket.send(&data).await
    }
}

impl RTPSenderOut<SynchronizerData> {
    pub fn on_message(&self, data: SynchronizerData) -> Bytes {
        let (osc, audio, timestamp) = data;

        let osc_bytes = encode(&osc).unwrap();

        let min_data_size = osc_bytes.len() + audio.len() + timestamp.to_ne_bytes().len();

        // all the space we need + a bit of a buffer
        let mut bytes_out = BytesMut::with_capacity(min_data_size + 16);
        assert!(bytes_out.len() < 1400);  // it needs to be less than the standard mtu or we're in trouble

        // if it's larger we definitely have problems we need to clear up
        // anyway, preface with a u16 denoting the number of bytes the OSC message will take up
        bytes_out.put_u16(osc_bytes.len() as u16);
        // followed by the number of bytes the data itself will take
        bytes_out.put_u16(audio.len() as u16);
        // and lastly the timestamp in f32 form
        bytes_out.put_f32(timestamp);
        // now the data
        // osc first
        bytes_out.put(osc_bytes.as_slice());
        // followed soon after by the audio
        bytes_out.put(audio);

        // it's set, freeze it and punt it
        bytes_out.freeze()

    }
}

impl RTPSenderOut<Bytes> {
    pub fn on_message(&self, data: Bytes) -> Bytes {
        data
    }
}

// impl <T: Send + Sync> Streamer for RTPSenderOut<T> {
//     fn init() {
//     }
// }