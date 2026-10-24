//! Sound effects API. Gameplay code calls these free functions from anywhere
//! (like the TS prototype's `sfx.*`); they queue commands that `AudioPlugin`
//! turns into sounds. All sounds are synthesised at startup (no asset files).
//!
//! NOTE: the synthesis + playback side is a stub until audio is ported; the
//! API is stable.
use bevy::prelude::*;
use std::sync::Mutex;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Snd {
    Thud,
    Clang,
    Grind,
    SpinnerHit,
    Whoosh,
    Pneumatic,
    Snap,
    Pop,
    Ding,
    RareDing,
    Buzzer,
    Bell,
    Bark,
    Alarm,
    Scrape,
    Explode,
    Click,
    Place,
    Cheer,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HumKind {
    Motor,
    Spinner,
}

#[derive(Clone, Debug)]
pub enum Cmd {
    /// one-shot: intensity 0..~1.5, pitch multiplier
    Play { snd: Snd, intensity: f32, pitch: f32 },
    HumStart { id: u32, kind: HumKind },
    /// level 0..1 (revs), vol 0..1
    HumSet { id: u32, level: f32, vol: f32 },
    HumStop { id: u32 },
    Crowd(bool),
    Music(bool),
}

static QUEUE: Mutex<Vec<Cmd>> = Mutex::new(Vec::new());
static NEXT_HUM: Mutex<u32> = Mutex::new(1);

fn push(c: Cmd) {
    if let Ok(mut q) = QUEUE.lock() {
        if q.len() < 256 {
            q.push(c);
        }
    }
}
/// Drain queued commands (used by the playback system).
pub fn drain() -> Vec<Cmd> {
    QUEUE.lock().map(|mut q| std::mem::take(&mut *q)).unwrap_or_default()
}

fn play(snd: Snd, intensity: f32, pitch: f32) {
    push(Cmd::Play { snd, intensity, pitch });
}

pub fn thud(i: f32) { play(Snd::Thud, i, 1.0) }
pub fn clang(i: f32) { play(Snd::Clang, i, 1.0) }
pub fn grind(i: f32) { play(Snd::Grind, i, 1.0) }
pub fn spinner_hit(i: f32) { play(Snd::SpinnerHit, i, 1.0) }
pub fn whoosh() { play(Snd::Whoosh, 1.0, 1.0) }
pub fn pneumatic() { play(Snd::Pneumatic, 1.0, 1.0) }
pub fn snap() { play(Snd::Snap, 1.0, 1.0) }
pub fn pop() { play(Snd::Pop, 1.0, 1.0) }
pub fn ding(pitch: f32) { play(Snd::Ding, 1.0, pitch) }
pub fn rare_ding() { play(Snd::RareDing, 1.0, 1.0) }
pub fn buzzer() { play(Snd::Buzzer, 1.0, 1.0) }
pub fn bell() { play(Snd::Bell, 1.0, 1.0) }
pub fn bark() { play(Snd::Bark, 1.0, 1.0) }
pub fn alarm() { play(Snd::Alarm, 1.0, 1.0) }
pub fn scrape() { play(Snd::Scrape, 1.0, 1.0) }
pub fn explode() { play(Snd::Explode, 1.0, 1.0) }
pub fn click() { play(Snd::Click, 1.0, 1.0) }
pub fn place() { play(Snd::Place, 1.0, 1.0) }
pub fn cheer(i: f32) { play(Snd::Cheer, i, 1.0) }
pub fn crowd(on: bool) { push(Cmd::Crowd(on)) }
pub fn music(on: bool) { push(Cmd::Music(on)) }

/// A continuous engine/spinner hum whose pitch follows `level`.
pub struct Hum {
    id: u32,
}
impl Hum {
    pub fn new(kind: HumKind) -> Self {
        let id = NEXT_HUM.lock().map(|mut n| {
            *n += 1;
            *n
        }).unwrap_or(0);
        push(Cmd::HumStart { id, kind });
        Hum { id }
    }
    pub fn set(&self, level: f32, vol: f32) {
        push(Cmd::HumSet { id: self.id, level, vol });
    }
    pub fn stop(&self) {
        push(Cmd::HumStop { id: self.id });
    }
}

pub struct AudioPlugin;
impl Plugin for AudioPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, |/* stub */| {
            drain();
        });
    }
}
