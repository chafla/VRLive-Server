use std::collections::HashMap;
use rosc::{OscBundle, OscMessage, OscPacket, OscType};
use crate::vrm_packet::ObjectData::{Position, Rotation};


enum OutputMessageFormat {
    Raw,
    Ligeia,
    VRM
}

// deconstruct slime vr message and convert it into a standardized format
fn slime_id_to_body_part<T>(message: &OscMessage, id_mapping: T) -> Result<SlimeVRMessageType, String>
    where T: Fn(u32) -> &'static str
{
    let mut msg_parts = message.addr.split("/");  // /tracking/trackers/...

    // msg_parts.s

    assert!(msg_parts.next() == Some("tracking"));
    assert!(msg_parts.next() == Some("trackers"));
    // todo clean this up
    let part = if let Some(part) = msg_parts.next() {
        if let Ok(id) = part.parse() {
            SlimeVRTarget::Tracker(id_mapping(id))
        }
        else {
            if part.to_ascii_lowercase() == "head" {
                SlimeVRTarget::Head
            }
            else {
                unimplemented!()
            }
        }
    }
    else {
        unimplemented!()
    };

    let msg_type = match (msg_parts.next(), message.args.as_slice()) {

        (Some("position"), [OscType::Float(x), OscType::Float(y), OscType::Float(z)]) => Position(*x, *y, *z),
        (Some("rotation"), [OscType::Float(x), OscType::Float(y), OscType::Float(z), OscType::Float(w)]) => Rotation(*x, *y, *z, *w),
        _ => unimplemented!()
    };


    return Ok(SlimeVRMessageType {
        data_type: msg_type,
        source: part

    })


}

fn base_vrm_id(id: u32) -> &'static str {
    match id {
        // these just need to match bones in unity
        1 => "Spine",
        2 => "LeftFoot",
        3 => "RightFoot",
        // these aren't actually in vrm but we'll include them anyway
        4 => "LeftUpperLeg",
        5 => "RightUpperLeg",
        6 => "UpperChest",
        7 => "LeftUpperArm",
        8 => "RightUpperArm",
        _ => unimplemented!()
    }
}

// match our testbed avatar
fn ligeia_vrm_id(id: u32) -> &'static str {
    match id {
        // these just need to match bones in unity
        1 => "Spine",
        2 => "Foot.L",
        3 => "Foot.R",
        // these aren't actually in vrm but we'll include them anyway
        4 => "UpperLeg.L",
        5 => "UpperLeg.R",
        6 => "Chest",
        7 => "UpperArm.L",
        8 => "UpperArm.R",
        _ => unimplemented!()
    }
}

struct VRMBoneData {
    dest: String,
    position: (f32, f32, f32),
    // quaternion
    rotation: (f32, f32, f32, f32)
}

impl Default for VRMBoneData {
    fn default() -> Self {
        Self {
            dest: "".into(),
            position: (0.0, 0.0, 0.0),
            rotation: (0.0, 0.0, 0.0, 0.0)
        }
    }
}

#[derive(Eq, PartialEq, Hash)]
enum SlimeVRTarget {
    Head,
    Tracker(&'static str)
}

#[derive(Copy, Clone)]
enum ObjectData {
    /// quaternion
    Rotation(f32, f32, f32, f32),
    /// vec3
    Position(f32, f32, f32)
}

struct SlimeVRMessageType {
    source: SlimeVRTarget,
    data_type: ObjectData,
}

impl SlimeVRMessageType {
    pub fn get_vrm_addr(&self, format: &str) -> String {
        // todo make this use enums instead
        format!("/VMC/Ext/{format}/Pos")
    }
}

/// Convert an incoming address to a counterpart
// fn convert_message(message: &str, target: &OutputMessageFormat) -> String {
//
//     let message_fn = match target {
//         OutputMessageFormat::Raw => return message.into(),
//         OutputMessageFormat::VRM => base_vrm_id,
//         OutputMessageFormat::Ligeia => ligeia_vrm_id,
//     };
//
//     let mut message_type;
//
//     // for message_parts
//
//     let body_part_identifier = slime_id_to_body_part(message, message_fn).unwrap();
//
//     return format!("/VMC/Ext/Tra/Pos")
// }

pub fn convert_to_vrm_ligeia(b: &OscBundle) -> OscBundle {
    convert_to_vrm(b, &OutputMessageFormat::Ligeia, &ligeia_vrm_id, &mut HashMap::new())
}

/// Convert an osc bundle to the type that we expect, given the standard slimevr input.
fn convert_to_vrm<T>(b: &OscBundle, as_format: &OutputMessageFormat, converter: &T, hm: &mut HashMap<SlimeVRTarget, VRMBoneData>) -> OscBundle
    where T: Fn(u32) -> &'static str
{
    let mut messages = vec![];

    for ref pkt in &b.content {
        match pkt {
            OscPacket::Bundle(b) => {
                convert_to_vrm(b, as_format, converter, hm);
            }
            OscPacket::Message(m) => {
                let converted_message = slime_id_to_body_part(m, converter).unwrap();
                let msg_addr = converted_message.get_vrm_addr("Bone");
                let entry = hm.entry(converted_message.source).or_default();
                match converted_message.data_type {
                    Rotation(x,y,z, w) => entry.rotation = (x, y, z, w),
                    Position(x, y, z) => entry.position = (x, y, z)
                }
                entry.dest = msg_addr;
                // if entry.dest.is_none() {
                //     entry.dest = converted_message.
                // }
            }
        };
    };



    for (v) in hm.values() {
        messages.push(OscPacket::Message(OscMessage{
            addr: v.dest.clone(),
            args: vec![
                v.position.0.into(),
                v.position.1.into(),
                v.position.2.into(),
                v.rotation.0.into(),
                v.rotation.1.into(),
                v.rotation.2.into(),
                v.rotation.3.into(),
            ]
        }))
    }

    OscBundle {
        timetag: b.timetag,
        content: messages
    }


}