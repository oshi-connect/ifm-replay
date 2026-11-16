//! Playback on its own thread, so timing does not depend on the window redrawing.

use crate::ifm::{packet, Mapped};
use std::net::{SocketAddr, UdpSocket};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

const FRAME: Duration = Duration::from_micros(16_667);

#[derive(Default)]
struct State {
    clip: Option<Arc<Mapped>>,
    target: Option<SocketAddr>,
    looping: bool,
    /// Clip time at `anchor`; playing while `anchor` is set.
    position: f64,
    anchor: Option<Instant>,
    sent: u64,
    error: Option<String>,
    stopped: bool,
}

impl State {
    fn now(&self) -> f64 {
        self.position + self.anchor.map_or(0., |a| a.elapsed().as_secs_f64())
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Status {
    pub position: f64,
    pub playing: bool,
    pub looping: bool,
    pub sent: u64,
    pub error: Option<String>,
}

pub struct Player {
    state: Arc<Mutex<State>>,
}

impl Player {
    pub fn new() -> std::io::Result<Self> {
        // An ephemeral port, so this can run on the same PC as the receiver that owns 49983.
        let socket = UdpSocket::bind(("0.0.0.0", 0))?;
        let state = Arc::new(Mutex::new(State::default()));
        let shared = state.clone();
        thread::Builder::new()
            .name("ifm-sender".into())
            .spawn(move || run(socket, shared))?;
        Ok(Self { state })
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn load(&self, clip: Mapped) {
        let mut s = self.lock();
        s.clip = Some(Arc::new(clip));
        s.position = 0.;
        s.anchor = None;
        s.error = None;
    }

    pub fn set_target(&self, target: Option<SocketAddr>) {
        self.lock().target = target;
    }

    pub fn set_looping(&self, looping: bool) {
        self.lock().looping = looping;
    }

    pub fn toggle(&self) {
        let mut s = self.lock();
        if s.anchor.is_some() {
            s.position = s.now();
            s.anchor = None;
            return;
        }
        let duration = s.clip.as_ref().map_or(0., |c| c.duration);
        if s.position >= duration {
            s.position = 0.;
        }
        if s.clip.is_some() {
            s.anchor = Some(Instant::now());
        }
    }

    pub fn seek(&self, time: f64) {
        let mut s = self.lock();
        let duration = s.clip.as_ref().map_or(0., |c| c.duration);
        s.position = time.clamp(0., duration);
        if s.anchor.is_some() {
            s.anchor = Some(Instant::now());
        }
    }

    pub fn status(&self) -> Status {
        let s = self.lock();
        Status {
            position: s.now(),
            playing: s.anchor.is_some(),
            looping: s.looping,
            sent: s.sent,
            error: s.error.clone(),
        }
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        self.lock().stopped = true;
    }
}

fn run(socket: UdpSocket, state: Arc<Mutex<State>>) {
    let mut next = Instant::now();
    loop {
        let datagram = {
            let mut s = state.lock().unwrap_or_else(|e| e.into_inner());
            if s.stopped {
                return;
            }
            tick(&mut s)
        };
        if let Some((target, bytes)) = datagram {
            let result = socket.send_to(bytes.as_bytes(), target);
            let mut s = state.lock().unwrap_or_else(|e| e.into_inner());
            match result {
                Ok(_) => {
                    s.sent += 1;
                    s.error = None;
                }
                Err(e) => s.error = Some(e.to_string()),
            }
        }
        next += FRAME;
        let now = Instant::now();
        if next > now {
            thread::sleep(next - now);
        } else {
            next = now;
        }
    }
}

/// Advances the clock and returns the datagram to send this tick, if any.
///
/// A paused clip keeps sending its current frame: Warudo treats 0.5 s without a datagram as
/// lost tracking and drops the face back to idle.
fn tick(s: &mut State) -> Option<(SocketAddr, String)> {
    let clip = s.clip.clone()?;
    let mut time = s.now();
    if s.anchor.is_some() && time >= clip.duration {
        if s.looping && clip.duration > 0. {
            time %= clip.duration;
            s.position = time;
            s.anchor = Some(Instant::now());
        } else {
            time = clip.duration;
            s.position = time;
            s.anchor = None;
        }
    }
    Some((s.target?, packet(&clip.frame(time))))
}
