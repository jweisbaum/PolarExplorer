/**
 * What the Compare stage shows, worked out from the packed comparison
 * (spec.md 11) without three.js or React, so it can be tested in Node: the
 * operands' names and colours, the A, B and difference surfaces, the
 * summary's regions and the heat map's cells. Nothing here computes a
 * difference: every Δ comes from Rust.
 */
import type { CompareOperand } from "../generated/CompareOperand";
import type { ProjectSummary } from "../generated/ProjectSummary";
import { place, type Layout, type PolarGrid } from "../polar/geometry3d";
import type { SurfaceInput } from "../polar/scene3d";
import type { Bounds } from "../polar/view3d";
import { CLASS_A_ONLY, CLASS_B_ONLY, CLASS_BOTH, type ComparePacket } from "./comparePacket";
import { diverging, POLES, scaleHalfWidth, type Scheme } from "./diverging";

/** An operand as the pickers and the legend name it. */
export interface OperandInfo {
  /** Its colour, `#rrggbb`: the source's, or the Blend entry's. */
  colour: string;
  /** The source's label; null for the blend (the view says "Blend"). */
  label: string | null;
  /** `orc`, `polar_file`, `track` or `blend`. */
  kind: string;
  /** A hidden source: not in the blend or the plots, but still comparable. */
  hidden: boolean;
}

export function operandInfo(project: ProjectSummary, operand: CompareOperand): OperandInfo {
  if (operand.kind === "blend") return { colour: project.blend.colour, label: null, kind: "blend", hidden: false };
  const source = project.sources.find((s) => s.id === operand.source_id);
  return {
    colour: source?.colour ?? "#888888",
    label: source?.label ?? null,
    kind: source?.kind ?? "polar_file",
    hidden: source ? !source.visible : false,
  };
}

/** A picker's value for an operand, and back. */
export function operandKey(operand: CompareOperand): string {
  return operand.kind === "blend" ? "blend" : `${operand.kind}:${operand.source_id}`;
}

export function parseOperandKey(key: string): CompareOperand {
  if (key === "blend") return { kind: "blend" };
  const [kind, id] = key.split(":");
  return { kind: kind === "segment" ? "segment" : "polar", source_id: Number(id) };
}

/** Δ in the chosen unit: knots, or percent of B. */
export function deltaOf(packet: ComparePacket, percent: boolean): Float32Array {
  return percent ? packet.deltaPct : packet.deltaKn;
}

/**
 * A grid of one packet array for the scene: the packet's TWA-major arrays
 * as they are, NaN for no value. Flat typed arrays, not rows of numbers:
 * nothing is copied (M17a).
 */
export function gridOf(packet: ComparePacket, values: Float32Array): PolarGrid {
  return { twa: packet.twa, tws: packet.tws, bsp: values };
}

/** sRGB 0–1 → linear, which three.js expects of vertex colours. */
function linear(c: number): number {
  return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
}

function hexChannels(colour: string): [number, number, number] {
  const n = Number.parseInt(colour.slice(1), 16);
  return [((n >> 16) & 255) / 255, ((n >> 8) & 255) / 255, (n & 255) / 255];
}

/** Steps of the colour tables the views read from, per side of zero. */
const LUT_HALF = 256;
const linearLuts = new Map<Scheme, Float32Array>();
/** The diverging scale in linear light, for vertex colours: `(2 · LUT_HALF + 1)` RGB triples from −1 to +1. */
function linearLut(scheme: Scheme): Float32Array {
  let table = linearLuts.get(scheme);
  if (!table) {
    table = new Float32Array((2 * LUT_HALF + 1) * 3);
    for (let k = 0; k <= 2 * LUT_HALF; k++) table.set(diverging(k / LUT_HALF - 1, scheme).map(linear), k * 3);
    linearLuts.set(scheme, table);
  }
  return table;
}

/** Which surfaces are drawn. */
export interface CompareToggles {
  a: boolean;
  b: boolean;
  delta: boolean;
}

export const DEFAULT_COMPARE_TOGGLES: CompareToggles = { a: true, b: true, delta: true };

/**
 * The difference surface: midway between A and B where both have a value,
 * coloured by Δ on the diverging scale; where only one has a value, at that
 * value in neutral grey with a hatch (spec.md 11). The 0° row is no
 * comparison and is left out.
 */
export function differenceSurface(packet: ComparePacket, percent: boolean, scheme: Scheme): SurfaceInput {
  const ni = packet.twa.length, nj = packet.tws.length;
  const heights = new Float32Array(ni * nj).fill(Number.NaN);
  const delta = deltaOf(packet, percent);
  const stats = percent ? packet.pct : packet.kn;
  const half = scaleHalfWidth(stats.min, stats.max);
  const colours = new Float32Array(ni * nj * 3);
  const hatched = new Uint8Array(ni * nj);
  const table = linearLut(scheme);
  const single = hexChannels(POLES[scheme].single).map(linear);
  for (let i = 0; i < ni; i++) {
    for (let j = 0; j < nj; j++) {
      const k = i * nj + j;
      const node = j * ni + i;
      const cls = packet.cls[k]!;
      const at = node * 3;
      if (cls === CLASS_BOTH) {
        heights[k] = (packet.a[k]! + packet.b[k]!) / 2;
        const d = delta[k]!;
        if (Number.isFinite(d)) {
          const e = Math.round((Math.max(-1, Math.min(1, d / half)) + 1) * LUT_HALF) * 3;
          colours[at] = table[e]!; colours[at + 1] = table[e + 1]!; colours[at + 2] = table[e + 2]!;
        } else {
          // Both have a value but Δ % has none (B under 0.1 kn): not
          // comparable in %, plain grey (no hatch).
          colours[at] = single[0]!; colours[at + 1] = single[1]!; colours[at + 2] = single[2]!;
        }
      } else if (cls === CLASS_A_ONLY || cls === CLASS_B_ONLY) {
        heights[k] = cls === CLASS_A_ONLY ? packet.a[k]! : packet.b[k]!;
        colours[at] = single[0]!; colours[at + 1] = single[1]!; colours[at + 2] = single[2]!;
        hatched[node] = 1;
      }
    }
  }
  return {
    grid: gridOf(packet, heights),
    color: POLES[scheme].mid,
    opaque: true,
    pickable: true,
    vertexColors: colours,
    hatched,
    hatchColor: POLES[scheme].hatch,
    lineColor: POLES[scheme].hatch,
  };
}

/**
 * A cross at every node only one operand covers, in the hatch colour: the
 * hatch marks quads whose four corners are all one-only, so a lone cell (or
 * a strip one cell wide) needs a mark of its own (spec.md 11).
 */
export function singleMarkers(packet: ComparePacket, scheme: Scheme): { points: Float32Array; colors: Float32Array; cells: Uint32Array; count: number } {
  const nj = packet.tws.length;
  const found: number[] = [];
  const cells: number[] = [];
  for (let k = 0; k < packet.cls.length; k++) {
    const cls = packet.cls[k]!;
    if (cls !== CLASS_A_ONLY && cls !== CLASS_B_ONLY) continue;
    const i = Math.floor(k / nj), j = k % nj;
    found.push(packet.twa[i]!, packet.tws[j]!, cls === CLASS_A_ONLY ? packet.a[k]! : packet.b[k]!);
    cells.push(k);
  }
  const count = found.length / 3;
  const colour = hexChannels(POLES[scheme].hatch);
  const colors = new Float32Array(count * 3);
  for (let n = 0; n < count; n++) colors.set(colour, n * 3);
  return { points: Float32Array.from(found), colors, cells: Uint32Array.from(cells), count };
}

/** Every surface to draw: A and B translucent in their colours, the difference opaque. */
export function compareSurfaces(packet: ComparePacket, toggles: CompareToggles, colours: { a: string; b: string },
  percent: boolean, scheme: Scheme): SurfaceInput[] {
  const out: SurfaceInput[] = [];
  if (toggles.a) out.push({ grid: gridOf(packet, packet.a), color: colours.a, opacity: 0.16, pickable: true });
  if (toggles.b) out.push({ grid: gridOf(packet, packet.b), color: colours.b, opacity: 0.16, pickable: true });
  if (toggles.delta) out.push(differenceSurface(packet, percent, scheme));
  return out;
}

/** The model-space box round both operands, with the origin; a polar-sized default when empty. */
export function compareBounds(packet: ComparePacket, layout: Layout): Bounds {
  const min: [number, number, number] = [Infinity, Infinity, Infinity];
  const max: [number, number, number] = [-Infinity, -Infinity, -Infinity];
  const add = (twa: number, tws: number, bsp: number) => {
    if (![twa, tws, bsp].every(Number.isFinite)) return;
    const p = place(twa, tws, bsp, layout);
    for (let a = 0; a < 3; a++) { min[a] = Math.min(min[a]!, p[a]!); max[a] = Math.max(max[a]!, p[a]!); }
  };
  const nj = packet.tws.length;
  for (const values of [packet.a, packet.b]) {
    for (let k = 0; k < values.length; k++) add(packet.twa[Math.floor(k / nj)]!, packet.tws[k % nj]!, values[k]!);
  }
  if (!Number.isFinite(min[0])) {
    add(0, 0, 0);
    add(180, 20, 10);
    add(90, 0, 10);
  }
  add(0, 0, 0);
  return { min, max };
}

/** One TWS row of the summary's regions (spec.md 11): TWA spans in degrees. */
export interface RegionRow {
  tws: number;
  a: [number, number][];
  b: [number, number][];
}

/** The regions where A or B is faster, grouped by wind speed, in axis order. */
export function regionRows(packet: ComparePacket): RegionRow[] {
  const rows = new Map<number, RegionRow>();
  for (const region of packet.regions) {
    let row = rows.get(region.tws);
    if (!row) rows.set(region.tws, (row = { tws: packet.tws[region.tws]!, a: [], b: [] }));
    row[region.faster].push([packet.twa[region.firstTwa]!, packet.twa[region.lastTwa]!]);
  }
  return [...rows.entries()].sort((x, y) => x[0] - y[0]).map(([, row]) => row);
}

/** A TWA span as text: one angle, or first–last. */
export function spanText([first, last]: readonly [number, number]): string {
  const f = (v: number) => `${Number(v.toFixed(2))}°`;
  return first === last ? f(first) : `${f(first)}–${f(last)}`;
}

/** One comparison cell, shared by the heat-map readout and 3D tooltip. */
export interface HeatCell {
  /** The cell in the packet. */
  i: number;
  j: number;
  /** Degrees and knots. */
  twa: number;
  tws: number;
  /** The packet's class of the cell. */
  cls: number;
  a: number;
  b: number;
  /** Δ in the unit shown; NaN where not compared, or not comparable in %. */
  delta: number;
}

/** Read a grid cell verbatim from Rust's comparison, never interpolating Δ in the view. */
export function compareCell(packet: ComparePacket, i: number, j: number, percent: boolean): HeatCell | null {
  if (!Number.isInteger(i) || !Number.isInteger(j) || i < 0 || i >= packet.twa.length || j < 0 || j >= packet.tws.length) return null;
  const k = i * packet.tws.length + j;
  const cls = packet.cls[k]!;
  return {
    i, j, twa: packet.twa[i]!, tws: packet.tws[j]!, cls, a: packet.a[k]!, b: packet.b[k]!,
    delta: cls === CLASS_BOTH ? deltaOf(packet, percent)[k]! : Number.NaN,
  };
}

/**
 * The heat map as images, one pixel per cell (TWA rows × TWS columns, the
 * 0° row left out: it is never compared): the fills, and a mask of the
 * cells only one operand covers, which the view hatches. Flat typed arrays
 * rather than an element or an object per cell: a 512 × 512 output grid is
 * 262,144 cells (spec.md 12.2, 13).
 */
export interface HeatImage {
  rows: number;
  cols: number;
  /** The packet's TWA index of each row. */
  rowIndex: Uint32Array;
  /** RGBA per cell: a compared cell's colour; transparent otherwise. */
  rgba: Uint8ClampedArray<ArrayBuffer>;
  /** RGBA per cell: opaque where only one operand has a value. */
  single: Uint8ClampedArray<ArrayBuffer>;
}

const luts = new Map<Scheme, Uint8ClampedArray>();
function lut(scheme: Scheme): Uint8ClampedArray {
  let table = luts.get(scheme);
  if (!table) {
    table = new Uint8ClampedArray((2 * LUT_HALF + 1) * 3);
    for (let k = 0; k <= 2 * LUT_HALF; k++) {
      const rgb = diverging(k / LUT_HALF - 1, scheme);
      table[k * 3] = Math.round(rgb[0] * 255);
      table[k * 3 + 1] = Math.round(rgb[1] * 255);
      table[k * 3 + 2] = Math.round(rgb[2] * 255);
    }
    luts.set(scheme, table);
  }
  return table;
}

export function heatImage(packet: ComparePacket, percent: boolean, scheme: Scheme): HeatImage {
  const ni = packet.twa.length, nj = packet.tws.length;
  const kept: number[] = [];
  for (let i = 0; i < ni; i++) if (Math.abs(packet.twa[i]!) >= 1e-9) kept.push(i);
  const rows = kept.length, cols = nj;
  const rgba = new Uint8ClampedArray(rows * cols * 4);
  const single = new Uint8ClampedArray(rows * cols * 4);
  const delta = deltaOf(packet, percent);
  const stats = percent ? packet.pct : packet.kn;
  const half = scaleHalfWidth(stats.min, stats.max);
  const table = lut(scheme);
  const plain = hexChannels(POLES[scheme].single).map((c) => Math.round(c * 255));
  for (let r = 0; r < rows; r++) {
    const i = kept[r]!;
    for (let j = 0; j < cols; j++) {
      const k = i * nj + j;
      const p = (r * cols + j) * 4;
      const cls = packet.cls[k]!;
      if (cls === CLASS_BOTH) {
        const d = delta[k]!;
        if (Number.isFinite(d)) {
          const t = Math.max(-1, Math.min(1, d / half));
          const e = Math.round((t + 1) * LUT_HALF) * 3;
          rgba[p] = table[e]!; rgba[p + 1] = table[e + 1]!; rgba[p + 2] = table[e + 2]!;
        } else {
          // Compared, but not comparable in % (B under 0.1 kn): plain grey.
          rgba[p] = plain[0]!; rgba[p + 1] = plain[1]!; rgba[p + 2] = plain[2]!;
        }
        rgba[p + 3] = 255;
      } else if (cls === CLASS_A_ONLY || cls === CLASS_B_ONLY) {
        single[p + 3] = 255;
      }
    }
  }
  return { rows, cols, rowIndex: Uint32Array.from(kept), rgba, single };
}

/** Where the heat map's cells are drawn, CSS pixels. */
export interface HeatLayout {
  left: number;
  top: number;
  cellWidth: number;
  cellHeight: number;
}

/** The layout of a heat map `width` pixels wide: labels at the left and top, cells at most 14 px tall. */
export function heatLayout(image: Pick<HeatImage, "rows" | "cols">, width: number): HeatLayout {
  const left = 34, top = 16;
  const cellWidth = Math.max(0.25, (width - left) / Math.max(1, image.cols));
  const cellHeight = Math.min(14, Math.max(0.25, 480 / Math.max(1, image.rows)));
  return { left, top, cellWidth, cellHeight };
}

/** The cell under a point of the heat map (CSS pixels from its top left), or null. */
export function heatCellAt(packet: ComparePacket, image: HeatImage, layout: HeatLayout, percent: boolean,
  x: number, y: number): HeatCell | null {
  const c = Math.floor((x - layout.left) / layout.cellWidth);
  const r = Math.floor((y - layout.top) / layout.cellHeight);
  if (!(c >= 0 && c < image.cols && r >= 0 && r < image.rows)) return null;
  return compareCell(packet, image.rowIndex[r]!, c, percent);
}

/** Every `step`-th index from 0, so labels `size` pixels apart never overlap at `spacing` pixels per index. */
export function labelStep(spacing: number, size: number): number {
  return Math.max(1, Math.ceil(size / Math.max(1e-6, spacing)));
}
