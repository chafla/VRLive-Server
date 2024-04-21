use std::str::from_utf8;
use bytes::{Buf, BufMut, Bytes, BytesMut};

/// The standardized packet format we use for sending certain kinds of TCP data out to clients.
/// Note that the "header" length is the length of the proper header field.
pub struct VRLTCPPacket {
    /// Length of our message type
    header_msg_len: u8,
    /// String message marking our message type.
    header_msg: String,
    /// Length of the header (not including the lengths and header message) in bytes.
    /// You can store whatever information you want in here, just make sure it's accounted for.
    header_len: u16,
    /// Length of the body payload in bytes.
    body_len: u32,
    /// Header, containing any information you want to stash in here.
    /// Note that in the message this will be offset after header_msg, header_len, and body_len.
    header: Bytes,
    /// Payload
    body: Bytes
}

impl VRLTCPPacket {
    pub fn new(header_msg: &str, header: Bytes, body: Bytes) -> Self {
        Self {
            header_msg_len: header_msg.len() as u8,
            header_msg: header_msg.into(),
            header_len: header.len() as u16,
            header,
            body_len: body.len() as u32,
            body
        }
    }
}

impl From<Bytes> for VRLTCPPacket {
    fn from(mut value: Bytes) -> Self {
        let message_type_len = value.get_u8();
        let title = &value[0..message_type_len as usize];
        let title = from_utf8(title).unwrap().to_owned();
        let header_len = value.get_u16();
        let body_len = value.get_u32();
        // header offset doesn't include body/header sizes
        let header_offset = message_type_len as usize + 1 + 2 + 4;
        let body_offset = header_offset + header_len as usize;
        let header = &value[header_offset..body_offset];
        // let header = Bytes::from(header);
        let body = &value[body_offset..body_offset + body_len as usize];

        Self {
            header_msg_len: message_type_len,
            header_msg: title,
            header_len,
            body_len,
            header: Bytes::copy_from_slice(header),
            body: Bytes::copy_from_slice(body)
        }
    }
}

impl From<VRLTCPPacket> for Bytes {
    fn from(value: VRLTCPPacket) -> Self {
        let mut out_bytes = BytesMut::with_capacity(value.body_len as usize + value.header_len as usize + value.header_msg_len as usize + 7);
        out_bytes.put_u8(value.header_msg_len);
        out_bytes.put(value.header_msg.as_bytes());
        out_bytes.put_u16(value.header_len);
        out_bytes.put_u32(value.body_len);
        out_bytes.put(value.header);
        out_bytes.put(value.body);

        return out_bytes.freeze();
    }
}
