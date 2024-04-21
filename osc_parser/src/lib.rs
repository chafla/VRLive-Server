use std::ffi::{CStr, CString};
use interoptopus::{ffi_function, ffi_type, Inventory, InventoryBuilder, function};
use rosc::{decoder, OscBundle, OscMessage, OscPacket, OscType};
use rosc::decoder::decode_udp;

pub fn add(left: usize, right: usize) -> usize {
    left + right
}

// #[ffi_type]
// #[repr(C)]
// pub enum RawOscType {
//     Int(i32)
// }


#[ffi_type]
#[repr(C)]
pub struct FFIOscMessage {
    pub message_dest: *const u8,
    pub dest_length: u16,
    // pub message_contents: *mut OscType
}



#[ffi_type]
#[repr(C)]
pub struct OscData {
    pub message_count: u32,
    pub messages: *const FFIOscMessage,
}

/// Flatten a bundle down to a vec
pub fn flatten_bundle(b: OscBundle, messages: &mut Vec<OscMessage>) {
    for packet in b.content {
        match packet {
            OscPacket::Bundle(nested) => {
                flatten_bundle(nested, messages)
            },
            OscPacket::Message(m) => messages.push(m)
        };
    }
}


// pub fn convert_message(msg: &OscMessage) -> FFIOscMessage {
//     let x: &[u8] = &msg.addr.as_bytes()
//     
//     
// }
// 
// 
// pub extern "C" fn parse_message(message: &str) -> Result<*const OscData, u16> {
//     let res = match decode_udp(message.as_bytes()) {
//         Err(e) => return Err(1),
//         Ok((_, pkt)) => pkt
//     };
//     
//     let mut messages = vec![];
//     
//     match res {
//         OscPacket::Message(msg) => messages.push(msg),
//         OscPacket::Bundle(bun) => flatten_bundle(bun, &mut messages)
//     };
//     
//     let data_out = OscData {
//         message_count: messages.len() as u32,
//         messages: 
//     }
//     
//     
//     
//     
// }

// pub extern "C" fn to_

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let result = add(2, 2);
        assert_eq!(result, 4);
    }
}
