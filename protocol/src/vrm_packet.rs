use std::collections::{HashMap, HashSet};
use std::f32::consts::PI;
use rosc::{OscBundle, OscMessage, OscPacket, OscType};
use crate::vrm_packet::ObjectData::{Position, Rotation};


enum OutputMessageFormat {
    Raw,
    VRM
}

fn euler_angles_to_quaternion_degrees(roll: f32, pitch: f32, yaw: f32) -> (f32, f32, f32, f32) {
    let conversion_factor: f32 = 2.0 * PI / 360.0;
    euler_angles_to_quaternion(roll * conversion_factor, pitch * conversion_factor, yaw * conversion_factor)
}

/// Take euler angles in radians and convert them into a quaternion
fn euler_angles_to_quaternion(roll: f32, pitch: f32, yaw: f32) -> (f32, f32, f32, f32) {
    // https://en.wikipedia.org/wiki/Conversion_between_quaternions_and_Euler_angles
    let cr = (roll * 0.5).cos();
    let sr = (roll * 0.5).sin();
    let cp = (pitch * 0.5).cos();
    let sp = (pitch * 0.5).sin();
    let cy = (yaw * 0.5).cos();
    let sy = (yaw * 0.5).sin();

    let qw = cr * cp * cy + sr * sp * sy;
    let qx = sr * cp * cy - cr * sp * sy;
    let qy = cr * sp * cy + sr * cp * sy;
    let qz = cr * cp * sy - sr * sp * cy;

    (qw, qx, qy, qz)
}

// deconstruct slime vr message and convert it into a standardized format
fn slime_id_to_body_part<T>(message: &OscMessage, id_mapping: T) -> Result<SlimeVRMessageType, String>
    where T: Fn(u32) -> &'static str
{
    if message.addr.starts_with("/VMC") {
        return Err("Input appears to be in VRM format.".into())
    }
    if !message.addr.starts_with("/tracking/trackers") {
        return Err(format!("Input {} does not appear to be slimeVR mapping!", message.addr))
    }
    
    let mut msg_parts = message.addr.split("/");
    msg_parts.nth(2);  // consume the starting message
    
    let part = if let Some(part) = msg_parts.next() {
        if let Ok(id) = part.parse() {
            SlimeVRTarget::Tracker(id_mapping(id))
        }
        else {
            if part.to_ascii_lowercase() == "head" {
                SlimeVRTarget::Head
            }
            else {
                dbg!(part);
                dbg!(message);
                unimplemented!()
            }
        }
    }
    else {
        unimplemented!()
    };

    let msg_type = match (msg_parts.next(), message.args.as_slice()) {

        // positions are in -z
        (Some("position"), [OscType::Float(x), OscType::Float(y), OscType::Float(z)]) => Position(*x, *y, -*z),
        // already in quaternion form
        (Some("rotation"), [OscType::Float(x), OscType::Float(y), OscType::Float(z), OscType::Float(w)]) => Rotation(*x, *y, *z, *w),
        (Some("rotation"), [OscType::Float(x), OscType::Float(y), OscType::Float(z)]) => {
            // i hope you like gimbal lock
            // also slimevr sends them out in degrees
            let (qw, qx, qy, qz) = euler_angles_to_quaternion_degrees(*x, *y, *z);
            Rotation(-qx, -qy, qz, qw)
        }
        (Some(s), a) => panic!("Unknown message type: {s} with {a:?}"),
        _ => unimplemented!()
    };


    return Ok(SlimeVRMessageType {
        data_type: msg_type,
        source: part

    })


}

/// The base IDs that unity uses for objects.
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

impl SlimeVRTarget {
    fn to_string(&self) -> String {
        match self {
            SlimeVRTarget::Head => "Head".into(),
            SlimeVRTarget::Tracker(s) => s.to_string()
        }
    }
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


pub fn convert_to_vrm_base(b: &OscBundle) -> Result<OscBundle, String> {
    convert_to_vrm(b, &OutputMessageFormat::VRM, &base_vrm_id, &mut HashMap::new())
}

pub const FILTERED_VRM_BONES: [&str; 32] = [
    // we /really/ don't need all of these
    "LeftToes",
    "RightToes",
    "LeftThumbProximal",
    "LeftThumbIntermediate",
    "LeftThumbDistal",
    "LeftIndexProximal",
    "LeftIndexIntermediate",
    "LeftIndexDistal",
    "LeftMiddleProximal",
    "LeftMiddleIntermediate",
    "LeftMiddleDistal",
    "LeftRingProximal",
    "LeftRingIntermediate",
    "LeftRingDistal",
    "LeftLittleProximal",
    "LeftLittleIntermediate",
    "LeftLittleDistal",
    "RightThumbProximal",
    "RightThumbIntermediate",
    "RightThumbDistal",
    "RightIndexProximal",
    "RightIndexIntermediate",
    "RightIndexDistal",
    "RightMiddleProximal",
    "RightMiddleIntermediate",
    "RightMiddleDistal",
    "RightRingProximal",
    "RightRingIntermediate",
    "RightRingDistal",
    "RightLittleProximal",
    "RightLittleIntermediate",
    "RightLittleDistal",
];

/// Filter a vrm packet for data that we really don't want or care about.
pub fn filter_vrm(b: OscBundle, filtered_items: &HashSet<&str>) -> OscBundle {
    let mut messages = vec![];

    for pkt in b.content {
        match pkt {
            OscPacket::Bundle(b) => {
                messages.push(OscPacket::Bundle(filter_vrm(b, filtered_items)))
            },
            OscPacket::Message(m) => {
                if &m.addr != "/VMC/Ext/Bone/Pos" {
                    messages.push(OscPacket::Message(m));
                    continue;
                };
                match &m.args[0] {
                    OscType::String(str) if filtered_items.contains(str.as_str())  => {
                        continue
                    },
                    _ => messages.push(OscPacket::Message(m))
                }
            }
        };
    }

    OscBundle {
        timetag: b.timetag,
        content: messages
    }
}

pub fn convert_to_vrm_do_nothing(b: &OscBundle) -> OscBundle {
    b.clone()
}

/// Convert an osc bundle to the type that we expect, given the standard slimevr input.
fn convert_to_vrm<T>(b: &OscBundle, as_format: &OutputMessageFormat, converter: &T, hm: &mut HashMap<SlimeVRTarget, VRMBoneData>) -> Result<OscBundle, String>
    where T: Fn(u32) -> &'static str
{
    let mut messages = vec![];

    for pkt in &b.content {
        match pkt {
            OscPacket::Bundle(b) => {
                convert_to_vrm(b, as_format, converter, hm)?;
            }
            OscPacket::Message(m) => {
                let converted_message = slime_id_to_body_part(m, converter)?;
                let msg_addr = converted_message.get_vrm_addr("Bone");

                // let msg_kind = converted_message.sou
                let entry = hm.entry(converted_message.source).or_default();
                match converted_message.data_type {
                    Rotation(x,y,z, w) => entry.rotation = (x, y, z, w),
                    Position(x, y, z) => entry.position = (x, y, z)
                }
                entry.dest = msg_addr;
            }
        };
    };



    for (k, v) in hm.iter() {
        messages.push(OscPacket::Message(OscMessage{
            addr: v.dest.clone(),
            args: vec![
                OscType::String(k.to_string()),
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

    Ok(OscBundle {
        timetag: b.timetag,
        content: messages
    })


}