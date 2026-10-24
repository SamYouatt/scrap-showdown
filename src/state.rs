//! The current run: your bot, your stash, the tournament ladder.
use crate::parts::{AddOpts, Builder, Design, Face, PartKind, Res};
use crate::rng;
use bevy::prelude::*;

pub(super) fn plugin(app: &mut App) {
    app.init_resource::<Run>();
}

#[derive(Clone, Debug, Default)]
pub struct BattleResult {
    pub won: bool,
    pub how: String,
    pub time_left: f32,
    pub dmg_dealt: f32,
    pub dmg_taken: f32,
    /// part ids knocked off (incl. everything hanging off them)
    pub lost: Vec<u32>,
    /// (id, remaining hp fraction)
    pub damaged: Vec<(u32, f32)>,
    pub trophy: Option<PartKind>,
    pub opponent: String,
}

/// One physical part in the bin. dmg is 0..1 like Placed.dmg.
#[derive(Clone, Debug)]
pub struct InvItem {
    pub uid: u32,
    pub kind: PartKind,
    pub dmg: f32,
}

/// Everything that carries over between screens during one run.
#[derive(Resource, Clone, Debug)]
pub struct Run {
    pub design: Design,
    pub inv: Vec<InvItem>,
    pub next_uid: u32,
    pub plate: i32,
    pub bolts: i32,
    pub round: usize,
    pub results: Vec<bool>,
    pub lives: i32,
    pub scrap_time: f32,
    /// last fight's result (for the results screen)
    pub last: Option<BattleResult>,
    /// test drive against the dummy from the workshop
    pub testing: bool,
}

pub struct Opponent {
    pub design: Design,
    pub blurb: &'static str,
    /// 0..1
    pub skill: f32,
}

use Face::*;
use PartKind::*;

/// Core + a block front and back + four wheels: the classic box-on-wheels.
fn chassis(name: &str, paint: &str, wheel: PartKind) -> (Builder, u32, u32) {
    let mut b = Builder::new(name, paint);
    let f = b.add(0, PZ, Block);
    let r = b.add(0, NZ, Block);
    b.add_m(f, PX, wheel);
    b.add_m(r, PX, wheel);
    (b, f, r)
}

/// Core + a block front and back + four legs: a scrap-heap dog.
fn critter(name: &str, paint: &str) -> (Builder, u32, u32) {
    let mut b = Builder::new(name, paint);
    let f = b.add(0, PZ, Block);
    let r = b.add(0, NZ, Block);
    b.add_m(f, PX, Leg);
    b.add_m(r, PX, Leg);
    (b, f, r)
}

fn v(v: f32) -> AddOpts {
    AddOpts { v, ..default() }
}

fn dave() -> Design {
    let (mut b, f, _) = critter("Dustbin Dave", "#6fae4a");
    b.add(f, PZ, Armour);
    b.add(0, PY, Armour);
    b.d
}

fn larry() -> Design {
    let (mut b, f, r) = chassis("Lawnmower Larry", "#e8c13a", Wheel);
    let rail = b.add_o(f, PZ, Slab, v(-0.1));
    b.add_o(rail, PY, Spinner, v(0.18));
    b.add(r, PY, Battery);
    b.d
}

fn thumper() -> Design {
    // six-legged tortoise: tripod gait, very planted
    let (mut b, f, r) = critter("Thumper", "#5b8dd6");
    b.add_m(0, PX, Leg);
    b.add(f, PY, Hammer);
    b.add(r, PZ, Armour);
    b.add(f, PZ, Wedge);
    b.d
}

fn flipzilla() -> Design {
    let (mut b, f, r) = chassis("Flipzilla", "#d8453a", BigWheel);
    b.add_o(f, PY, Flipper, v(0.02));
    b.add_m(0, PX, Armour);
    b.add(r, PY, Battery);
    b.add(r, PZ, Armour);
    b.d
}

fn granny() -> Design {
    let (mut b, f, r) = critter("Granny Stomps", "#b57bd6");
    let rail = b.add_o(f, PZ, Slab, v(-0.08));
    b.add_o(rail, PY, Hammer, v(0.2));
    b.add(rail, PZ, Spike);
    b.add(r, PY, Battery);
    b.add(r, PZ, Armour);
    b.add(0, PY, Battery);
    b.d
}

pub fn opponents() -> Vec<Opponent> {
    vec![
        Opponent { design: dave(), blurb: "A wheelie bin that grew legs. Just wants to push you around.", skill: 0.3 },
        Opponent { design: larry(), blurb: "Built from an actual ride-on mower. That blade is spinning. Keep your legs away.", skill: 0.5 },
        Opponent { design: thumper(), blurb: "Armoured tortoise with a pickaxe. Slow, heavy, patient.", skill: 0.65 },
        Opponent { design: flipzilla(), blurb: "Crowd favourite. Will throw you into the pit and wave to the crowd.", skill: 0.8 },
        Opponent { design: granny(), blurb: "Reigning champion. Four legs, a hammer on a boom, and absolutely no mercy.", skill: 0.95 },
    ]
}

pub fn dummy() -> Design {
    let (mut b, _, _) = chassis("Practice Dummy", "#d9c9a3", Wheel);
    b.add_m(0, PX, Armour);
    b.add(0, PY, Armour);
    b.d
}

pub fn starter_design() -> Design {
    let (mut b, f, r) = critter("Your Bot", "#e2483d");
    b.add(f, PZ, Spike);
    b.add(r, PY, Battery);
    b.d
}

impl Default for Run {
    fn default() -> Self {
        Run::new_game()
    }
}

impl Run {
    pub fn new_game() -> Self {
        let mut s = Run {
            design: starter_design(),
            inv: vec![],
            next_uid: 1,
            plate: 4,
            bolts: 3,
            round: 0,
            results: vec![],
            lives: 2,
            scrap_time: 0.0,
            last: None,
            testing: false,
        };
        for (k, n) in [(Block, 2), (Slab, 2), (Girder, 2), (Armour, 2), (Leg, 2), (Wedge, 1), (Hammer, 1)] {
            for _ in 0..n {
                s.add_item(k, 0.0);
            }
        }
        s
    }

    /// Put a part in the bin.
    pub fn add_item(&mut self, kind: PartKind, dmg: f32) -> u32 {
        let uid = self.next_uid;
        self.next_uid += 1;
        self.inv.push(InvItem { uid, kind, dmg });
        uid
    }

    pub fn count_of(&self, kind: PartKind) -> usize {
        self.inv.iter().filter(|x| x.kind == kind).count()
    }

    /// Returns the parts salvaged from the arena floor.
    pub fn apply_battle(&mut self, r: &BattleResult) -> Vec<PartKind> {
        let mut salvaged = vec![];
        let mut d = self.design.clone();
        for (id, frac) in &r.damaged {
            if let Some(c) = d.by_id_mut(*id) {
                c.dmg = (1.0 - frac).min(0.95);
            }
        }
        let mut lost_parts: Vec<_> = d.parts.iter().filter(|c| r.lost.contains(&c.id) && c.kind != Core).cloned().collect();
        d.parts.retain(|c| !r.lost.contains(&c.id) || c.kind == Core);
        // anything whose parent went missing goes too
        loop {
            let ids: Vec<u32> = d.parts.iter().map(|c| c.id).collect();
            let orphans: Vec<_> = d.parts.iter().filter(|c| c.parent.is_some_and(|p| !ids.contains(&p))).cloned().collect();
            if orphans.is_empty() {
                break;
            }
            lost_parts.extend(orphans);
            d.parts.retain(|c| c.parent.is_none_or(|p| ids.contains(&p)));
        }
        let ids: Vec<u32> = d.parts.iter().map(|c| c.id).collect();
        for c in d.parts.iter_mut() {
            if c.mirror.is_some_and(|m| !ids.contains(&m)) {
                c.mirror = None;
            }
        }
        if let Some(core) = d.parts.iter_mut().find(|c| c.kind == Core) {
            core.dmg = core.dmg.min(0.9);
        }
        if r.won {
            // winner gets to sweep the arena floor, but some bits are scrap
            for c in lost_parts {
                if rng::chance(0.5) {
                    continue;
                }
                self.add_item(c.kind, c.dmg);
                salvaged.push(c.kind);
            }
            if let Some(t) = r.trophy {
                self.add_item(t, 0.0);
            }
        }
        self.design = d;
        salvaged
    }
}

/// Repair cost for one part given its damage.
pub fn repair_cost(kind: PartKind, dmg: f32) -> (Res, i32) {
    let d = kind.def();
    if dmg <= 0.001 {
        return (d.repair, 0);
    }
    (d.repair, ((dmg * (d.hp / 18.0)).ceil() as i32).max(1))
}

pub const FAB_COST: [(PartKind, i32); 4] = [(Block, 4), (Slab, 3), (Girder, 2), (Armour, 7)];

pub fn scrap_time_for(r: &BattleResult) -> (f32, Vec<(String, f32)>) {
    let mut lines: Vec<(String, f32)> = vec![("Base scavenging time".into(), 25.0)];
    if r.won {
        lines.push(("Victory".into(), 25.0));
    }
    if r.won && r.time_left > 0.0 {
        lines.push(("Quick finish".into(), (r.time_left / 120.0 * 15.0).round()));
    }
    lines.push(("Damage dealt".into(), (r.dmg_dealt / 25.0).round().min(12.0)));
    lines.push(("Damage avoided".into(), (10.0 - (r.dmg_taken / 30.0).round()).max(0.0)));
    let total = lines.iter().map(|l| l.1).sum();
    (total, lines)
}
