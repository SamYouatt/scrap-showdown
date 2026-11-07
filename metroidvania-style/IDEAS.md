# Metroidvania-style: ideas

This folder started as a copy of Scrap Crawlers at `2e3bb4a` (workshop, arena,
and explore mode with wild encounters). This file collects ideas for turning it
into a metroidvania, where unlocking robot parts lets you explore more of the
world. `DESIGN.md` is the original Scrap Crawlers design doc.

## The rule: everything is physics

No gate checks which part you own. If you can't climb something, it's because
the geometry really is too hard for your legs. If a rock moves, it's because the
plow physically pushed it. Many obstacles have several physical answers: a small
lava pool might be jumped with jumping legs, stepped over with long legs, or
filled with a rock you pushed in. A wide one might need lava-proof feet.

### What has to change in the code

The current walker is scripted, not simulated:

- `creature.ts` sets the body height from the average height of the feet, and
  feet snap to the top of whatever is under them, so even a 3 m crate gets a
  foothold.
- The only thing that stops a climb is a hard rule in `Creature.move`:
  `if (rise > legLength * 0.75) vel = 0`. That's a fake gate, so it goes.
- In fights (`battle.ts`), hulls are boxes pushed apart by shoves that fade out.
  Nothing has mass, gravity, friction or tipping.
- Crates, drums and tyres are part of the ground's height (`World.heightAt`), so
  they can't move.

Proposal: keep the procedural legs, but make the world they stand in physical.

| Thing | Becomes |
| --- | --- |
| Hull | A rigid body with mass and gravity, held up only by legs planted on something. |
| Legs | Still procedural IK, but a foothold must be within reach of the hip, on a surface that isn't too steep, and something a foot can stand on. Planted legs push the hull up like springs and drive it forward through friction. |
| Rocks, drums, crates, planks | Rigid bodies with mass and friction. They can be pushed, tipped and dragged. |
| Plow | A wedge-shaped collider on the hull. Its lip gets under things and tips them. How hard it pushes is limited by the grip of your planted feet. |
| Pincer | Grabbing makes a joint to the thing it holds. Pulling is limited by grip, like the plow. |
| Terrain | Real collision shapes: ledges, gaps and slopes. Each surface has a material (dirt, scree, oil, ice, steel, lava) that sets friction, heat and current. |

What falls out of this without any rules:

- **Ledges.** Climbing needs the front feet to reach the top *and* the hull to
  clear the lip. A low millipede gets its feet up but scrapes its hull and stalls.
- **Tipping.** If the centre of mass moves outside the planted feet, you fall
  over. The strider's tippiness becomes real.
- **Gaps.** A long stride steps across a narrow gap. A millipede can bridge one
  with feet planted on both sides.
- **Slopes.** Feet slip when the slope beats their friction.
- **Pushing.** How hard you push equals how hard your planted feet grip. A heavy,
  wide quad shoves a boulder that a light strider can't, even with the same plow.

Engine: Rapier (`@dimforge/rapier3d-compat`, 0.21.0 at the time of writing,
maintained, runs in the browser through WebAssembly, about 1–2 MB) rather than
writing our own. Leg footholds and hull clearance alone could be hand-written,
so Rapier could wait until the first movable objects.

## Parts as physical properties

Parts open areas only by changing the crawler's physical properties: reach, hull
clearance, stance width, number of feet, grip, mass, centre of mass, and what the
feet and hull are made of. Today's leg kits, from `parts.ts`:

| Leg kit | Reach (leg length) | Hip clearance (ride height × length) | Feet | What that should physically allow |
| --- | --- | --- | --- | --- |
| Millipede | 0.80 | 0.28 | 8, grippy | Steep and loose slopes, low tunnels, bridging gaps with body length. Stalls on tall lips |
| Crab | 1.10 | 0.50 | 6 | The all-rounder |
| Girder quad | 1.35 | 0.54 | 4, heavy | Pushing, holding against force, staying put in wind. Slow to turn |
| Piston strider | 1.50 | 0.83 | 2 | Tall ledges, long strides over gaps. Tips easily, little grip for pushing |

New part ideas the barriers below refer to:

- **Legs and feet:** jumping legs (crouch, store energy, launch), spike feet
  (grip on steep and icy ground), wide pad feet (don't sink), hook feet (catch
  on rebar and grating), magnet feet (stick to steel), lava-proof ceramic feet,
  rubber insulated feet, shock-absorber legs (survive long drops), tracks or
  wheels (fast on flat ground, bad on rough).
- **Hulls:** sealed drum hull (floats and rolls), pod hull (low and rounded),
  armour plating (heavier, takes falling junk).
- **Tools and top mounts:** winch or grapple (rope joint to an anchor), magnet
  (pulls loose metal), heat sinks (slow overheating), gyro stabiliser (resists
  tipping).

## Progression barriers

Each barrier physically stops some builds. In the answers column, parts come
first and, after the semicolon, tricks that use the world instead of a part.

### Height and reach

| Barrier | What physically stops you | Answers |
| --- | --- | --- |
| Tall ledge | Front feet can't reach the top, or the hull catches on the lip | Long legs, jumping legs, winch; push a crate against it as a step |
| Cliff with rebar or grating | Feet slide off a vertical face with no flat foothold | Hook feet, pincer climbing hand over hand, winch |
| Steel wall or ceiling | Vertical, nothing to stand on | Magnet feet walk up it |
| Shaft or long drop | Landing impact damages legs, and you can't climb back out | Shock-absorber legs, winch to lower yourself; otherwise it's a one-way shortcut |
| Switch or lever up high | Out of reach of your head or tools | Long legs, pincer on a top mount; knock it with a pushed object |
| Tiered terraces (car stacks) | Several ledges in a row, each needing a fresh foothold | Long legs or jumps; push crates up tier by tier |

### Gaps and footing

| Barrier | What physically stops you | Answers |
| --- | --- | --- |
| Narrow chasm | No foothold in the middle | Long stride, jumping legs, millipede spanning it with its body |
| Wide chasm | Too far to step or span | Jumping legs on a light build, tracks or wheels off a ramp at speed, grapple swing; drag a girder across as a bridge |
| Stepping-stone pillars | Footholds are sparse. Many-legged builds can't plant enough feet, and low builds fall between | Few long legs (strider), jumping legs |
| Single beam or pipe | A wide stance puts feet off the edge | Narrow-stance legs, millipede gripping both sides of a round pipe, gyro stabiliser |
| Rotten floor | Breaks under too much weight | Light builds cross it. A heavy build crashes through, which can be the way *down* |
| Swinging or moving platform | Lurches throw you off | Grippy or magnet feet, low centre of mass; timing |
| Narrow corridor with a tight turn | A long hull or wide legs jam and can't turn round | Short hull, narrow stance, millipede that bends along its length |
| Low tunnel or culvert | Hull hits the ceiling | Low ride height, legs that splay flat |
| Snag wires at shin height | They catch on a low hull | High hip clearance steps over; cut them with a pincer |

### Slopes and surfaces

| Barrier | What physically stops you | Answers |
| --- | --- | --- |
| Steep slope | Feet slip when the slope beats their friction | Spike feet, many legs, low centre of mass |
| Scree | The surface slides away under your feet | Many legs or wide pads spreading the load; plow a flat path |
| Ice or oil slick | Almost no grip | Spike feet. Or use it: a run-up slide carries you across a gap |
| Mud or sludge | You sink until the hull drags | Wide pad feet, many legs, long wading legs |
| Conveyor running against you | It carries you back faster than you walk | Enough speed and grip, or enough mass; ride it the other way |
| Deep water or oil lake | The hull sinks | Sealed drum hull floats; long legs wade the shallows; push floating drums together as a raft |

### Heat, current and corrosion

| Barrier | What physically stops you | Answers |
| --- | --- | --- |
| Small lava pool | Feet burn, and the hull heats when it's close to the surface | Jumping legs, a strider stepping over; plow a rock in and step on it |
| Lava lake | Too wide to step or jump | Lava-proof feet with legs tall enough to keep the hull cool; plowed stepping stones; drag a girder across |
| Furnace air | Hull heat builds over time | Heat sinks, speed to dash through; push a metal sheet along as a shield |
| Electrified floor | Current runs through steel feet | Rubber insulated feet, jumping across; drop a scrap bar to short it out |
| Acid puddles | Each step corrodes your feet | Coated feet; plow sand over the puddle |
| Crane magnet field | Steel parts get pulled and stuck | Non-steel parts, heavy legs that resist. Or use it to lift yourself up |

### Forces

| Barrier | What physically stops you | Answers |
| --- | --- | --- |
| Fans or a wind tunnel | Light, tall builds get blown back | Mass and grip (quad), a low profile (millipede) |
| Steam vent or geyser | It launches whatever stands on it | Use it: a light build rides the jet up to a ledge. A heavy build or a pushed crate caps it |
| Piston press | It crushes whatever is under it | Timing, armour to survive one hit; jam it with a crate |
| Falling junk | Things land on you | Armour plating; wait for the cascade |

### Objects

| Barrier | What physically stops you | Answers |
| --- | --- | --- |
| Boulder in a gap | Too heavy to move without grip behind the push | Plow with heavy grippy legs; drag it out with the pincer; get a wedge lip under one edge and roll it |
| Stacked crate wall | Solid until it topples | Ram it with momentum; pull the bottom crate with the pincer |
| Rusted gate | Needs a steady pull | Pincer pulling with grip, winch |
| Drawbridge with a counterweight | Needs weight on the other end | Push drums onto the counterweight; a heavy build stands on it |
| Pressure plate | Needs weight to stay down | A heavy build, or something pushed onto it |
| Valve wheel or crank | Needs grip and turning force | Pincer, magnet |
| Fuse socket powering a lift | Needs something carried to it | Pincer or magnet carrying the fuse |
| Hanging chain | Too high to climb or pull | Pincer to climb or pull it and drop a bridge |

### Perception

Dark zones and smoke can stay physical if feet can only target surfaces the
crawler can *see*. In the dark, legs creep and probe for footholds, so you can
still cross slowly. A lamp-eye's light cone lets you walk normally. Smoke works
the same way, with a telly judging depth through it.

### Patterns worth keeping

- **Two-way barriers.** Heavy breaks rotten floors and holds plates down; light
  crosses floors and rides geysers. Neither build is simply better.
- **Barriers you can use.** Ice, geysers, magnet fields and conveyors block one
  build and carry another.
- **One-way shortcuts.** Drops you can go down but not climb back up make loops
  back to a bench.
- **Visible before reachable.** Every barrier shows what it physically wants (a
  lip height, a gap width, a glow of heat), so players can work out a build
  before they own the part.

## How traversal parts change combat

Today a fight (`battle.ts`) picks one plan from the build: **ram** with a wedge,
**grab** with a powered pincer, or **bump**. Ram damage grows with closing
speed. A grab holds, squeezes and flings. You win by knockout or by ring-out. On
the same physics as the world, fights get real tipping, falls and grip, and a
third way to lose: **flipped**, on your back and unable to right yourself.

The rule for every part: its traversal strength is also a fight strength, and it
comes with a fight weakness.

| Part | In the world | Fight upside | Fight downside |
| --- | --- | --- | --- |
| Piston strider | Tall ledges, long strides | Fastest charge, so the hardest rams. Steps over a low wedge | High centre of mass on two feet. A wedge lip tips it, and it's easy to push off the edge |
| Millipede | Slopes, low tunnels, gaps | Hard to tip, grips against shoves. Gets under a tall foe and lifts it | Short reach, slow hits. Stalls on raised arena tiers |
| Girder quad | Pushes boulders, holds plates, breaks rotten floors | Braces against shoves and pushes others off the edge | Slow to turn, so faster crawlers flank it. Falls through rotten arena floors |
| Jumping legs | Ledges, small gaps and lava pools | Pounce from above, with damage from fall height. Hop away from a ram or the edge | Can't brace in the air, so a mid-jump shove sends it far. Hard landings hurt |
| Spike feet | Steep slopes, ice | Anchored: very hard to ring out | Slower steps, slow to reposition |
| Wide pad feet | Sludge, scree | Very stable stance | Slow, and a wide target |
| Magnet feet | Steel walls | Clings to a steel deck, nearly ring-out proof there | No help on dirt. Crane-magnet arenas pull it around |
| Lava-proof feet | Lava | Stands in lava where foes can't follow | Heavy, so slower |
| Insulated feet | Live floors | Ignores electrified arena tiles | Rubber has less grip on steel |
| Heat sinks | Hot zones | Doesn't overheat in hot arenas | Takes a top mount, where the pincer's pneumatics tank goes |
| Sealed drum hull | Floats | Round, hard to grab. Can roll out of trouble | Rolls when pushed too, can't hold a line |
| Winch or grapple | Hauling up, crossing gaps | Anchors to an arena post to refuse a ring-out. Reels a foe in | Takes a mount. The cable can be grabbed |
| Magnet tool | Carries and pulls metal | Yanks a light foe off balance | A heavier foe pulls *you* instead |
| Wedge plow | Pushes boulders | A real wedge: lifts and flips foes | A heavier foe just stops it |
| Pincer | Gates, levers, fuses | Grab and fling, with distance set by the foe's mass | Can't lift heavy foes. Needs a tank on a top mount |
| Lamp-eye | Dark zones | Spots wild crawlers first. Sees in dark arenas | Narrow cone, so it gets flanked |
| Tracks or wheels | Flats, ramp jumps | Fast rams | Bad on rough floors. Can't climb arena tiers |

Bigger ideas that follow:

1. **Each area's arena uses that area's terrain.** `Pit` themes already exist
   (`steel`, `yard`). A Furnace pit has a lava moat, the Stacks pit has tiers,
   the Live Yard pit has shock tiles. The part that opened an area is strongest
   in its arena, and the wild crawlers there are built for it.
2. **Terrain is a weapon.** Shove a foe into lava, off a tier, onto a rotten
   floor that gives way, or into a fan's air stream.
3. **Mounts are the budget.** Heat sinks, a winch and a magnet compete with the
   pneumatics tank for top mounts, so kit for exploring weakens kit for
   fighting. That's the trade-off at every refit bench.

The fight AI already picks its plan from the build (`pickWeapon` in
`battle.ts`). New parts would add plans such as **anchor** (spike feet or winch,
sit tight near the centre), **pounce** (jumping legs) and **lift** (millipede or
wedge, get under and flip).

## How you get parts

- **Scrap a wild crawler, keep one of its parts.**
- **Landmark crawlers** (tougher, named, one per area) guard the parts that open
  new areas.
- **Found in the world**, such as a pneumatics tank on a ledge you can see but
  not yet reach.

Refit benches (the green repair pad) repair you and let you swap the parts you
own, because a crawler can't carry everything at once.

## Candidate areas

- **The Yard** (start): rolling dirt, crates and drums. Open ground.
- **The Stacks**: crushed-car terraces, tall ledges, crate steps, rotten floors.
- **The Furnace**: lava pools and lakes, hot air, geysers.
- **The Drains**: low culverts, sludge, water.
- **The Live Yard**: electrified floors, fans, pressure plates.

## First prototype: the Yard and the Stilts

The goal is one area, some fights in it, one harder encounter, and a progression
item that physically lets you leave.

**Proposed first progression: the piston strider legs, which get you up a tall
ledge into the Stacks.**

Why this one first:

- **Smallest physics slice.** A ledge only needs real footholds and hull
  clearance, without movable objects, grip-limited pushing or heat. The plow and
  boulder need all of the hull-and-legs physics *plus* rigid bodies, so they
  make a good second progression.
- **No new part to model.** The strider kit already exists, animates and fights.
- **The barrier reads at a glance.** A wall is either taller than you can reach
  or it isn't, and you can see the top from below.
- **The trade-off shows up straight away.** The strider is the tippiest kit, so
  taking it makes fights in the Yard riskier.

### The loop

1. **Start** at the refit bench in the Yard with crab legs, a can-eye and a
   wedge. The only parts you own are what's fitted.
2. **The ledge.** One edge of the Yard is a wall about 1.0–1.1 high, the
   foot of the Stacks. The crab (reach 1.10, hip clearance 0.50) gets its front
   feet onto the lip but its hull catches, so it stalls. Something visible
   up top, such as a lamp on a pole or the next bench, shows there's more.
3. **Fights.** Wild crawlers roam the Yard as they do today. For this prototype
   none of them have strider or quad legs, so nothing in the Yard can climb the
   wall. Scrapping one lets you keep one of its parts (a telly, a lamp-eye, a
   pincer and tank), which gives the fights a reason without opening the ledge.
4. **The challenging encounter: "Stilts".** A landmark crawler on strider legs
   lives on top of the ledge. You can see it pacing up there and watching you
   (it has a lamp-eye, so it sees you from across the Yard). Once you've
   scrapped enough of the Yard's crawlers, it steps down off the ledge to deal
   with you. Coming down is a drop it can take but you can't reverse, which
   shows the ledge is one-way for you. It has more HP than a wild crawler and
   charges hard, so the fight is a step up. Losing tows you to the bench as
   usual, and Stilts goes back up.
5. **The reward.** Scrapping Stilts gives you its piston strider legs.
6. **Refit and climb.** Swap crab legs for strider legs at the bench. Walk to
   the wall, and the long legs reach the top and lift the hull over the lip.
   You're in the Stacks.
7. **The tease.** Just over the top sits the next barrier, such as a boulder in a
   gap (the plow progression) or the first car-stack terrace.

### What it needs

- **Physics slice:** remove the 0.75 climb rule. Feet only plant within reach of
  the hip on a surface that isn't too steep. The hull collides with ledge lips.
  Ledges are authored geometry in `world.ts`, not random scatter.
- **Owned parts:** an inventory. The win screen offers a part from the scrapped
  crawler. The refit bench opens the workshop bin filtered to parts you own.
- **Stilts:** a named wild crawler with more HP, a home on top of the ledge, and
  a trigger to come down.
- **The Yard roster:** wild crawlers without strider or quad legs.
- **Not needed yet:** Rapier, movable objects, new parts, new art or a new pit
  theme. The boss fight uses the existing yard pit, though a raised tier in
  that pit would show off the strider well later.

### What we'd learn

- Does a ledge read as "too tall for me" from the isometric pixel view?
- Does a crab stalling at the lip look physical and fair, or like an invisible
  wall?
- Is the Stilts fight a satisfying step up from wild fights?
- Does refitting to the strider feel like a trade-off, given it fights worse?

### Alternatives for the first progression

- **Plow → boulder.** The most direct test of "physics moves the rock", but it
  needs rigid bodies and grip-limited pushing on day one. Best as the second
  step.
- **Jumping legs → small lava pool.** Exciting, but needs a new part, jump
  physics and heat.
- **Millipede → low culvert.** Simple geometry like the ledge, but going *under*
  something reads less clearly from above than going *up*.

## Open decisions

1. **Physics depth.** Proposed: rigid hull, rocks and plow, with procedural legs
   that need real footholds and hold the hull up through springs. The
   alternative is fully motor-driven legs, which are truer to physics but much
   harder to tune and risk QWOP-style flailing.
2. **Engine.** Rapier, or a small hand-written layer. The first prototype may not
   need either beyond footholds and hull clearance.
3. **Fights.** Leave `battle.ts` as it is for the first prototype, or move fights
   onto the same physics straight away.
4. **Physics test course.** A separate course (step ledges at 0.4, 0.7, 1.0 and
   1.3; a heavy boulder and a light rock; narrow and wide lava; scree next to
   dirt; a shove near an edge) for tuning, alongside or instead of the first
   area.
