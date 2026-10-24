//! Sound effects API. Gameplay code calls these free functions from anywhere
//! (like the TS prototype's `sfx.*`); they queue commands that `AudioPlugin`
//! turns into sounds. All sounds are synthesised live with WebAudio (no asset
//! files), ported from the TS prototype's `audio.ts`.
//!
//! Playback only exists on wasm32. The AudioContext is created on the first
//! keyboard or mouse press (browser autoplay rules); commands queued before
//! that are dropped, except the crowd on/off state. On native targets the
//! queue is drained and ignored.
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
        app.add_systems(Update, audio_system);
    }
}

fn audio_system(
    keys: Option<Res<ButtonInput<KeyCode>>>,
    mouse: Option<Res<ButtonInput<MouseButton>>>,
) {
    let gesture = keys.is_some_and(|k| k.get_just_pressed().len() > 0)
        || mouse.is_some_and(|m| m.get_just_pressed().len() > 0);
    let cmds = drain();
    #[cfg(target_arch = "wasm32")]
    web::update(gesture, cmds);
    #[cfg(not(target_arch = "wasm32"))]
    let _ = (gesture, cmds);
}

#[cfg(target_arch = "wasm32")]
mod web {
    //! WebAudio synth. The nodes are !Send, so everything lives in a
    //! thread_local (wasm32 runs Bevy on the single main thread).
    use super::{Cmd, HumKind, Snd};
    use std::cell::RefCell;
    use std::collections::HashMap;
    use web_sys::{
        AudioBuffer, AudioBufferSourceNode, AudioContext, AudioScheduledSourceNode, AudioContextState, BiquadFilterNode,
        BiquadFilterType, GainNode, OscillatorNode, OscillatorType,
    };

    #[derive(Default)]
    struct State {
        synth: Option<Synth>,
        /// Last requested crowd state, applied when the context is created.
        crowd_wanted: bool,
    }

    thread_local! {
        static STATE: RefCell<State> = RefCell::new(State::default());
    }

    pub fn update(gesture: bool, cmds: Vec<Cmd>) {
        STATE.with_borrow_mut(|st| {
            if gesture {
                match &st.synth {
                    Some(s) => s.resume(),
                    None => {
                        st.synth = Synth::new();
                        if st.crowd_wanted {
                            if let Some(s) = &mut st.synth {
                                s.crowd(true);
                            }
                        }
                    }
                }
            }
            for c in cmds {
                if let Cmd::Crowd(on) = c {
                    st.crowd_wanted = on;
                }
                if let Some(s) = &mut st.synth {
                    s.handle(c);
                }
            }
        });
    }

    struct Crowd {
        src: AudioBufferSourceNode,
        filter: BiquadFilterNode,
        gain: GainNode,
    }

    struct HumNodes {
        kind: HumKind,
        o: OscillatorNode,
        o2: OscillatorNode,
        f: BiquadFilterNode,
        g: GainNode,
    }

    struct Synth {
        ctx: AudioContext,
        master: GainNode,
        noise_buf: AudioBuffer,
        crowd: Option<Crowd>,
        hums: HashMap<u32, HumNodes>,
        rng: u64,
    }

    use BiquadFilterType::{Bandpass, Highpass, Lowpass};
    use OscillatorType::{Sawtooth, Sine, Square, Triangle};

    impl Synth {
        fn new() -> Option<Self> {
            let ctx = AudioContext::new().ok()?;
            let master = ctx.create_gain().ok()?;
            master.gain().set_value(0.55);
            // Master bus limiter (same settings as the TS prototype).
            let comp = ctx.create_dynamics_compressor().ok()?;
            comp.threshold().set_value(-14.0);
            comp.ratio().set_value(6.0);
            comp.attack().set_value(0.003);
            comp.release().set_value(0.25);
            master.connect_with_audio_node(&comp).ok()?;
            comp.connect_with_audio_node(&ctx.destination()).ok()?;

            let mut rng = getrandom::u64().unwrap_or(0x9E37_79B9_7F4A_7C15) | 1;
            let rate = ctx.sample_rate();
            let len = (rate * 2.0) as u32;
            let noise_buf = ctx.create_buffer(1, len, rate).ok()?;
            let data: Vec<f32> = (0..len).map(|_| rand_f32(&mut rng) * 2.0 - 1.0).collect();
            noise_buf.copy_to_channel(&data, 0).ok()?;

            let s = Synth { ctx, master, noise_buf, crowd: None, hums: HashMap::new(), rng };
            s.resume();
            Some(s)
        }

        fn resume(&self) {
            if self.ctx.state() == AudioContextState::Suspended {
                let _ = self.ctx.resume();
            }
        }

        fn now(&self) -> f64 {
            self.ctx.current_time()
        }

        fn rand(&mut self) -> f32 {
            rand_f32(&mut self.rng)
        }

        fn handle(&mut self, c: Cmd) {
            match c {
                Cmd::Play { snd, intensity, pitch } => self.play(snd, intensity.max(0.0), pitch),
                Cmd::HumStart { id, kind } => {
                    if let Some(h) = self.hum_new(kind) {
                        if let Some(old) = self.hums.insert(id, h) {
                            self.hum_stop(old);
                        }
                    }
                }
                Cmd::HumSet { id, level, vol } => {
                    if let Some(h) = self.hums.get(&id) {
                        self.hum_set(h, level, vol);
                    }
                }
                Cmd::HumStop { id } => {
                    if let Some(h) = self.hums.remove(&id) {
                        self.hum_stop(h);
                    }
                }
                Cmd::Crowd(on) => self.crowd(on),
                // The TS prototype only stores a flag; there is no music synth.
                Cmd::Music(_) => {}
            }
        }

        /// Filtered noise burst with a linear attack and exponential decay;
        /// the filter optionally sweeps exponentially to `sweep_to`.
        #[allow(clippy::too_many_arguments)]
        fn noise(
            &mut self,
            dur: f64,
            freq: f32,
            q: f32,
            gain: f32,
            ty: BiquadFilterType,
            attack: f64,
            sweep_to: Option<f32>,
        ) -> Option<()> {
            let t = self.now();
            let src = self.ctx.create_buffer_source().ok()?;
            src.set_buffer(Some(&self.noise_buf));
            src.playback_rate().set_value(0.8 + self.rand() * 0.4);
            let f = self.ctx.create_biquad_filter().ok()?;
            f.set_type(ty);
            f.frequency().set_value_at_time(freq, t).ok()?;
            f.q().set_value(q);
            if let Some(to) = sweep_to {
                f.frequency().exponential_ramp_to_value_at_time(to, t + dur).ok()?;
            }
            let g = self.ctx.create_gain().ok()?;
            g.gain().set_value_at_time(0.0, t).ok()?;
            g.gain().linear_ramp_to_value_at_time(gain, t + attack).ok()?;
            g.gain().exponential_ramp_to_value_at_time(0.0001, t + dur).ok()?;
            src.connect_with_audio_node(&f).ok()?;
            f.connect_with_audio_node(&g).ok()?;
            g.connect_with_audio_node(&self.master).ok()?;
            let offset = self.rand() as f64;
            src.start_with_when_and_grain_offset(t, offset).ok()?;
            AsRef::<AudioScheduledSourceNode>::as_ref(&src).stop_with_when(t + dur + 0.05).ok()?;
            Some(())
        }

        /// Oscillator with a 5 ms attack and exponential decay; the pitch
        /// optionally slides exponentially to `slide_to`.
        fn tone(
            &self,
            freq: f32,
            dur: f64,
            gain: f32,
            ty: OscillatorType,
            slide_to: Option<f32>,
            delay: f64,
        ) -> Option<()> {
            let t0 = self.now() + delay;
            let o = self.ctx.create_oscillator().ok()?;
            o.set_type(ty);
            o.frequency().set_value_at_time(freq, t0).ok()?;
            if let Some(to) = slide_to {
                o.frequency().exponential_ramp_to_value_at_time(to, t0 + dur).ok()?;
            }
            let g = self.ctx.create_gain().ok()?;
            g.gain().set_value_at_time(0.0, t0).ok()?;
            g.gain().linear_ramp_to_value_at_time(gain, t0 + 0.005).ok()?;
            g.gain().exponential_ramp_to_value_at_time(0.0001, t0 + dur).ok()?;
            o.connect_with_audio_node(&g).ok()?;
            g.connect_with_audio_node(&self.master).ok()?;
            o.start_with_when(t0).ok()?;
            o.stop_with_when(t0 + dur + 0.05).ok()?;
            Some(())
        }

        fn play(&mut self, snd: Snd, i: f32, pitch: f32) {
            match snd {
                Snd::Thud => self.thud(i),
                Snd::Clang => self.clang(i),
                Snd::Grind => {
                    let f = 2500.0 + self.rand() * 1500.0;
                    self.noise(0.25, f, 4.0, 0.25 * i, Bandpass, 0.002, None);
                    let f = 900.0 + self.rand() * 400.0;
                    self.tone(f, 0.2, 0.05 * i, Sawtooth, Some(600.0), 0.0);
                }
                Snd::SpinnerHit => {
                    self.noise(0.5, 3000.0, 2.0, 0.6 * i, Bandpass, 0.001, Some(600.0));
                    self.tone(180.0, 0.5, 0.55 * i, Square, Some(40.0), 0.0);
                    self.clang(i);
                }
                Snd::Whoosh => {
                    self.noise(0.25, 600.0, 2.0, 0.25, Bandpass, 0.05, Some(2400.0));
                }
                Snd::Pneumatic => {
                    self.noise(0.35, 1200.0, 0.8, 0.5, Highpass, 0.002, Some(300.0));
                    self.tone(90.0, 0.2, 0.4, Sine, Some(50.0), 0.0);
                }
                Snd::Snap => {
                    self.noise(0.06, 2500.0, 1.0, 0.5, Highpass, 0.002, None);
                    self.tone(1400.0, 0.1, 0.12, Square, Some(400.0), 0.0);
                }
                Snd::Pop => {
                    self.tone(500.0, 0.08, 0.15, Sine, Some(900.0), 0.0);
                }
                Snd::Ding => {
                    self.tone(880.0 * pitch, 0.35, 0.18, Triangle, None, 0.0);
                    self.tone(1320.0 * pitch, 0.4, 0.1, Triangle, None, 0.07);
                }
                Snd::RareDing => {
                    for (k, f) in [660.0, 880.0, 1100.0, 1320.0].into_iter().enumerate() {
                        self.tone(f, 0.4, 0.13, Triangle, None, k as f64 * 0.08);
                    }
                }
                Snd::Buzzer => {
                    self.tone(140.0, 0.9, 0.25, Sawtooth, None, 0.0);
                    self.tone(147.0, 0.9, 0.25, Sawtooth, None, 0.0);
                }
                Snd::Bell => {
                    for k in 0..3 {
                        let d = k as f64 * 0.18;
                        self.tone(1200.0, 0.5, 0.15, Triangle, None, d);
                        self.tone(1800.0, 0.5, 0.07, Sine, None, d);
                    }
                }
                Snd::Bark => {
                    self.tone(420.0, 0.09, 0.3, Sawtooth, Some(220.0), 0.0);
                    self.noise(0.1, 900.0, 3.0, 0.3, Bandpass, 0.002, None);
                    self.tone(420.0, 0.09, 0.3, Sawtooth, Some(220.0), 0.16);
                }
                Snd::Alarm => {
                    self.tone(700.0, 0.18, 0.12, Square, Some(900.0), 0.0);
                    self.tone(700.0, 0.18, 0.12, Square, Some(900.0), 0.25);
                }
                Snd::Scrape => {
                    let f = 1500.0 + self.rand() * 800.0;
                    self.noise(0.18, f, 2.0, 0.12, Bandpass, 0.002, None);
                }
                Snd::Explode => {
                    self.noise(1.2, 900.0, 0.7, 0.9, Lowpass, 0.002, Some(80.0));
                    self.tone(70.0, 0.9, 0.7, Sine, Some(25.0), 0.0);
                    self.clang(1.0);
                }
                Snd::Click => {
                    self.tone(1500.0, 0.04, 0.08, Square, None, 0.0);
                }
                Snd::Place => {
                    self.tone(220.0, 0.08, 0.25, Square, Some(120.0), 0.0);
                    self.noise(0.06, 3000.0, 1.0, 0.15, Highpass, 0.002, None);
                }
                Snd::Cheer => self.cheer(i),
            }
        }

        /// Heavy body-to-body impact: falling sine + lowpassed noise, plus a
        /// clang on harder hits.
        fn thud(&mut self, i: f32) {
            let i = i.min(1.0);
            let f = 110.0 + self.rand() * 20.0;
            let d = i as f64;
            self.tone(f, 0.25 + d * 0.25, 0.5 * i + 0.15, Sine, Some(38.0), 0.0);
            self.noise(0.18 + d * 0.2, 500.0 + i * 900.0, 1.0, 0.35 * i + 0.1, Lowpass, 0.002, None);
            if i > 0.4 {
                self.clang(i * 0.6);
            }
        }

        /// Metallic clang: three inharmonic triangle partials + highpassed tick.
        fn clang(&mut self, i: f32) {
            let base = 300.0 + self.rand() * 500.0;
            for r in [1.0, 2.76, 5.4] {
                let dur = 0.2 + i as f64 * 0.4;
                self.tone(base * r, dur, 0.12 * i / r, Triangle, Some(base * r * 0.97), 0.0);
            }
            self.noise(0.08, 4000.0, 0.7, 0.25 * i, Highpass, 0.002, None);
        }

        fn crowd(&mut self, on: bool) {
            if on && self.crowd.is_none() {
                self.crowd = self.crowd_new();
            } else if !on {
                if let Some(c) = self.crowd.take() {
                    let t = self.now();
                    let g = c.gain.gain();
                    let _ = g.cancel_scheduled_values(t);
                    let _ = g.set_value_at_time(g.value(), t);
                    let _ = g.linear_ramp_to_value_at_time(0.0, t + 0.5);
                    let _ = AsRef::<AudioScheduledSourceNode>::as_ref(&c.src).stop_with_when(t + 0.6);
                }
            }
        }

        /// Looping bandpassed noise bed.
        fn crowd_new(&self) -> Option<Crowd> {
            let src = self.ctx.create_buffer_source().ok()?;
            src.set_buffer(Some(&self.noise_buf));
            src.set_loop(true);
            let filter = self.ctx.create_biquad_filter().ok()?;
            filter.set_type(Bandpass);
            filter.frequency().set_value(700.0);
            filter.q().set_value(0.6);
            let gain = self.ctx.create_gain().ok()?;
            gain.gain().set_value(0.05);
            src.connect_with_audio_node(&filter).ok()?;
            filter.connect_with_audio_node(&gain).ok()?;
            gain.connect_with_audio_node(&self.master).ok()?;
            src.start().ok()?;
            Some(Crowd { src, filter, gain })
        }

        /// Swell the crowd bed: louder and brighter, then settle back.
        fn cheer(&self, i: f32) {
            let Some(c) = &self.crowd else { return };
            let t = self.now();
            let d = i as f64;
            let g = c.gain.gain();
            let _ = g.cancel_scheduled_values(t);
            let _ = g.set_value_at_time(g.value(), t);
            let _ = g.linear_ramp_to_value_at_time(0.05 + 0.25 * i.min(1.0), t + 0.12);
            let _ = g.linear_ramp_to_value_at_time(0.05, t + 1.6 + d);
            let f = c.filter.frequency();
            let _ = f.cancel_scheduled_values(t);
            let _ = f.set_value_at_time(700.0, t);
            let _ = f.linear_ramp_to_value_at_time(1100.0, t + 0.15);
            let _ = f.linear_ramp_to_value_at_time(700.0, t + 1.5);
        }

        /// Sawtooth + square (an octave below) through a lowpass, silent
        /// until the first `HumSet`.
        fn hum_new(&self, kind: HumKind) -> Option<HumNodes> {
            let o = self.ctx.create_oscillator().ok()?;
            o.set_type(Sawtooth);
            let o2 = self.ctx.create_oscillator().ok()?;
            o2.set_type(Square);
            let f = self.ctx.create_biquad_filter().ok()?;
            f.set_type(Lowpass);
            f.frequency().set_value(600.0);
            let g = self.ctx.create_gain().ok()?;
            g.gain().set_value(0.0);
            o.connect_with_audio_node(&f).ok()?;
            o2.connect_with_audio_node(&f).ok()?;
            f.connect_with_audio_node(&g).ok()?;
            g.connect_with_audio_node(&self.master).ok()?;
            o.start().ok()?;
            o2.start().ok()?;
            Some(HumNodes { kind, o, o2, f, g })
        }

        fn hum_set(&self, h: &HumNodes, level: f32, vol: f32) {
            let t = self.now();
            let (f1, f2, cut, gain) = match h.kind {
                HumKind::Motor => (
                    40.0 + level * 90.0,
                    20.0 + level * 45.0,
                    250.0 + level * 500.0,
                    vol * (0.02 + level * 0.05),
                ),
                HumKind::Spinner => (
                    60.0 + level * 520.0,
                    30.0 + level * 260.0,
                    400.0 + level * 2400.0,
                    vol * level * 0.06,
                ),
            };
            let _ = h.o.frequency().set_target_at_time(f1, t, 0.05);
            let _ = h.o2.frequency().set_target_at_time(f2, t, 0.05);
            let _ = h.f.frequency().set_target_at_time(cut, t, 0.05);
            let _ = h.g.gain().set_target_at_time(gain, t, 0.05);
        }

        fn hum_stop(&self, h: HumNodes) {
            let t = self.now();
            let _ = h.g.gain().set_target_at_time(0.0, t, 0.05);
            let _ = h.o.stop_with_when(t + 0.3);
            let _ = h.o2.stop_with_when(t + 0.3);
        }
    }

    /// xorshift64*, returns 0..1.
    fn rand_f32(s: &mut u64) -> f32 {
        *s ^= *s >> 12;
        *s ^= *s << 25;
        *s ^= *s >> 27;
        (s.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 40) as f32 / (1u64 << 24) as f32
    }
}
