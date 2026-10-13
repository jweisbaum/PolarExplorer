import type { PolarPdfResult } from "../generated/PolarPdfResult";

/** Display interpolation only: every Gaussian was fitted by Rust's blend rule. */
export interface PdfHeat {
  maxBsp: number;
  maxDensity: number;
  densityAt: (angle: number, bsp: number) => number;
}

const SPEED_STEP = 0.1;
const SPEED_COUNT = 601;

/** Project the shared conditional PDFs through the selected wind slice/range.
 * A range averages densities, retaining the spread between wind speeds instead
 * of refitting dots with a second, unrelated weighting rule.
 */
export function pdfHeat(
  result: PolarPdfResult | null,
  slice: number | null,
  range: readonly [number, number] | null,
): PdfHeat | null {
  if (!result?.cells.length || !result.tws.length) return null;
  const { twa, tws, cells } = result;
  const windWeights = new Float64Array(tws.length);
  const interval = range ?? (slice === null ? [tws[0]!, tws[tws.length - 1]!] : null);
  const at = interval && interval[0] === interval[1] ? interval[0]! : slice;
  if (interval && interval[0]! < interval[1]!) {
    // Integrate the linear wind interpolation exactly. This makes a shrinking
    // range converge to the single-speed view, including irregular wind grids.
    if (tws.length === 1 && interval[0]! <= tws[0]! && interval[1]! >= tws[0]!) windWeights[0] = 1;
    for (let j = 0; j + 1 < tws.length; j++) {
      const a = tws[j]!, b = tws[j + 1]!;
      const lo = Math.max(interval[0]!, a), hi = Math.min(interval[1]!, b);
      if (hi <= lo) continue;
      const mix = ((lo + hi) / 2 - a) / (b - a);
      windWeights[j] = windWeights[j]! + (hi - lo) * (1 - mix);
      windWeights[j + 1] = windWeights[j + 1]! + (hi - lo) * mix;
    }
  } else if (at !== null) {
    if (at < tws[0]! || at > tws[tws.length - 1]!) return null;
    const hi = tws.findIndex((value) => value >= at);
    if (tws[hi] === at || hi === 0) windWeights[hi] = 1;
    else {
      const mix = (at - tws[hi - 1]!) / (tws[hi]! - tws[hi - 1]!);
      windWeights[hi - 1] = 1 - mix;
      windWeights[hi] = mix;
    }
  }

  const rows = twa.map(() => new Float32Array(SPEED_COUNT));
  const totals = new Float64Array(twa.length);
  let maxBsp = 0;
  for (const cell of cells) {
    const i = Math.floor(cell.windCell / tws.length), j = cell.windCell % tws.length;
    const weight = windWeights[j]!;
    if (weight <= 0) continue;
    totals[i] = totals[i]! + weight;
    maxBsp = Math.max(maxBsp, Math.min(60, cell.mean + 4 * cell.stddev));
    const row = rows[i]!;
    const scale = weight / (cell.stddev * Math.sqrt(2 * Math.PI) * cell.mass);
    for (let k = 0; k < SPEED_COUNT; k++) {
      const z = (k * SPEED_STEP - cell.mean) / cell.stddev;
      row[k] = row[k]! + scale * Math.exp(-0.5 * z * z);
    }
  }
  let maxDensity = 0;
  rows.forEach((row, i) => {
    if (totals[i]! <= 0) return;
    for (let k = 0; k < row.length; k++) {
      row[k] = row[k]! / totals[i]!;
      maxDensity = Math.max(maxDensity, row[k]!);
    }
  });
  if (maxDensity <= 0) return null;
  const maxAngle = twa[twa.length - 1]!;
  const densityAt = (angle: number, bsp: number) => {
    if (bsp < 0 || bsp > 60 || angle < twa[0]! || angle > maxAngle) return 0;
    let left = 0, right = twa.length - 1;
    while (left < right) {
      const middle = (left + right) >>> 1;
      if (twa[middle]! < angle) left = middle + 1;
      else right = middle;
    }
    const hi = left, lo = Math.max(0, hi - 1);
    const mix = hi === lo ? 0 : (angle - twa[lo]!) / (twa[hi]! - twa[lo]!);
    const speed = bsp / SPEED_STEP, loSpeed = Math.floor(speed), hiSpeed = Math.min(SPEED_COUNT - 1, loSpeed + 1);
    const speedMix = speed - loSpeed;
    const read = (row: Float32Array) => row[loSpeed]! * (1 - speedMix) + row[hiSpeed]! * speedMix;
    return read(rows[lo]!) * (1 - mix) + read(rows[hi]!) * mix;
  };
  return { maxBsp, maxDensity, densityAt };
}
