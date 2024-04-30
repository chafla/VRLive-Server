use async_trait::async_trait;
use bytes::Bytes;

// These two traits represent different usable methods of sending and receiving data from clients.
// They exist so we can create a few different methods to possibly transmit data (be they raw RTP or WebRTC).

#[async_trait]
/// Trait for something that is capable of sending
pub trait Streamer : Sync + Send {

    /// Initialize the stream in some way.
    fn init();

    /// Send a data frame out to the target.
    async fn send_frame(&self, data: Bytes) -> anyhow::Result<()>;
}


#[async_trait]
pub trait StreamReceiver : Sync + Send {

    /// Initialize the stream in some way.
    fn init();

    /// Read in a frame.
    async fn read_frame() -> anyhow::Result<Bytes>;

}