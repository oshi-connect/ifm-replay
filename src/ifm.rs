//! iFacialMocap stream format: channel names, the v2 text packet, and clip-to-frame mapping.

use crate::anim::{BoneCurve, Clip, Rotation};
use crate::curve::Curve;

pub const DEFAULT_PORT: u16 = 49983;

/// The 52 channels iFacialMocap streams. Sided ARKit names use `_L` / `_R`.
pub const NAMES: [&str; 52] = [
    "eyeBlink_L", "eyeBlink_R", "eyeSquint_L", "eyeSquint_R", "eyeLookDown_L", "eyeLookDown_R",
    "eyeLookIn_L", "eyeLookIn_R", "eyeWide_L", "eyeWide_R", "eyeLookOut_L", "eyeLookOut_R",
    "eyeLookUp_L", "eyeLookUp_R", "browDown_L", "browDown_R", "browInnerUp", "browOuterUp_L",
    "browOuterUp_R", "jawOpen", "mouthClose", "jawLeft", "jawRight", "jawForward",
    "mouthUpperUp_L", "mouthUpperUp_R", "mouthLowerDown_L", "mouthLowerDown_R", "mouthRollUpper",
    "mouthRollLower", "mouthSmile_L", "mouthSmile_R", "mouthDimple_L", "mouthDimple_R",
    "mouthStretch_L", "mouthStretch_R", "mouthFrown_L", "mouthFrown_R", "mouthPress_L",
    "mouthPress_R", "mouthPucker", "mouthFunnel", "mouthLeft", "mouthRight", "mouthShrugLower",
    "mouthShrugUpper", "noseSneer_L", "noseSneer_R", "cheekPuff", "cheekSquint_L",
    "cheekSquint_R", "tongueOut",
];

/// Reads `host` or `host:port`; a bare IP gets the iFacialMocap port.
pub fn parse_target(text: &str) -> Option<std::net::SocketAddr> {
    let text = text.trim();
    text.parse()
        .ok()
        .or_else(|| text.parse::<std::net::IpAddr>().ok().map(|ip| (ip, DEFAULT_PORT).into()))
}

fn normalized(name: &str) -> String {
    name.chars().filter(char::is_ascii_alphanumeric).collect::<String>().to_ascii_lowercase()
}

/// The stream channel for a source blendshape name. Accepts `eyeBlinkLeft`, `eyeBlink_L`,
/// `EyeBlinkLeft` and DCC prefixes such as `blendShape1.eyeBlinkLeft`.
pub fn stream_name(source: &str) -> Option<&'static str> {
    let base = normalized(source.rsplit(['.', ':', '|']).next().unwrap_or(source));
    NAMES.into_iter().find(|name| {
        let arkit = name
            .strip_suffix("_L")
            .map(|b| format!("{b}Left"))
            .or_else(|| name.strip_suffix("_R").map(|b| format!("{b}Right")))
            .unwrap_or_else(|| name.to_string());
        base == normalized(name) || base == normalized(&arkit)
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bone {
    Head,
    LeftEye,
    RightEye,
}

/// Classifies a transform path by its last segment, for the common rig names.
pub fn bone_role(path: &str) -> Option<Bone> {
    let leaf = path.rsplit('/').next().unwrap_or(path).to_ascii_lowercase();
    let token = |t: &str| leaf.split(['_', '.', ' ', '-']).any(|p| p == t);
    if leaf.contains("eye") && !leaf.contains("lid") && !leaf.contains("brow") && !leaf.contains("highlight") {
        if leaf.contains("left") || token("l") {
            return Some(Bone::LeftEye);
        }
        if leaf.contains("right") || token("r") {
            return Some(Bone::RightEye);
        }
    }
    // An iFacialMocap FBX keeps the head rotation on its `face` root.
    (leaf == "head" || leaf == "face" || leaf.ends_with("_head")).then_some(Bone::Head)
}

/// Converts a Unity local rotation to the degrees iFacialMocap sends.
///
/// Warudo passes the streamed angles straight to Unity's `Quaternion.Euler`, so these are
/// Unity's own Euler angles (ZXY order). A recorded take already holds its rotations in that
/// space: a left eye that looks out has a negative Y, a left eye that looks up a negative X.
pub fn ifm_degrees([x, y, z, w]: [f64; 4]) -> [f64; 3] {
    let len = (x * x + y * y + z * z + w * w).sqrt();
    if len < 1e-9 {
        return [0.; 3];
    }
    let (x, y, z, w) = (x / len, y / len, z / len, w / len);
    let rx = (2. * (w * x - y * z)).clamp(-1., 1.).asin();
    let ry = (2. * (w * y + x * z)).atan2(1. - 2. * (x * x + y * y));
    let rz = (2. * (w * z + x * y)).atan2(1. - 2. * (x * x + z * z));
    [rx.to_degrees(), ry.to_degrees(), rz.to_degrees()]
}

/// Unity Euler curves use ZXY order; turn them into a quaternion first.
fn unity_euler_quat([x, y, z]: [f64; 3]) -> [f64; 4] {
    let (sx, cx) = (x.to_radians() / 2.).sin_cos();
    let (sy, cy) = (y.to_radians() / 2.).sin_cos();
    let (sz, cz) = (z.to_radians() / 2.).sin_cos();
    [
        cy * sx * cz + sy * cx * sz,
        sy * cx * cz - cy * sx * sz,
        cy * cx * sz - sy * sx * cz,
        cy * cx * cz + sy * sx * sz,
    ]
}

fn sample(rotation: &Rotation, time: f64) -> [f64; 3] {
    let quat = match rotation {
        Rotation::Quaternion(c) => std::array::from_fn(|i| c[i].evaluate(time)),
        Rotation::Euler(c) => unity_euler_quat(std::array::from_fn(|i| c[i].evaluate(time))),
    };
    ifm_degrees(quat)
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Frame {
    /// Channel name and 0–100 weight.
    pub weights: Vec<(&'static str, f64)>,
    pub head: [f64; 3],
    pub right_eye: [f64; 3],
    pub left_eye: [f64; 3],
}

/// One datagram in the original `name-value` layout. Warudo reads only this layout: it splits
/// each weight at `-` and ignores the `name&value` form of the v2 handshake.
pub fn packet(frame: &Frame) -> String {
    let mut out = String::with_capacity(1024);
    for (name, weight) in &frame.weights {
        out.push_str(&format!("{name}-{}|", weight.round().clamp(0., 100.) as i32));
    }
    // Adding zero turns -0.0 into 0.0, so a still axis prints as 0.0000.
    let angles = |[x, y, z]: [f64; 3]| format!("{:.4},{:.4},{:.4}", x + 0., y + 0., z + 0.);
    out.push_str(&format!("=head#{},0,0,0|", angles(frame.head)));
    out.push_str(&format!("rightEye#{}|", angles(frame.right_eye)));
    out.push_str(&format!("leftEye#{}|", angles(frame.left_eye)));
    out
}

/// A clip with its curves resolved to stream channels.
#[derive(Clone, Debug)]
pub struct Mapped {
    pub name: String,
    pub duration: f64,
    pub channels: Vec<(&'static str, Curve)>,
    pub unmatched: Vec<String>,
    pub head: Option<Rotation>,
    pub left_eye: Option<Rotation>,
    pub right_eye: Option<Rotation>,
}

impl Mapped {
    pub fn new(clip: Clip) -> Self {
        let mut channels: Vec<(&'static str, Curve)> = Vec::new();
        let mut unmatched = Vec::new();
        for (source, curve) in clip.blendshapes {
            match stream_name(&source) {
                Some(name) if !channels.iter().any(|(n, _)| *n == name) => channels.push((name, curve)),
                Some(_) => {}
                None => unmatched.push(source),
            }
        }
        let find = |role| {
            clip.bones
                .iter()
                .find(|b: &&BoneCurve| bone_role(&b.path) == Some(role))
                .map(|b| b.rotation.clone())
        };
        Self {
            name: clip.name,
            duration: clip.duration,
            head: find(Bone::Head),
            left_eye: find(Bone::LeftEye),
            right_eye: find(Bone::RightEye),
            channels,
            unmatched,
        }
    }

    /// All 52 channels at `time`; channels the clip lacks stay at zero.
    pub fn frame(&self, time: f64) -> Frame {
        let rotation = |r: &Option<Rotation>| r.as_ref().map_or([0.; 3], |r| sample(r, time));
        Frame {
            weights: NAMES
                .iter()
                .map(|name| {
                    let curve = self.channels.iter().find(|(n, _)| n == name);
                    (*name, curve.map_or(0., |(_, c)| c.evaluate(time)))
                })
                .collect(),
            head: rotation(&self.head),
            right_eye: rotation(&self.right_eye),
            left_eye: rotation(&self.left_eye),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_defaults_to_the_ifm_port() {
        assert_eq!(parse_target("127.0.0.1"), Some(([127, 0, 0, 1], 49983).into()));
        assert_eq!(parse_target(" 10.0.0.5:5000 "), Some(([10, 0, 0, 5], 5000).into()));
        assert_eq!(parse_target("127.0.0"), None);
    }

    #[test]
    fn maps_arkit_and_ifm_spellings() {
        assert_eq!(stream_name("eyeBlinkLeft"), Some("eyeBlink_L"));
        assert_eq!(stream_name("eyeBlink_L"), Some("eyeBlink_L"));
        assert_eq!(stream_name("EyeBlinkRight"), Some("eyeBlink_R"));
        assert_eq!(stream_name("blendShape1.mouthSmileLeft"), Some("mouthSmile_L"));
        assert_eq!(stream_name("jawOpen"), Some("jawOpen"));
    }

    #[test]
    fn directional_names_are_not_sides() {
        assert_eq!(stream_name("jawLeft"), Some("jawLeft"));
        assert_eq!(stream_name("mouthRight"), Some("mouthRight"));
    }

    #[test]
    fn unknown_names_do_not_match() {
        assert_eq!(stream_name("Fcl_MTH_A"), None);
        assert_eq!(stream_name("eyeBlink"), None);
    }

    #[test]
    fn classifies_common_eye_and_head_bones() {
        assert_eq!(bone_role("Armature/Hips/Spine/Neck/Head/LeftEye"), Some(Bone::LeftEye));
        assert_eq!(bone_role("Root/J_Adj_R_FaceEye"), Some(Bone::RightEye));
        assert_eq!(bone_role("rig/eye.L"), Some(Bone::LeftEye));
        assert_eq!(bone_role("Armature/Head"), Some(Bone::Head));
        assert_eq!(bone_role("Armature/Head/Eyelid_L"), None);
    }

    #[test]
    fn classifies_the_ifacialmocap_fbx_rig() {
        assert_eq!(bone_role("face"), Some(Bone::Head));
        assert_eq!(bone_role("face/eye_grp/eyeBall_L"), Some(Bone::LeftEye));
        assert_eq!(bone_role("face/eye_grp/eyeBall_R"), Some(Bone::RightEye));
    }

    #[test]
    fn identity_rotation_is_zero_degrees() {
        assert_eq!(ifm_degrees([0., 0., 0., 1.]), [0., 0., 0.]);
    }

    #[test]
    fn single_axis_rotations_keep_size_and_sign() {
        let half = 10f64.to_radians() / 2.;
        let [x, y, z] = ifm_degrees([half.sin(), 0., 0., half.cos()]);
        assert!((x - 10.).abs() < 1e-9 && y.abs() < 1e-9 && z.abs() < 1e-9);
        let [x, y, z] = ifm_degrees([0., -half.sin(), 0., half.cos()]);
        assert!(x.abs() < 1e-9 && (y + 10.).abs() < 1e-9 && z.abs() < 1e-9);
    }

    #[test]
    fn unity_euler_round_trips_through_a_quaternion() {
        let [x, y, z] = ifm_degrees(unity_euler_quat([-10., -18., 3.]));
        assert!((x + 10.).abs() < 1e-9 && (y + 18.).abs() < 1e-9 && (z - 3.).abs() < 1e-9);
    }

    #[test]
    fn packet_uses_the_name_dash_value_layout() {
        let frame = Frame {
            weights: vec![("jawOpen", 45.4), ("eyeBlink_L", 120.)],
            head: [1., 2., 3.],
            right_eye: [4., 5., 6.],
            left_eye: [7., 8., 9.],
        };
        assert_eq!(
            packet(&frame),
            "jawOpen-45|eyeBlink_L-100|=head#1.0000,2.0000,3.0000,0,0,0|\
             rightEye#4.0000,5.0000,6.0000|leftEye#7.0000,8.0000,9.0000|"
        );
    }
}
