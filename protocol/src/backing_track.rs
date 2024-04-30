/// Stuff for managing the backing track.
use std::path::Path;

use bytes::{BufMut, Bytes, BytesMut};
use tokio::fs::File;
use tokio::io;
use tokio::io::AsyncReadExt;
use crate::vrl_tcp_packet::VRLTCPPacket;

/// Data for the backing track.
#[derive(Clone, Debug)]
pub struct BackingTrackData {
    data: Bytes,
    #[allow(dead_code)]
    filename: String,
}

impl BackingTrackData {
    pub async fn open(filename: &str) -> io::Result<Self> {
        let mut file = File::open(filename).await?;
        let mut buf = Vec::<u8>::new();
        let _ = file.read_to_end(&mut buf).await?;
        // convert to utf-8
        let filename_header = Path::new(filename);
        
        let filename_bytes = filename_header.file_name().unwrap().as_encoded_bytes();
        // append the filename
        let mut header = BytesMut::with_capacity(10);
        header.put_u16(filename_bytes.len() as u16);
        header.put(filename_bytes);

        // let body_length = buf.len();
        
        let backing_track_data = VRLTCPPacket::new(
            "NEWTRACK",
            header.freeze(),
            Bytes::copy_from_slice(&buf)
        );

        Ok(Self {
            data: backing_track_data.into(),
            filename: filename.to_owned()
        })
    }
    
    pub fn get_data(&self) -> &Bytes {
        &self.data
    }

    #[allow(dead_code)]
    fn get_filename(&self) -> &str {
        &self.filename
    }
}