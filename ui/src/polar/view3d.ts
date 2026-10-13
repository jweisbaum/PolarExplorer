/**
 * What the 3D view shows, worked out from the packed scene (spec.md 10.1–
 * 10.3) without three.js, so it can be tested in Node: which dots are drawn
 * and how, the surfaces, the camera presets, the axis guides, and what a
 * selection holds.
 *
 * A dot is named everywhere by its **global index**: node `k` is `k`, sample
 * `k` is `nodes.count + k`. The scene draws only the dots the toggles leave
 * on, so `DotBuild.refs` maps each drawn dot back to its global index.
 */
import { NO_WAVE_RANGES, waveRangePredicate, type WaveRanges } from "./waveRanges";
import { CELL_EVERY } from "./waveSplit";
import { dayBand, dayBandRgb } from "../dayBand";
import type { PolarCell } from "../generated/PolarCell";
import type { PolarNodeRef } from "../generated/PolarNodeRef";
import type { SpeedUnit } from "../generated/SpeedUnit";
import { CARTESIAN_TWA_SCALE, place, type Layout, type PolarGrid } from "./geometry3d";
import { BLEND_SOURCE, FLAG_EDITED, FLAG_EXCLUDED, FLAG_FILTERED, sampleId, type ScenePacket, type SplitPacket } from "./scenePacket";
import { SHAPE_CROSS, SHAPE_DISC, SHAPE_RING, SHAPE_SQUARE, type SurfaceInput, type View } from "./scene3d";
import type { PdfBox, PdfVolumeInput } from "./pdfVolume";

export type ColourMode = "source" | "hs" | "current" | "time" | "timeOfDay" | "wavePeriod" | "waveAngle" | "waveWindAngle";

/** The show toggles (spec.md 10.2). */
export interface Toggles {
  samples: boolean;
  nodes: boolean;
  surfaces: boolean;
  /** Samples removed by the filters, drawn dimmed. */
  filtered: boolean;
  /** Samples and polar points the person excluded; hidden unless asked for (asked 2026-10-04). */
  excluded: boolean;
}

export const DEFAULT_TOGGLES: Toggles = { samples: true, nodes: true, surfaces: true, filtered: false, excluded: false };

/** Step used only to normalize the truncated Gaussian density, in knots. */
const PDF_MASS_STEP_KN = 0.5;
const PDF_SPEED_MIN_STDDEV_KN = 0.5;
const PDF_SPEED_MAX_KN = 60;
const PDF_TAIL_CUTOFF = 0.005;
const PDF_CONTEXT_ALPHA = 0.12;
export type PdfVolume = PdfVolumeInput;

export interface PdfCellStatistics {
  windCell: number;
  copy: number;
  twa: number;
  tws: number;
  count: number;
  mean: number;
  stddev: number;
  mass: number;
  percentiles: readonly [number, number, number, number, number];
}

export interface PdfDensity {
  /** Continuous Gaussian fields, one per wind-condition cell. */
  volumes: PdfVolume[];
  /** Number of occupied wind-cell PDFs. */
  count: number;
  /** The largest probability density across the shown wind cells, per knot. */
  maxDensity: number;
  /** Empirical statistics for each occupied wind cell and split-wave copy. */
  cells: PdfCellStatistics[];
}

/**
 * Fits a Gaussian to the visible speeds in each output-grid wind cell, then
 * defines its normalized density over boat speed. A small minimum deviation
 * keeps single-speed cells visible as narrow curves instead of point masses.
 * The renderer integrates each field directly, without boat-speed slices.
 */
export function buildPdfDensity(
  points: Float32Array,
  twaAxis: ArrayLike<number>,
  twsAxis: ArrayLike<number>,
  _layout: Layout,
  splitCells: Int16Array | null = null,
  splitCount = 0,
  focus: { windCell: number; copy: number } | null = null,
): PdfDensity {
  const twaCount = twaAxis.length, twsCount = twsAxis.length;
  if (points.length % 3 !== 0 || twaCount === 0 || twsCount === 0 || (splitCells && splitCells.length !== points.length / 3)) {
    return { volumes: [], count: 0, maxDensity: 0, cells: [] };
  }
  const copies = splitCells ? splitCount : 1;
  if (copies < 1) return { volumes: [], count: 0, maxDensity: 0, cells: [] };
  const windCells = twaCount * twsCount;
  const summaries = new Map<number, { count: number; sum: number; sumSquares: number; speeds: number[] }>();
  const integrationSteps = Math.ceil(PDF_SPEED_MAX_KN / PDF_MASS_STEP_KN);
  const nearest = (axis: ArrayLike<number>, value: number): number => {
    let lo = 0, hi = axis.length;
    while (lo < hi) {
      const mid = (lo + hi) >>> 1;
      if (axis[mid]! < value) lo = mid + 1;
      else hi = mid;
    }
    if (lo === 0) return 0;
    if (lo === axis.length) return axis.length - 1;
    return value - axis[lo - 1]! <= axis[lo]! - value ? lo - 1 : lo;
  };
  const add = (i: number, j: number, speed: number, copy: number) => {
    const wi = copy * windCells + i * twsCount + j;
    const summary = summaries.get(wi) ?? { count: 0, sum: 0, sumSquares: 0, speeds: [] };
    summary.count++;
    summary.sum += speed;
    summary.sumSquares += speed * speed;
    summary.speeds.push(speed);
    summaries.set(wi, summary);
  };
  const count = points.length / 3;
  for (let d = 0; d < count; d++) {
    const twa = points[d * 3]!, tws = points[d * 3 + 1]!, bsp = points[d * 3 + 2]!;
    if (!Number.isFinite(twa) || !Number.isFinite(tws) || !Number.isFinite(bsp)) continue;
    const i = nearest(twaAxis, twa), j = nearest(twsAxis, tws);
    if (i < 0 || j < 0 || bsp < 0 || bsp > 60) continue;
    if (!splitCells) add(i, j, bsp, 0);
    else {
      const cell = splitCells[d]!;
      if (cell === CELL_EVERY) for (let copy = 0; copy < copies; copy++) add(i, j, bsp, copy);
      else if (cell >= 0 && cell < copies) add(i, j, bsp, cell);
    }
  }
  const twaBounds = Array.from({ length: twaCount }, (_, i) => axisBounds(twaAxis, i, 0, 360));
  const twsBounds = Array.from({ length: twsCount }, (_, i) => axisBounds(twsAxis, i, 0, Infinity));
  const occupied = summaries.size;
  let maxDensity = 0;
  const curves = new Map<number, { mean: number; stddev: number; mass: number }>();
  const cellStatistics: PdfCellStatistics[] = [];
  for (const [wi, summary] of summaries) {
    const copy = Math.floor(wi / windCells), wind = wi % windCells;
    const i = Math.floor(wind / twsCount), j = wind % twsCount;
    const sorted = [...summary.speeds].sort((a, b) => a - b);
    const percentile = (p: number) => {
      const at = (sorted.length - 1) * p;
      const lo = Math.floor(at), hi = Math.ceil(at);
      return sorted[lo]! + (sorted[hi]! - sorted[lo]!) * (at - lo);
    };
    const mean = summary.sum / summary.count;
    const variance = Math.max(0, summary.sumSquares / summary.count - mean * mean);
    const stddev = Math.max(PDF_SPEED_MIN_STDDEV_KN, Math.sqrt(variance));
    let mass = 0;
    for (let speedBin = 0; speedBin < integrationSteps; speedBin++) {
      const speed = (speedBin + 0.5) * PDF_MASS_STEP_KN;
      const z = (speed - mean) / stddev;
      const density = Math.exp(-0.5 * z * z) / (stddev * Math.sqrt(2 * Math.PI));
      mass += density * PDF_MASS_STEP_KN;
    }
    if (mass <= 0) continue;
    cellStatistics.push({
      windCell: wind, copy, twa: twaAxis[i]!, tws: twsAxis[j]!, count: summary.count,
      mean, stddev, mass,
      percentiles: [percentile(0.1), percentile(0.25), percentile(0.5), percentile(0.75), percentile(0.9)],
    });
    // The continuous shader reaches the exact peak; sampling that peak at
    // bin midpoints would clip the colour ramp into a plateau near the mean.
    maxDensity = Math.max(maxDensity, 1 / (stddev * Math.sqrt(2 * Math.PI) * mass));
    curves.set(wi, { mean, stddev, mass });
  }
  const boxes: PdfBox[] = [];
  for (const [wi, fitted] of curves) {
    const copy = Math.floor(wi / windCells), wind = wi % windCells;
    const i = Math.floor(wind / twsCount), j = wind % twsCount;
    const peak = 1 / (fitted.stddev * Math.sqrt(2 * Math.PI) * fitted.mass);
    if (peak < maxDensity * PDF_TAIL_CUTOFF) continue;
    const b0 = Math.max(0, fitted.mean - 4 * fitted.stddev);
    const b1 = Math.min(PDF_SPEED_MAX_KN, fitted.mean + 4 * fitted.stddev);
    if (b1 <= b0) continue;
    const isFocused = focus?.windCell === wind && focus.copy === copy;
    boxes.push({
      windCell: wind, copy: splitCells ? copy : -1,
      angle: twaBounds[i]!, wind: twsBounds[j]!, speed: [b0, b1],
      gaussian: [fitted.mean, fitted.stddev, fitted.mass],
      opacity: focus && !isFocused ? PDF_CONTEXT_ALPHA : 1,
    });
  }
  const volumes = boxes.length > 0 ? [{ boxes, maxDensity }] : [];
  return {
    volumes, count: occupied, maxDensity, cells: cellStatistics,
  };
}

/** Halfway boundaries around a wind-axis point, clipped to its valid range. */
function axisBounds(axis: ArrayLike<number>, index: number, min: number, max: number): [number, number] {
  const value = axis[index]!;
  const before = index === 0 ? value - (axis[index + 1] !== undefined ? axis[index + 1]! - value : 1) / 2 : (axis[index - 1]! + value) / 2;
  const after = index + 1 === axis.length ? value + (index > 0 ? value - axis[index - 1]! : 1) / 2 : (value + axis[index + 1]!) / 2;
  return [Math.max(min, before), Math.min(max, after)];
}

/**
 * Edit mode (spec.md 10.4): the source being edited, by its index in the
 * packet's sources, is drawn fully; the others fade, or are hidden.
 */
export interface Focus {
  index: number;
  hideOthers: boolean;
}

/** The focused source's index in the packet, or -1 when it is not in the scene (hidden). */
export function focusIndex(packet: ScenePacket, sourceId: number | null): number {
  if (sourceId === null) return -1;
  return packet.sources.findIndex((s) => s.id === sourceId);
}

/** The dots to draw. */
export interface DotBuild {
  points: Float32Array;
  colors: Float32Array;
  shapes: Float32Array;
  /** The global index of each drawn dot. */
  refs: Uint32Array;
}

/** `#rrggbb` → (r, g, b) in 0–1. */
export function rgb(colour: string): [number, number, number] {
  const value = Number.parseInt(colour.slice(1), 16);
  if (!/^#[0-9a-fA-F]{6}$/.test(colour)) return [0.5, 0.5, 0.5];
  return [((value >> 16) & 255) / 255, ((value >> 8) & 255) / 255, (value & 255) / 255];
}

/** A value with no data: neutral grey. */
const MISSING: [number, number, number] = [0.55, 0.55, 0.55];

/**
 * A perceptual ramp (viridis, five stops) for colouring by a value, `t` in
 * 0–1. Dark purple is low, yellow is high.
 */
const RAMP: readonly [number, number, number][] = [
  [0.267, 0.005, 0.329], [0.229, 0.322, 0.546], [0.128, 0.567, 0.551], [0.369, 0.789, 0.383], [0.993, 0.906, 0.144],
];
export function ramp(t: number): [number, number, number] {
  const x = Math.min(1, Math.max(0, t)) * (RAMP.length - 1);
  const i = Math.min(RAMP.length - 2, Math.floor(x));
  const f = x - i;
  const a = RAMP[i]!, b = RAMP[i + 1]!;
  return [a[0] + (b[0] - a[0]) * f, a[1] + (b[1] - a[1]) * f, a[2] + (b[2] - a[2]) * f];
}

/**
 * The sample values a colour mode reads along the ramp, or null for the
 * modes that have none: "by source", and "by time of day", whose four
 * bands are categories read from the samples' flags.
 */
export function modeValues(packet: ScenePacket, mode: ColourMode): Float32Array | null {
  if (mode === "hs") return packet.samples.hs;
  if (mode === "current") return packet.samples.current;
  if (mode === "time") return packet.samples.time;
  if (mode === "wavePeriod" || mode === "waveAngle" || mode === "waveWindAngle") return packet.samples[mode];
  return null;
}

/** The finite range of some values, or null when none is finite. */
export function range(values: ArrayLike<number>): [number, number] | null {
  let lo = Infinity, hi = -Infinity;
  for (let i = 0; i < values.length; i++) {
    const v = values[i]!;
    if (Number.isFinite(v)) { if (v < lo) lo = v; if (v > hi) hi = v; }
  }
  return lo <= hi ? [lo, hi] : null;
}

/**
 * Which colour modes have anything to show (spec.md 10.2). Hs and current
 * arrive with the environment (M9) and time with tracks (M8); until a sample
 * has one, its mode is offered disabled.
 */
export function availableModes(packet: ScenePacket): Record<ColourMode, boolean> {
  return {
    source: true,
    hs: range(packet.samples.hs) !== null,
    current: range(packet.samples.current) !== null,
    time: packet.samples.count > 0,
    timeOfDay: packet.samples.count > 0,
    wavePeriod: range(packet.samples.wavePeriod) !== null,
    waveAngle: range(packet.samples.waveAngle) !== null,
    waveWindAngle: range(packet.samples.waveWindAngle) !== null,
  };
}

/** Whether any sample or polar point is excluded, so "show excluded" has something to show. */
export function hasExcluded(packet: ScenePacket): boolean {
  for (const set of [packet.nodes, packet.samples]) {
    for (let k = 0; k < set.count; k++) if (set.flags[k]! & FLAG_EXCLUDED) return true;
  }
  return false;
}

/** Whether any sample is filtered out, so "show filtered" has something to show. */
export function hasFiltered(packet: ScenePacket): boolean {
  for (let k = 0; k < packet.samples.count; k++) if (packet.samples.flags[k]! & FLAG_FILTERED) return true;
  return false;
}

/**
 * Dimming a colour channel toward grey, `c → DIM_KEEP · c + DIM_ADD`: once
 * for a filtered sample (spec.md 10.2), twice for another source's dot
 * while one is edited (spec.md 10.4).
 */
const DIM_KEEP = 0.35;
const DIM_ADD = 0.65 * 0.5;

/**
 * The dots the toggles leave on, coloured by `mode`. Polar nodes have no
 * environment, so they keep their source colour in every mode. Excluded
 * samples are rings, excluded nodes crosses (spec.md 10.3), edited nodes
 * squares (spec.md 10.4). In edit mode the other sources' dots are dimmed,
 * or left out.
 */
export function buildDots(packet: ScenePacket, toggles: Toggles, mode: ColourMode, focus: Focus | null = null): DotBuild {
  return mergeDots(nodeDots(packet, toggles, focus), sampleDots(packet, toggles, mode, focus));
}

/**
 * The polar nodes' part of `buildDots`. An edit changes only these (and the
 * samples' flags), so the view rebuilds them alone when the samples are the
 * ones it already holds (plan.md M13).
 */
export function nodeDots(packet: ScenePacket, toggles: Toggles, focus: Focus | null = null): DotBuild {
  return dotsOf(packet, toggles, "source", focus, "nodes");
}

/** The samples' part of `buildDots`. */
export function sampleDots(packet: ScenePacket, toggles: Toggles, mode: ColourMode, focus: Focus | null = null, waveRanges: WaveRanges = NO_WAVE_RANGES): DotBuild {
  return dotsOf(packet, toggles, mode, focus, "samples", waveRanges);
}

/** Two parts drawn together, nodes first. */
export function mergeDots(a: DotBuild, b: DotBuild): DotBuild {
  const join = <T extends Float32Array | Uint32Array>(x: T, y: T, make: (n: number) => T): T => {
    const out = make(x.length + y.length);
    out.set(x);
    out.set(y, x.length);
    return out;
  };
  return {
    points: join(a.points, b.points, (n) => new Float32Array(n)),
    colors: join(a.colors, b.colors, (n) => new Float32Array(n)),
    shapes: join(a.shapes, b.shapes, (n) => new Float32Array(n)),
    refs: join(a.refs, b.refs, (n) => new Uint32Array(n)),
  };
}

function dotsOf(packet: ScenePacket, toggles: Toggles, mode: ColourMode, focus: Focus | null, which: "nodes" | "samples", waveRanges: WaveRanges = NO_WAVE_RANGES): DotBuild {
  const { nodes, samples } = packet;
  const sourceColours = packet.sources.map((s) => rgb(s.colour));
  const values = which === "samples" ? modeValues(packet, mode) : null;
  const byBand = which === "samples" && mode === "timeOfDay";
  const span = values ? range(values) : null;
  // Two passes over flat arrays, no per-dot allocation: this runs on every
  // edit at up to 200,000 dots (spec.md 13).
  const refs = new Uint32Array(which === "nodes" ? nodes.count : samples.count);
  const focused = focus !== null && focus.index >= 0;
  const hidden = (source: number) => focused && focus.hideOthers && source !== focus.index;
  let n = 0;
  if (which === "nodes" && toggles.nodes) {
    for (let k = 0; k < nodes.count; k++) {
      if ((nodes.flags[k]! & FLAG_EXCLUDED) && !toggles.excluded) continue;
      if (!hidden(nodes.source[k]!)) refs[n++] = k;
    }
  }
  if (which === "samples" && toggles.samples) {
    const inWaveRange = waveRangePredicate(samples, waveRanges);
    for (let k = 0; k < samples.count; k++) {
      if ((samples.flags[k]! & FLAG_FILTERED) && !toggles.filtered) continue;
      if ((samples.flags[k]! & FLAG_EXCLUDED) && !toggles.excluded) continue;
      if (hidden(samples.source[k]!) || !inWaveRange(k)) continue;
      refs[n++] = nodes.count + k;
    }
  }
  const points = new Float32Array(n * 3);
  const colors = new Float32Array(n * 3);
  const shapes = new Float32Array(n);
  const lo = span ? span[0] : 0;
  const width = span && span[1] > span[0] ? span[1] - span[0] : 0;
  for (let d = 0; d < n; d++) {
    const g = refs[d]!;
    const isNode = g < nodes.count;
    const k = isNode ? g : g - nodes.count;
    const set = isNode ? nodes : samples;
    points[d * 3] = set.points[k * 3]!;
    points[d * 3 + 1] = set.points[k * 3 + 1]!;
    points[d * 3 + 2] = set.points[k * 3 + 2]!;
    const flags = set.flags[k]!;
    let colour: readonly [number, number, number];
    if (!isNode && byBand) {
      colour = dayBandRgb(dayBand(flags));
    } else if (!isNode && values) {
      const v = values[k]!;
      colour = span && Number.isFinite(v) ? ramp(width > 0 ? (v - lo) / width : 0.5) : MISSING;
    } else {
      colour = sourceColours[set.source[k]!] ?? MISSING;
    }
    // Dimming toward grey, in place (no per-dot allocation): once for a
    // filtered sample, twice for another source's dot while one is edited.
    let fades = !isNode && flags & FLAG_FILTERED ? 1 : 0;
    if (focused && set.source[k] !== focus.index) fades += 2;
    let [r, gr, b] = colour;
    for (let f = 0; f < fades; f++) {
      r = DIM_KEEP * r + DIM_ADD;
      gr = DIM_KEEP * gr + DIM_ADD;
      b = DIM_KEEP * b + DIM_ADD;
    }
    colors[d * 3] = r;
    colors[d * 3 + 1] = gr;
    colors[d * 3 + 2] = b;
    shapes[d] = flags & FLAG_EXCLUDED ? (isNode ? SHAPE_CROSS : SHAPE_RING)
      : isNode && flags & FLAG_EDITED ? SHAPE_SQUARE : SHAPE_DISC;
  }
  return { points, colors, shapes, refs: refs.slice(0, n) };
}

/** A surface's grid for the scene: the packet's own arrays, TWA-major, NaN an empty cell; nothing is copied (M17a). */
export function surfaceGrid(twa: Float32Array, tws: Float32Array, bsp: Float32Array): PolarGrid {
  return { twa, tws, bsp };
}

/** How faint the other sources' surfaces are while one is edited (spec.md 10.4). */
export const FADED_OPACITY = 0.05;

/**
 * Every surface to draw: one per visible polar source, the blend opaque. In
 * edit mode the source being edited is opaque and the others fade, or are
 * left out.
 */
export function buildSurfaces(packet: ScenePacket, blendColour: string, focus: Focus | null = null,
  blendLine?: string, split: SplitPacket | null = null): SurfaceInput[] {
  const focused = focus !== null && focus.index >= 0;
  const surfaces = packet.surfaces.flatMap((surface): SurfaceInput[] => {
    const blend = surface.source === BLEND_SOURCE;
    // In a split view each copy has a blend of its own (spec.md 10.5): the
    // whole blend gives way to them.
    if (blend && split) return [];
    const mine = focused && surface.source === focus.index;
    if (focused && !mine && focus.hideOthers) return [];
    return [{
      grid: surfaceGrid(surface.twa, surface.tws, surface.bsp),
      color: blend ? blendColour : packet.sources[surface.source]?.colour ?? "#888888",
      opaque: blend || mine,
      // Hovering the blend names the sources behind a cell (spec.md 10.1).
      ...(blend ? { pickable: true } : {}),
      ...(blend && blendLine !== undefined ? { lineColor: blendLine } : {}),
      ...(focused && !mine ? { opacity: FADED_OPACITY } : {}),
    }];
  });
  for (const surface of split?.surfaces ?? []) {
    surfaces.push({
      grid: surfaceGrid(surface.twa, surface.tws, surface.bsp),
      color: blendColour, opaque: true, pickable: true, cell: surface.cell,
      ...(blendLine !== undefined ? { lineColor: blendLine } : {}),
    });
  }
  return surfaces;
}

/**
 * The index of the value on `axis` nearest `value` (the lower on a tie), or
 * -1 for an empty axis: the output-grid cell a point on the blend belongs
 * to, where the drawn surface is finer than the grid (spline mode).
 */
export function nearestIndex(axis: ArrayLike<number>, value: number): number {
  let best = -1;
  let distance = Infinity;
  for (let k = 0; k < axis.length; k++) {
    const d = Math.abs(axis[k]! - value);
    if (d < distance) { distance = d; best = k; }
  }
  return best;
}

/** Whether a node belongs to a track: a polar segment's, shown only in edit mode, and never excluded. */
function isSegmentNode(packet: ScenePacket, k: number): boolean {
  return packet.sources[packet.nodes.source[k]!]?.kind === "track";
}

/** The cells of the edited source among some dots: what the edit tools act on. */
export function editCells(packet: ScenePacket, selection: readonly number[], index: number): PolarCell[] {
  const out: PolarCell[] = [];
  for (const g of selection) {
    if (g >= packet.nodes.count || packet.nodes.source[g] !== index) continue;
    const cell = packet.nodes.cell[g]!;
    out.push({ twa_index: cell & 0xffff, tws_index: cell >>> 16 });
  }
  return out;
}

/** The global indices of the edited source's nodes at some cells (TWA index | TWS index << 16). */
export function nodesAtCells(packet: ScenePacket, index: number, cells: ReadonlySet<number>): number[] {
  const out: number[] = [];
  for (let k = 0; k < packet.nodes.count; k++) {
    if (packet.nodes.source[k] === index && cells.has(packet.nodes.cell[k]!)) out.push(k);
  }
  return out;
}

// ------------------------------------------------------------------ units

/** Knots to the display unit (spec.md 3.4); stored values stay knots. */
export const SPEED_FACTOR: Record<SpeedUnit, number> = { kn: 1, ms: 1852 / 3600, kmh: 1.852 };
/** The display unit's symbol. */
export const SPEED_SYMBOL: Record<SpeedUnit, string> = { kn: "kn", ms: "m/s", kmh: "km/h" };

// ----------------------------------------------------------------- bounds

/** A model-space box. */
export interface Bounds {
  min: [number, number, number];
  max: [number, number, number];
}

/**
 * The model-space box round every dot and surface that counts, or a default
 * polar-sized one when there are none. A point not drawn does not stretch
 * the scales: an excluded one unless "Excluded" is ticked, a filtered-out
 * one unless "Filtered" is, a sample a wave range hides. Excluding an
 * outlier brings the axes back to the data that is left, and showing it
 * again takes them out to it (asked 2026-10-04).
 */
export function sceneBounds(
  packet: ScenePacket, layout: Layout, waveRanges: WaveRanges = NO_WAVE_RANGES,
  shown: Pick<Toggles, "excluded" | "filtered"> = { excluded: false, filtered: false },
): Bounds {
  const min: [number, number, number] = [Infinity, Infinity, Infinity];
  const max: [number, number, number] = [-Infinity, -Infinity, -Infinity];
  const add = (twa: number, tws: number, bsp: number) => {
    if (![twa, tws, bsp].every(Number.isFinite)) return;
    const p = place(twa, tws, bsp, layout);
    for (let a = 0; a < 3; a++) { min[a] = Math.min(min[a]!, p[a]!); max[a] = Math.max(max[a]!, p[a]!); }
  };
  const inWaveRange = waveRangePredicate(packet.samples, waveRanges);
  for (const set of [packet.nodes, packet.samples]) {
    for (let k = 0; k < set.count; k++) {
      if ((set.flags[k]! & FLAG_EXCLUDED) && !shown.excluded) continue;
      if ((set.flags[k]! & FLAG_FILTERED) && !shown.filtered) continue;
      if (set === packet.samples && !inWaveRange(k)) continue;
      add(set.points[k * 3]!, set.points[k * 3 + 1]!, set.points[k * 3 + 2]!);
    }
  }
  // The blend has no nodes of its own; its surface belongs in the view. A
  // speed of 0 is its row at 0°, head to wind, across the whole output
  // grid: it would stretch the wind scale to the grid's top wind speed
  // whatever the data, and the origin is in the box anyway.
  for (const surface of packet.surfaces) {
    if (surface.source !== BLEND_SOURCE) continue;
    const nj = surface.tws.length;
    surface.twa.forEach((twa, i) => {
      surface.tws.forEach((tws, j) => {
        const bsp = surface.bsp[i * nj + j]!;
        if (bsp > 0) add(twa, tws, bsp);
      });
    });
  }
  if (!Number.isFinite(min[0])) {
    add(0, 0, 0);
    add(180, 20, 10);
    add(90, 0, 10);
  }
  // The origin belongs in the view: speeds and wind start at zero.
  add(0, 0, 0);
  return { min, max };
}

/** How far from the origin `bounds` reach: what a camera looking at the origin frames. */
function reachOf(bounds: Bounds): number {
  return Math.hypot(...[0, 1, 2].map((a) => Math.max(Math.abs(bounds.min[a]!), Math.abs(bounds.max[a]!))));
}

/**
 * `view` for a scene whose box went from `from` to `to`: target and camera
 * scaled about the origin, where every polar starts, so the camera keeps
 * the angle it looks from and the data the room it had on screen. Excluding
 * an outlier zooms in on what is left (asked 2026-10-04) without undoing
 * the person's own rotation and zoom, as a preset would.
 */
export function rescaleView(view: View, from: Bounds, to: Bounds): View {
  const before = reachOf(from), after = reachOf(to);
  if (!(before > 0) || !(after > 0) || Math.abs(after / before - 1) < 1e-3) return view;
  const k = after / before;
  const scale = (p: readonly [number, number, number]) => p.map((v) => v * k) as [number, number, number];
  return { position: scale(view.position), target: scale(view.target) };
}

export type CameraPreset = "top" | "side" | "iso";

/**
 * A preset camera (spec.md 10.1): **top** looks down the vertical axis — in
 * the polar tower that is the classic polar diagram, every TWS stacked;
 * **side** looks across it, so each TWS is a level; **iso** is the three-
 * quarter view. Far enough back that the whole box fits a `fov`° lens.
 */
/**
 * A preset camera framing `bounds`, looking at their middle or (`"origin"`,
 * the 3D polar since 2026-10-03) at the origin: the polar's axis at no wind,
 * from far enough back that the corner farthest from it is in the frame.
 */
export function presetView(preset: CameraPreset, bounds: Bounds, fov = 40, look: "middle" | "origin" = "middle"): View {
  const target: [number, number, number] = look === "origin"
    ? [0, 0, 0]
    : [0, 1, 2].map((a) => (bounds.min[a]! + bounds.max[a]!) / 2) as [number, number, number];
  const radius = Math.max(1, look === "origin"
    ? reachOf(bounds)
    : Math.hypot(...[0, 1, 2].map((a) => bounds.max[a]! - bounds.min[a]!)) / 2);
  const distance = (radius / Math.tan(((fov / 2) * Math.PI) / 180)) * 1.1;
  // `up` is +z; straight down would leave the view's roll undefined, so the
  // top view leans a hair toward -y.
  const direction: [number, number, number] = preset === "top" ? [0, -0.001, 1]
    : preset === "side" ? [1, 0, 0] : [0.6, -0.6, 0.5];
  const length = Math.hypot(...direction);
  return {
    position: [0, 1, 2].map((a) => target[a]! + (direction[a]! / length) * distance) as [number, number, number],
    target,
  };
}

// ----------------------------------------------------------------- guides

/** A tick label: where in model space, and what it says. */
export interface GuideLabel {
  at: [number, number, number];
  text: string;
}

/** Axis guide lines (segments) and their tick labels. */
export interface Guides {
  segments: Float32Array;
  labels: GuideLabel[];
}

/** Round tick values from 0 to `max`, about `count` of them. */
export function ticks(max: number, count = 5): number[] {
  if (!(max > 0)) return [0];
  const raw = max / count;
  const step = [1, 2, 5, 10].map((m) => m * 10 ** Math.floor(Math.log10(raw))).find((s) => s >= raw) ?? raw;
  const out: number[] = [];
  for (let v = 0; v <= max + 1e-9; v += step) out.push(Number(v.toFixed(6)));
  return out;
}

/**
 * The axis legend drawn in the scene (spec.md 10.1), in the display unit.
 * Polar tower: BSP rings on the floor with their speeds, TWA spokes every
 * 30° with their angles, and the TWS axis up the middle with its speeds.
 * Cartesian: the three axes from the origin with ticks.
 */
export function buildGuides(bounds: Bounds, layout: Layout, unit: SpeedUnit, asymmetric = false): Guides {
  const maxAngle = asymmetric ? 360 : 180;
  const factor = SPEED_FACTOR[unit];
  const symbol = SPEED_SYMBOL[unit];
  const segments: number[] = [];
  const labels: GuideLabel[] = [];
  const speed = (knots: number) => `${Number((knots * factor).toFixed(1))} ${symbol}`;
  const line = (a: [number, number, number], b: [number, number, number]) => segments.push(...a, ...b);
  const maxTws = Math.max(1, layout === "tower" ? bounds.max[2] : bounds.max[1]);

  if (layout === "tower") {
    const maxBsp = Math.max(1, ...[0, 1].flatMap((a) => [Math.abs(bounds.min[a]!), Math.abs(bounds.max[a]!)]));
    for (const r of ticks(maxBsp, 4).filter((r) => r > 0)) {
      for (let a = 0; a < maxAngle; a += 10) {
        line(place(a, 0, r, "tower"), place(a + 10, 0, r, "tower"));
      }
      labels.push({ at: place(90, 0, r, "tower"), text: speed(r) });
    }
    for (let a = 0; a <= (asymmetric ? maxAngle - 30 : maxAngle); a += 30) {
      line([0, 0, 0], place(a, 0, maxBsp, "tower"));
      labels.push({ at: place(a, 0, maxBsp * 1.08, "tower"), text: `${Math.min(a, 360 - a)}°` });
    }
    line([0, 0, 0], [0, 0, maxTws]);
    for (const w of ticks(maxTws, 5).filter((w) => w > 0)) {
      line([-0.3, 0, w], [0.3, 0, w]);
      labels.push({ at: [-0.6, 0, w], text: speed(w) });
    }
  } else {
    const maxBsp = Math.max(1, bounds.max[2]);
    const xEnd = maxAngle / CARTESIAN_TWA_SCALE;
    line([0, 0, 0], [xEnd, 0, 0]);
    line([0, 0, 0], [0, maxTws, 0]);
    line([0, 0, 0], [0, 0, maxBsp]);
    for (let a = 0; a <= maxAngle; a += 30) labels.push({ at: [a / CARTESIAN_TWA_SCALE, -0.8, 0], text: `${Math.min(a, 360 - a)}°` });
    for (const w of ticks(maxTws, 5).filter((w) => w > 0)) labels.push({ at: [-0.8, w, 0], text: speed(w) });
    for (const b of ticks(maxBsp, 4).filter((b) => b > 0)) labels.push({ at: [-0.8, 0, b], text: speed(b) });
  }
  return { segments: Float32Array.from(segments), labels };
}

// -------------------------------------------------------------- selection

/**
 * Stable names for selected dots that survive a refetch: a node by its
 * source and grid cell, a sample by its id. Numbers in maps and sets, not
 * strings: re-finding a selection runs over every dot on every edit, and at
 * 200,000 dots building a string per dot costs tens of milliseconds.
 */
export interface SelectionKeys {
  /** Source id → cells (TWA index | TWS index << 16). */
  nodes: Map<number, Set<number>>;
  samples: Set<number>;
}

export function emptyKeys(): SelectionKeys {
  return { nodes: new Map(), samples: new Set() };
}

/** The keys of some dots, by global index. */
export function keysOf(packet: ScenePacket, globals: readonly number[]): SelectionKeys {
  const keys = emptyKeys();
  const { nodes, samples } = packet;
  for (const g of globals) {
    if (g < nodes.count) {
      const id = packet.sources[nodes.source[g]!]!.id;
      let cells = keys.nodes.get(id);
      if (!cells) keys.nodes.set(id, (cells = new Set()));
      cells.add(nodes.cell[g]!);
    } else if (g - nodes.count < samples.count) {
      keys.samples.add(sampleId(samples.ids, g - nodes.count));
    }
  }
  return keys;
}

/** The global indices of `keys` in a (new) packet, in order; keys no longer there drop out. */
export function resolveKeys(packet: ScenePacket, keys: SelectionKeys): number[] {
  const out: number[] = [];
  const { nodes, samples } = packet;
  if (keys.nodes.size > 0) {
    const bySource = packet.sources.map((s) => keys.nodes.get(s.id));
    for (let k = 0; k < nodes.count; k++) if (bySource[nodes.source[k]!]?.has(nodes.cell[k]!)) out.push(k);
  }
  if (keys.samples.size > 0) {
    for (let k = 0; k < samples.count; k++) if (keys.samples.has(sampleId(samples.ids, k))) out.push(nodes.count + k);
  }
  return out;
}

/** The sample ids among some dots. */
export function sampleIdsOf(packet: ScenePacket, globals: readonly number[]): number[] {
  const out: number[] = [];
  for (const g of globals) {
    const k = g - packet.nodes.count;
    if (k >= 0 && k < packet.samples.count) out.push(sampleId(packet.samples.ids, k));
  }
  return out;
}

/**
 * The part of a selection that is drawn. A dot the toggles hide (samples
 * off, or a filtered sample while "show filtered" is off) must not be
 * counted or excluded by an action the person cannot see it take part in.
 */
export function drawnOnly(selection: readonly number[], refs: Uint32Array, total: number): number[] {
  if (selection.length === 0) return [];
  const drawn = new Uint8Array(total);
  for (let d = 0; d < refs.length; d++) drawn[refs[d]!] = 1;
  return selection.filter((g) => drawn[g] === 1);
}

/** What the selection panel shows (spec.md 10.3). Means are in knots and degrees. */
export interface SelectionSummary {
  count: number;
  meanTwa: number;
  meanTws: number;
  meanBsp: number;
  /** Per source, in source order: how many selected dots are its. */
  bySource: { sourceId: number; count: number }[];
  /** Selected dots currently excluded, and not. */
  excluded: number;
  included: number;
  /** Selected samples (for "show on map"). */
  samples: number;
}

export function summarise(packet: ScenePacket, selection: readonly number[]): SelectionSummary {
  const { nodes, samples } = packet;
  let twa = 0, tws = 0, bsp = 0, excluded = 0, sampleCount = 0, segment = 0;
  const perSource = new Array<number>(packet.sources.length).fill(0);
  for (const g of selection) {
    const isNode = g < nodes.count;
    const k = isNode ? g : g - nodes.count;
    const set = isNode ? nodes : samples;
    twa += set.points[k * 3]!;
    tws += set.points[k * 3 + 1]!;
    bsp += set.points[k * 3 + 2]!;
    perSource[set.source[k]!]! += 1;
    if (isNode && isSegmentNode(packet, k)) segment++;
    else if (set.flags[k]! & FLAG_EXCLUDED) excluded++;
    if (!isNode) sampleCount++;
  }
  const count = selection.length;
  return {
    count,
    meanTwa: count ? twa / count : NaN,
    meanTws: count ? tws / count : NaN,
    meanBsp: count ? bsp / count : NaN,
    bySource: perSource.flatMap((c, i) => (c > 0 ? [{ sourceId: packet.sources[i]!.id, count: c }] : [])),
    excluded,
    // A track's segment cells are edited, never excluded: its samples are.
    included: count - excluded - segment,
    samples: sampleCount,
  };
}

/** The selection as the exclude/include command takes it: polar nodes by grid place, samples by id. */
export function exclusionTargets(packet: ScenePacket, selection: readonly number[]): { nodes: PolarNodeRef[]; samples: number[] } {
  const nodes: PolarNodeRef[] = [];
  const samples: number[] = [];
  for (const g of selection) {
    if (g < packet.nodes.count) {
      if (isSegmentNode(packet, g)) continue;
      const cell = packet.nodes.cell[g]!;
      nodes.push({ source_id: packet.sources[packet.nodes.source[g]!]!.id, twa_index: cell & 0xffff, tws_index: cell >>> 16 });
    } else {
      samples.push(sampleId(packet.samples.ids, g - packet.nodes.count));
    }
  }
  return { nodes, samples };
}

/** Combines a new pick with the selection: Shift adds (toggling a dot already in it), otherwise replaces. */
export function combine(current: readonly number[], picked: readonly number[], add: boolean): number[] {
  if (!add) return [...new Set(picked)].sort((a, b) => a - b);
  const set = new Set(current);
  if (picked.length === 1 && set.has(picked[0]!)) set.delete(picked[0]!);
  else for (const g of picked) set.add(g);
  return [...set].sort((a, b) => a - b);
}
