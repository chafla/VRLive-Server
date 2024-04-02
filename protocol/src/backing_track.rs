use std::env;
use std::path::Path;

use bytes::{BufMut, Bytes, BytesMut};
use tokio::fs::File;
use tokio::io;
use tokio::io::AsyncReadExt;
use crate::vrl_packet::VRTPPacket;
use crate::VRLTCPPacket;

static BACKING_TRACK_DIR_ENV: &str = "BACKING_TRACK_DIR";

/// Data that will be sent along for backing track messages.
/// Will probably just be a string describing the local file path for the song in use
// pub type BackingTrackData = String;

fn get_backing_track_directory() -> String {
    // TODO find a way to clean this up
    env::var(BACKING_TRACK_DIR_ENV).unwrap_or_else(|_| env::current_dir().unwrap().into_os_string().into_string().unwrap())

}

/// Load in a backing track from a given file path.
/// It should be found within the environment variable marked directory,
/// which currently just maps to songs
// pub async fn load_backing_track(backing_track_name: &str) -> Result<impl Read + Send, String> {
//     let backing_track_dir = get_backing_track_directory();
//
//     let dir = env::join_paths(vec![&backing_track_dir, backing_track_name]);
//     if let Err(e) = dir {
//         return Err(format!("Joining paths failed: {e}"));
//     }
//
//     let dir = dir.unwrap().into_string().unwrap();
//
//     let file = File::open(dir).await;
//
//     Ok()
// }

#[derive(Clone, Debug)]
pub struct BackingTrackData {
    data: Bytes,
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
    
    fn get_filename(&self) -> &str {
        &self.filename
    }
}