//! Simple but scrappy opponent brain.
use crate::parts::PartKind;
use crate::physics::Phys;
use crate::rng;
use crate::robot::{strike_point, Input, Robot};
use bevy::prelude::*;

#[derive(Clone, Copy)]
pub struct Hazard {
    pub x: f32,
    pub z: f32,
    pub r: f32,
}

pub struct Brain {
    pub skill: f32,
    pub hazards: Vec<Hazard>,
    backoff: f32,
    unstick: f32,
    stuck_t: f32,
    last_pos: Vec3,
    wobble: f32,
    circle_dir: f32,
    fire_delay: f32,
    // where it last "looked" at the foe: low skill bots react late, so you can
    // get round their side
    seen: Vec3,
    seen_vel: Vec3,
    look_t: f32,
    // shark-circling breaker
    circle_t: f32,
    break_t: f32,
    /// debug: freeze the brain
    pub frozen: bool,
}

impl Brain {
    pub fn new(skill: f32, hazards: Vec<Hazard>) -> Self {
        Brain {
            skill,
            hazards,
            backoff: 0.0,
            unstick: 0.0,
            stuck_t: 0.0,
            last_pos: Vec3::ZERO,
            wobble: rng::rand() * 10.0,
            circle_dir: if rng::chance(0.5) { 1.0 } else { -1.0 },
            fire_delay: 0.0,
            seen: Vec3::ZERO,
            seen_vel: Vec3::ZERO,
            look_t: 0.0,
            circle_t: 0.0,
            break_t: 0.0,
            frozen: false,
        }
    }

    fn primary(me: &Robot) -> Option<usize> {
        let order = [PartKind::Spinner, PartKind::Flipper, PartKind::Hammer, PartKind::Spike, PartKind::Wedge];
        (0..me.parts.len())
            .filter(|&i| me.parts[i].attached && order.contains(&me.parts[i].kind))
            .min_by_key(|&i| order.iter().position(|k| *k == me.parts[i].kind).unwrap())
    }

    pub fn think(&mut self, me: &Robot, foe: &Robot, phys: &Phys, dt: f32, t: f32) -> Input {
        let mut out = Input::default();
        if !me.alive || self.frozen {
            return out;
        }
        let w = Self::primary(me);
        // aim the weapon's striking point (bot-local, relative to the COM) at the foe
        let mut aim_yaw = 0.0f32;
        if let Some(wi) = w {
            let p = &me.parts[wi];
            let sp = p.quat * strike_point(p.kind).unwrap() + p.pos - me.com;
            if sp.x.hypot(sp.z) > 0.1 {
                aim_yaw = sp.x.atan2(sp.z);
            }
        }
        let aim_dir = (me.q * Vec3::new(aim_yaw.sin(), 0.0, aim_yaw.cos())).with_y(0.0).normalize_or_zero();
        self.look_t -= dt;
        if self.look_t <= 0.0 {
            self.look_t = 0.06 + (1.0 - self.skill) * 0.38;
            self.seen = foe.pos;
            self.seen_vel = foe.linvel(phys).with_y(0.0);
        }
        let dist = foe.pos.distance(me.pos);
        // lead the target a little
        let target = self.seen + self.seen_vel * 0.25 * self.skill;
        let to_t = (target - me.pos).with_y(0.0).normalize_or_zero();
        let mut err = (aim_dir.x * to_t.z - aim_dir.z * to_t.x).atan2(aim_dir.dot(to_t));
        // sloppy aim at low skill
        err += (t * 1.7 + self.wobble).sin() * (1.0 - self.skill) * 0.45;
        out.turn = (err * 2.4).clamp(-1.0, 1.0);
        let facing = err.cos();
        let a = aim_yaw.sin().atan2(aim_yaw.cos());
        out.throttle = if facing > 0.6 { 1.0 } else if facing > 0.0 { 0.45 } else { -0.2 };
        if a.abs() > 2.0 {
            out.throttle = -out.throttle; // weapon at the back: reverse into them
        } else if a.abs() > 0.9 {
            out.throttle *= 0.3; // side weapon: circle rather than charge
        }
        let wk = w.map(|i| me.parts[i].kind);
        if wk == Some(PartKind::Spinner) {
            if me.parts[w.unwrap()].spin < 0.55 {
                self.backoff = self.backoff.max(0.5);
            }
            if self.backoff > 0.0 {
                self.backoff -= dt;
                out.throttle = if dist < 3.0 { -0.8 } else { 0.2 };
                out.turn += self.circle_dir * 0.5;
            }
        }
        if matches!(wk, Some(PartKind::Hammer) | Some(PartKind::Flipper)) {
            let wi = w.unwrap();
            let wp_ = &me.parts[wi];
            let reach = if wp_.kind == PartKind::Hammer { 1.25 } else { 1.1 };
            let wp = me.part_world(wp_, strike_point(wp_.kind).unwrap());
            let nearest = foe.parts.iter().filter(|p| p.attached).map(|p| foe.part_world(p, p.kind.def().bounds.c).distance(wp)).fold(f32::INFINITY, f32::min);
            if nearest < reach * 0.75 && err.abs() < 0.6 {
                self.fire_delay += dt;
                if self.fire_delay > 0.25 - self.skill * 0.2 {
                    out.fire = true;
                    self.fire_delay = 0.0;
                }
            }
            // hammer bots hover just outside reach then lunge
            if wp_.kind == PartKind::Hammer && dist < 1.4 && wp_.cooldown > 0.0 {
                out.throttle *= 0.3;
            }
        }
        // Circling each other with neither getting a line: break off and commit
        // to a run. Back out to open some room, then boost back in.
        if dist < 4.0 && err.abs() > 0.55 {
            self.circle_t += dt;
        } else {
            self.circle_t = (self.circle_t - dt * 0.5).max(0.0);
        }
        if self.circle_t > 2.2 - self.skill && self.break_t <= 0.0 && wk != Some(PartKind::Spinner) {
            self.break_t = 1.6;
            self.circle_t = 0.0;
        }
        if self.break_t > 0.0 {
            self.break_t -= dt;
            if self.break_t > 0.9 {
                out.throttle = -1.0;
                out.turn = -(if err == 0.0 { 1.0 } else { err.signum() }) * 0.6; // swing the nose round while reversing
            } else {
                out.throttle = 1.0;
                out.boost = err.abs() < 0.5;
            }
        }
        // a lined-up run: hit the gas
        if err.abs() < 0.18 && dist > 1.6 && dist < 5.0 && out.throttle > 0.5 && rng::rand() < dt * (0.4 + self.skill * 1.6) {
            out.boost = true;
        }
        if wk == Some(PartKind::Spinner) && me.parts[w.unwrap()].spin < 0.55 {
            out.boost = false;
        }
        // self-right
        if me.up.y < 0.0 && me.invert_time > 0.5 {
            out.fire = true;
        }
        // avoid hazards (pit) unless the foe is between us and it
        for h in &self.hazards {
            let hp = Vec3::new(h.x, 0.0, h.z);
            let dh = hp.distance(me.pos.with_y(0.0));
            if dh < h.r + 1.6 {
                let away = (me.pos - hp).with_y(0.0).normalize_or_zero();
                let f = me.fwd.with_y(0.0).normalize_or_zero();
                let side = f.x * away.z - f.z * away.x;
                out.turn = (out.turn + side.signum() * 1.2).clamp(-1.0, 1.0);
                if f.dot(away) < -0.3 {
                    out.throttle = -0.6;
                }
            }
        }
        // unstick
        let moved = me.pos.distance(self.last_pos);
        self.last_pos = me.pos;
        if out.throttle.abs() > 0.4 && moved < dt * 0.4 {
            self.stuck_t += dt;
        } else {
            self.stuck_t = (self.stuck_t - dt).max(0.0);
        }
        if self.stuck_t > 1.2 {
            self.unstick = 0.7;
            self.stuck_t = 0.0;
            self.circle_dir *= -1.0;
        }
        if self.unstick > 0.0 {
            self.unstick -= dt;
            out.throttle = -0.9;
            out.turn = self.circle_dir;
        }
        out
    }
}
