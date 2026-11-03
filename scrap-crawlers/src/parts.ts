// The workshop's parts catalogue and builds. A build is a body plus whatever is
// fitted to each of its mounts; designFor() turns it into a creature Design.
import { Design, HeadStyle, defaultDesign } from './creature';
import { SlotId, ToolSlot } from './tools';

/** Every mount except the sides, which are for legs. */
const ANY: ToolSlot[] = ['fronttop', 'front', 'spine', 'back'];
const TOPS: ToolSlot[] = ['fronttop', 'spine', 'back'];

export type BodyId = 'box' | 'drum' | 'pod';
export type Section = 'legs' | 'control' | 'weapons';

export const BODIES: { id: BodyId; name: string; blurb: string; ready: boolean }[] = [
  { id: 'box', name: 'Box hull', blurb: 'A crate with mounts on every face.', ready: true },
  { id: 'drum', name: 'Drum hull', blurb: 'Rolled steel barrel.', ready: false },
  { id: 'pod', name: 'Pod hull', blurb: 'Rounded shell.', ready: false },
];

export interface PartDef {
  id: string;
  name: string;
  section: Section;
  fits: SlotId[];
  blurb: string;
  stats: string[];
  /** Another part this one needs somewhere on the build before it works. */
  needs?: string;
  /** Listed under the part that needs it in the parts bin. */
  under?: string;
  apply: (d: Design, slot: SlotId) => void;
}

const legKit = (id: string, name: string, blurb: string, stats: string[], kit: Partial<Design>): PartDef => ({
  id, name, section: 'legs', fits: ['legs'], blurb, stats, apply: (d) => Object.assign(d, kit),
});
const control = (id: HeadStyle, name: string, blurb: string, stats: string[], antennae: number): PartDef => ({
  id, name, section: 'control', fits: ANY, blurb, stats,
  apply: (d, slot) => Object.assign(d, { head: id, headSlot: slot, antennae }),
});

export const PARTS: PartDef[] = [
  legKit('crab', 'Pipe crab kit', 'Six plumbing-pipe legs. The all-rounder.', ['6 legs', 'trot', 'steady'], {
    legPairs: 3, legSegments: 3, legLength: 1.1, legSpread: 1, legStyle: 'pipe', gait: 'trot',
    rideHeight: 0.45, stepHeight: 0.3, stepSpeed: 1, walkSpeed: 1.6,
  }),
  legKit('quad', 'Girder quad kit', 'Four heavy girder legs. Armoured, slow to turn.', ['4 legs', 'armoured', 'slow'], {
    legPairs: 2, legSegments: 2, legLength: 1.35, legSpread: 1.1, legStyle: 'girder', gait: 'trot',
    rideHeight: 0.4, stepHeight: 0.25, stepSpeed: 0.8, walkSpeed: 1.1,
  }),
  legKit('millipede', 'Millipede kit', 'Eight stubby legs that scurry. Low and grippy.', ['8 legs', 'scurry', 'low'], {
    legPairs: 4, legSegments: 2, legLength: 0.8, legSpread: 1.2, legStyle: 'pipe', gait: 'scurry',
    rideHeight: 0.35, stepHeight: 0.2, stepSpeed: 1.5, walkSpeed: 1.9,
  }),
  legKit('strider', 'Piston strider kit', 'Two long piston legs. Fast, tall, wobbly.', ['2 legs', 'fast', 'tippy'], {
    legPairs: 1, legSegments: 3, legLength: 1.5, legSpread: 1, legStyle: 'piston', gait: 'trot',
    rideHeight: 0.55, stepHeight: 0.35, stepSpeed: 1.3, walkSpeed: 2.2,
  }),
  control('can', 'Can-eye', 'One wide lens in a soup can. Steady but short-sighted.', ['wide view', 'short range'], 2),
  control('tv', 'Telly', 'Two eyes on a little screen. Good at judging distance.', ['depth', 'mid range'], 1),
  control('lamp', 'Lamp-eye', 'A huge lens in a lamp shade. Sees far, narrow cone.', ['long range', 'narrow'], 0),
  {
    id: 'wedge', name: 'Wedge plow', section: 'weapons', fits: ['front'], stats: ['passive', 'shove', 'tip'],
    blurb: 'A hinged steel ramp that scrapes the floor. Gets under things.',
    apply: (d) => (d.tools.front = 'wedge'),
  },
  {
    id: 'pincer', name: 'Pincer claw', section: 'weapons', fits: ANY, stats: ['grab', 'hold'], needs: 'pneumatics',
    blurb: 'Air-driven jaws that grab and hold. Reaches the way its mount faces. Dead weight without air.',
    apply: (d, slot) => (d.tools[slot as ToolSlot] = 'pincer'),
  },
  {
    id: 'pneumatics', name: 'Pneumatics tank', section: 'weapons', fits: TOPS, stats: ['powers pincer'], under: 'pincer',
    blurb: 'Compressed air for the pincer. Sits on any top mount.',
    apply: (d, slot) => (d.tools[slot as ToolSlot] = 'pneumatics'),
  },
];

export const part = (id: string) => PARTS.find((p) => p.id === id);

export const SLOT_NAMES: Record<SlotId, string> = {
  legs: 'Sides', fronttop: 'Front top', front: 'Front face', spine: 'Spine', back: 'Back top',
};
export const SLOT_ORDER: SlotId[] = ['legs', 'fronttop', 'front', 'spine', 'back'];

export interface Build {
  body: BodyId | null;
  slots: Partial<Record<SlotId, string>>;
}

export function designFor(b: Build): Design {
  const d: Design = {
    ...defaultDesign(), seed: 3, legPairs: 0, head: null, antennae: 0, tail: 0, tools: {}, junk: false,
  };
  for (const slot of SLOT_ORDER) {
    const p = b.slots[slot] ? part(b.slots[slot]!) : undefined;
    if (!p) continue;
    p.apply(d, slot);
  }
  return d;
}

/** Things the build sheet should warn about. */
export function problems(b: Build): string[] {
  if (!b.body) return [];
  const fitted = Object.values(b.slots);
  const out: string[] = [];
  if (!b.slots.legs) out.push('No legs: it can only sit there.');
  if (!fitted.some((id) => part(id!)?.section === 'control')) out.push('No control module: nothing is driving.');
  for (const id of new Set(fitted)) {
    const p = part(id!);
    if (p?.needs && !fitted.includes(p.needs)) out.push(`${p.name} has no air: fit a ${part(p.needs)!.name} on a top mount.`);
  }
  return out;
}

/** Fits a part to a mount. A build has one control module, so fitting another moves it. */
export function fit(b: Build, slot: SlotId, id: string): Build {
  const slots = { ...b.slots };
  if (part(id)?.section === 'control') {
    for (const s of SLOT_ORDER) if (part(slots[s] ?? '')?.section === 'control') delete slots[s];
  }
  slots[slot] = id;
  return { body: b.body, slots };
}

/** Builds round-trip through the URL as `box;legs=crab;fronttop=tv;front=pincer`. */
export function encodeBuild(b: Build): string {
  if (!b.body) return '';
  return [b.body, ...SLOT_ORDER.filter((s) => b.slots[s]).map((s) => `${s}=${b.slots[s]}`)].join(';');
}

export function decodeBuild(s: string | null): Build {
  const b: Build = { body: null, slots: {} };
  if (!s) return b;
  const [body, ...rest] = s.split(';');
  if (!BODIES.some((x) => x.id === body && x.ready)) return b;
  b.body = body as BodyId;
  for (const kv of rest) {
    const [slot, id] = kv.split('=') as [SlotId, string];
    if (part(id)?.fits.includes(slot)) Object.assign(b, fit(b, slot, id));
  }
  return b;
}
