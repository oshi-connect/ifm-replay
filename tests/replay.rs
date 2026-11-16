//! Parses a real-layout `.anim` and checks the datagrams that reach a UDP receiver.

use ifm_replay::{anim, ifm, player::Player};
use std::net::UdpSocket;
use std::time::Duration;

fn sample() -> ifm::Mapped {
    let text = std::fs::read_to_string("tests/fixtures/sample.anim").unwrap();
    ifm::Mapped::new(anim::parse(&text).unwrap())
}

fn weight(packet: &str, name: &str) -> Option<i32> {
    packet
        .split('|')
        .find_map(|part| part.strip_prefix(&format!("{name}-")))
        .and_then(|v| v.parse().ok())
}

#[test]
fn reads_clip_curves_and_maps_names() {
    let clip = sample();
    assert_eq!(clip.name, "Take 001");
    assert_eq!(clip.duration, 2.);
    let names: Vec<_> = clip.channels.iter().map(|(n, _)| *n).collect();
    assert_eq!(names, ["eyeBlink_L", "jawOpen"]);
    assert_eq!(clip.unmatched, ["Fcl_MTH_A"]);
    assert!(clip.left_eye.is_some() && clip.right_eye.is_none() && clip.head.is_none());
}

#[test]
fn frame_samples_float_and_rotation_curves() {
    let clip = sample();
    let start = clip.frame(0.5);
    let blink = start.weights.iter().find(|(n, _)| *n == "eyeBlink_L").unwrap().1;
    assert_eq!(blink, 0., "an infinite out slope holds until the next key");
    assert_eq!(start.weights.len(), 52);
    let end = clip.frame(2.);
    let jaw = end.weights.iter().find(|(n, _)| *n == "jawOpen").unwrap().1;
    assert!((jaw - 100.).abs() < 1e-9);
    assert!((end.left_eye[0] - 10.).abs() < 1e-3, "{:?}", end.left_eye);
}

#[test]
fn player_streams_frames_to_the_target() {
    let receiver = UdpSocket::bind(("127.0.0.1", 0)).unwrap();
    receiver.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    let player = Player::new().unwrap();
    player.set_target(Some(receiver.local_addr().unwrap()));
    player.load(sample());
    player.seek(1.5);
    player.toggle();

    let mut buffer = [0u8; 4096];
    let mut packets = Vec::new();
    while packets.len() < 10 {
        let size = receiver.recv(&mut buffer).expect("a datagram within 2 s");
        packets.push(String::from_utf8(buffer[..size].to_vec()).unwrap());
    }
    let first = &packets[0];
    assert_eq!(weight(first, "eyeBlink_L"), Some(100));
    assert!(weight(first, "jawOpen").unwrap() >= 70);
    assert!(first.contains("=head#") && first.ends_with('|'));

    // Without looping, playback stops at the end of the clip.
    std::thread::sleep(Duration::from_millis(700));
    let status = player.status();
    assert!(!status.playing);
    assert_eq!(status.position, 2.);
}
