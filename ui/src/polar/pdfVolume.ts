import { CARTESIAN_TWA_SCALE, place, type Layout } from "./geometry3d";

type Vec3 = readonly [number, number, number];

/** One continuous conditional PDF, with bounds in degrees and knots. */
export interface PdfBox {
  windCell: number;
  /** -1 in a single view; otherwise the split-wave copy. */
  copy: number;
  angle: readonly [number, number];
  wind: readonly [number, number];
  speed: readonly [number, number];
  /** Mean, standard deviation and probability mass on the supported SOG domain. */
  gaussian: readonly [number, number, number];
  opacity: number;
}

export interface PdfVolumeInput {
  boxes: readonly PdfBox[];
  maxDensity: number;
}

const RAD = Math.PI / 180;

/** Membership in the displayed volume, including its circular angular bounds. */
export function pdfBoxContains(box: PdfBox, twa: number, tws: number, speed: number): boolean {
  const epsilon = 1e-5; // Packed point coordinates are float32.
  const offset = ((twa - box.angle[0]) % 360 + 360) % 360;
  const angle = box.angle[0] + (offset > 360 - epsilon ? 0 : offset);
  return angle <= box.angle[1] + epsilon
    && tws >= box.wind[0] - epsilon && tws <= box.wind[1] + epsilon
    && speed >= box.speed[0] - epsilon && speed <= box.speed[1] + epsilon;
}

/** Exact world bounds, including extrema between the angular endpoints. */
export function pdfBoxBounds(box: PdfBox, layout: Layout): { min: Vec3; max: Vec3 } {
  if (layout === "cartesian") return {
    min: [box.angle[0] / CARTESIAN_TWA_SCALE, box.wind[0], box.speed[0]],
    max: [box.angle[1] / CARTESIAN_TWA_SCALE, box.wind[1], box.speed[1]],
  };
  const angles = [...box.angle];
  for (let a = Math.ceil(box.angle[0] / 90) * 90; a < box.angle[1]; a += 90) angles.push(a);
  let x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity;
  for (const a of angles) for (const r of box.speed) {
    const x = r * Math.sin(a * RAD), y = r * Math.cos(a * RAD);
    x0 = Math.min(x0, x); x1 = Math.max(x1, x);
    y0 = Math.min(y0, y); y1 = Math.max(y1, y);
  }
  return { min: [x0, y0, box.wind[0]], max: [x1, y1, box.wind[1]] };
}

/** The actual curved box edges, without internal tessellation or diagonals. */
export function pdfBoxEdges(box: PdfBox, layout: Layout): Float32Array {
  const out: number[] = [];
  const line = (a: Vec3, b: Vec3) => out.push(...a, ...b);
  const steps = layout === "tower" ? Math.max(1, Math.ceil((box.angle[1] - box.angle[0]) / 2)) : 1;
  for (const w of box.wind) for (const b of box.speed) {
    for (let i = 0; i < steps; i++) {
      const a0 = box.angle[0] + (box.angle[1] - box.angle[0]) * i / steps;
      const a1 = box.angle[0] + (box.angle[1] - box.angle[0]) * (i + 1) / steps;
      line(place(a0, w, b, layout), place(a1, w, b, layout));
    }
  }
  for (const a of box.angle) {
    for (const w of box.wind) line(place(a, w, box.speed[0], layout), place(a, w, box.speed[1], layout));
    for (const b of box.speed) line(place(a, box.wind[0], b, layout), place(a, box.wind[1], b, layout));
  }
  return Float32Array.from(out);
}

/**
 * Intersects the same bounded field as the GPU. An annular sector can have
 * two intervals when the ray crosses its empty central hole.
 */
export function intersectPdfBox(origin: Vec3, direction: Vec3, box: PdfBox, layout: Layout): [number, number][] {
  let entry = 0, exit = Infinity;
  // Inside a half-space when value + slope * t >= 0.
  const clip = (value: number, slope: number): boolean => {
    if (Math.abs(slope) < 1e-10) return value >= -1e-8;
    const at = -value / slope;
    if (slope > 0) entry = Math.max(entry, at);
    else exit = Math.min(exit, at);
    return exit > entry;
  };
  if (layout === "cartesian") {
    const { min, max } = pdfBoxBounds(box, layout);
    for (let axis = 0; axis < 3; axis++) {
      if (!clip(origin[axis]! - min[axis]!, direction[axis]!)
        || !clip(max[axis]! - origin[axis]!, -direction[axis]!)) return [];
    }
    return exit > entry ? [[entry, exit]] : [];
  }
  if (!clip(origin[2] - box.wind[0], direction[2]) || !clip(box.wind[1] - origin[2], -direction[2])) return [];
  const a0 = box.angle[0] * RAD, a1 = box.angle[1] * RAD;
  if (!clip(Math.cos(a0) * origin[0] - Math.sin(a0) * origin[1], Math.cos(a0) * direction[0] - Math.sin(a0) * direction[1])
    || !clip(-Math.cos(a1) * origin[0] + Math.sin(a1) * origin[1], -Math.cos(a1) * direction[0] + Math.sin(a1) * direction[1])) return [];
  const aa = direction[0] ** 2 + direction[1] ** 2;
  const bb = origin[0] * direction[0] + origin[1] * direction[1];
  const rr = origin[0] ** 2 + origin[1] ** 2;
  if (aa < 1e-12) {
    return rr >= box.speed[0] ** 2 && rr <= box.speed[1] ** 2 && exit > entry ? [[entry, exit]] : [];
  }
  const outer = bb * bb - aa * (rr - box.speed[1] ** 2);
  if (outer <= 0) return [];
  const root = Math.sqrt(outer);
  entry = Math.max(entry, (-bb - root) / aa);
  exit = Math.min(exit, (-bb + root) / aa);
  if (exit <= entry) return [];
  const inner = bb * bb - aa * (rr - box.speed[0] ** 2);
  if (inner <= 0 || box.speed[0] === 0) return [[entry, exit]];
  const holeRoot = Math.sqrt(inner);
  const holeStart = (-bb - holeRoot) / aa, holeEnd = (-bb + holeRoot) / aa;
  const intervals: [number, number][] = [];
  if (Math.min(exit, holeStart) > entry) intervals.push([entry, Math.min(exit, holeStart)]);
  if (exit > Math.max(entry, holeEnd)) intervals.push([Math.max(entry, holeEnd), exit]);
  return intervals;
}

/** Only eight proxy vertices per cell. The shader integrates its full interior. */
export function pdfProxyGeometry(boxes: readonly PdfBox[], layout: Layout) {
  const positions: number[] = [], lower: number[] = [], upper: number[] = [], parameters: number[] = [];
  const opacity: number[] = [], copies: number[] = [], windCells: number[] = [], indices: number[] = [];
  const faces = [0, 2, 1, 0, 3, 2, 4, 5, 6, 4, 6, 7, 0, 1, 5, 0, 5, 4, 1, 2, 6, 1, 6, 5, 2, 3, 7, 2, 7, 6, 3, 0, 4, 3, 4, 7];
  for (const box of boxes) {
    const { min, max } = pdfBoxBounds(box, layout);
    const base = positions.length / 3;
    for (const [x, y, z] of [[0, 0, 0], [1, 0, 0], [1, 1, 0], [0, 1, 0], [0, 0, 1], [1, 0, 1], [1, 1, 1], [0, 1, 1]]) {
      positions.push(x ? max[0] : min[0], y ? max[1] : min[1], z ? max[2] : min[2]);
      lower.push(box.angle[0] * RAD, box.wind[0], box.speed[0]);
      upper.push(box.angle[1] * RAD, box.wind[1], box.speed[1]);
      parameters.push(...box.gaussian);
      opacity.push(box.opacity); copies.push(box.copy); windCells.push(box.windCell);
    }
    for (const index of faces) indices.push(base + index);
  }
  return {
    positions: Float32Array.from(positions), lower: Float32Array.from(lower), upper: Float32Array.from(upper),
    parameters: Float32Array.from(parameters), opacity: Float32Array.from(opacity), copies: Float32Array.from(copies),
    windCells: Float32Array.from(windCells), indices: Uint32Array.from(indices),
  };
}
