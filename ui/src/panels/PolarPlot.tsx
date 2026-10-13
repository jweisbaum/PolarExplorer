import { useBoatApi, useBoatReadOnly } from "../boats/context";
import { useCallback, useEffect, useMemo, useRef, useState, type CSSProperties, type MouseEvent, type PointerEvent as ReactPointerEvent } from "react";

import { needsOutline } from "../colourContrast";
import { registerCapture } from "../mcp/capture";
import DayBandLegend from "../DayBandLegend";
import { reportFailure } from "../errors";
import type { BlendCell } from "../generated/BlendCell";
import type { PolarCurve } from "../generated/PolarCurve";
import type { PolarPdfResult } from "../generated/PolarPdfResult";
import { pdfHeat, type PdfHeat } from "../polar/pdfHeat";
import type { PolarPlotResult } from "../generated/PolarPlotResult";
import type { ProjectSummary } from "../generated/ProjectSummary";
import type { SpeedUnit } from "../generated/SpeedUnit";
import { useT } from "../i18n";

import WaveRangeControls from "../polar/WaveRangeControls";
import { useLiveEdit } from "./useLiveEdit";
import { DEFAULT_UNITS } from "./filterUnits";
import type { Units } from "../generated/Units";
import { lassoSelect } from "../polar/geometry3d";
import BlendCellTooltip from "../polar/BlendCellTooltip";
import { ramp, SPEED_FACTOR, SPEED_SYMBOL } from "../polar/view3d";
import { useSampleSelection, useBoatSelection } from "../selection";
import { onThemeChange } from "../settings/themes";
import { DOT_EXCLUDED, DOT_FILTERED, dotSampleId, emptyDots, withoutExcluded, type DotPacket } from "./dotPacket";
import {
  ANGLE_TICKS, FULL_ANGLE_TICKS, axisLabels, crossings, dotFill, fitLayout, maxBoatSpeed, measureBetween, nearestCrossing, nearestPoint,
  project as projectPoint, speedTicks, unproject,
  type Crossing, type DotColourMode, type Hover, type PolarPoint, type SourceStyle,
} from "./plotGeometry";

/** A point within this many pixels of the pointer counts as hovered. */
const HOVER_DISTANCE_PX = 16;
/** How close a hovered blend point must be to an output-grid value to be that cell's, degrees or knots. */
const ON_GRID = 1e-6;
/** A click this close to a curve on the measuring spoke pins the curve's own value, pixels. */
const PIN_SNAP_PX = 8;
/** Beyond this many dots each is a small square rather than a circle: far cheaper to fill. */
const MANY_DOTS = 20_000;
/** Draw the shared weighted Gaussian field as a smooth translucent heatmap. */
function drawPdfHeat(ctx: CanvasRenderingContext2D, width: number, height: number, dpr: number,
  layout: ReturnType<typeof fitLayout>, maxBsp: number, asymmetric: boolean, heat: PdfHeat | null) {
  if (!heat || !(layout.scale > 0)) return;
  const { maxDensity, densityAt } = heat;

  // Rasterize at device resolution, then composite the transparent heat over
  // the polar grid. This avoids visible polygon seams in a 2D view.
  const rasterScale = Math.min(dpr, 1.5, 1200 / Math.max(width, height));
  const pixelWidth = Math.max(1, Math.round(width * rasterScale));
  const pixelHeight = Math.max(1, Math.round(height * rasterScale));
  const image = new ImageData(pixelWidth, pixelHeight);
  for (let py = 0; py < pixelHeight; py++) {
    const y = (py + 0.5) / rasterScale;
    for (let px = 0; px < pixelWidth; px++) {
      const x = (px + 0.5) / rasterScale;
      const dx = x - layout.centerX, dy = layout.centerY - y;
      if (!asymmetric && dx < 0) continue;
      const bsp = Math.hypot(dx, dy) / layout.scale;
      if (bsp > maxBsp || bsp > 60) continue;
      let angle = Math.atan2(dx, dy) * 180 / Math.PI;
      if (angle < 0) angle += 360;
      const density = densityAt(angle, bsp);
      if (density < maxDensity * 0.005) continue;
      const [r, g, b] = ramp(density / maxDensity);
      const alpha = Math.min(0.82, 0.76 * Math.sqrt(density / maxDensity));
      const offset = (py * pixelWidth + px) * 4;
      image.data[offset] = Math.round(r * 255);
      image.data[offset + 1] = Math.round(g * 255);
      image.data[offset + 2] = Math.round(b * 255);
      image.data[offset + 3] = Math.round(alpha * 255);
    }
  }
  const raster = document.createElement("canvas");
  raster.width = pixelWidth;
  raster.height = pixelHeight;
  raster.getContext("2d")?.putImageData(image, 0, 0);
  ctx.drawImage(raster, 0, 0, width, height);
}

/** Draws one curve's polyline; empty curves (no points in range) draw nothing. */
function strokeCurve(ctx: CanvasRenderingContext2D, curve: PolarCurve, layout: ReturnType<typeof fitLayout>, width: number) {
  if (curve.points.length === 0) return;
  ctx.strokeStyle = curve.colour;
  ctx.lineWidth = width;
  ctx.beginPath();
  curve.points.forEach((point, index) => {
    const { x, y } = projectPoint(point.twa, point.bsp, layout);
    if (index === 0) ctx.moveTo(x, y);
    else ctx.lineTo(x, y);
  });
  ctx.stroke();
}

/**
 * The largest boat speed the plot must hold: every source curve, every dot
 * and the blend. The drawing and the hover hit-test both size the fan with
 * it, so a hovered point is where it is drawn (M6 carry).
 */
export function plotMaxBsp(result: PolarPlotResult | null, dots: DotPacket): number {
  if (!result) return maxBoatSpeed([], dots);
  return Math.max(maxBoatSpeed(result.curves, dots), maxBoatSpeed(result.blend, null));
}

/** The blend's curves with their label in the interface language. */
function blendCurves(result: PolarPlotResult | null, t: (key: string) => string): PolarCurve[] {
  return (result?.blend ?? []).map((curve) => ({ ...curve, label: t(curve.label) }));
}

/**
 * A speed in the display unit, for text: at most `digits` decimals, trailing
 * zeros dropped. Everything the plot is given is in knots; this is the one
 * place its text leaves them.
 */
export function displaySpeed(knots: number, unit: SpeedUnit, digits: number): string {
  return String(Number((knots * SPEED_FACTOR[unit]).toFixed(digits)));
}

/** What the Measure tool has on the plot (spec.md 9.2): all in knots and degrees. */
interface Measuring {
  /** Where the pointer is. */
  cursor: PolarPoint;
  /** Every curve's value on the pointer's spoke, fastest first. */
  crossings: Crossing[];
  /** The pinned point A, if any. */
  pin: PolarPoint | null;
}

/** A signed number for a difference: "+0.70", "−0.60" (a true minus sign), and a bare "0.00" for none. */
export function signed(value: number, digits: number): string {
  const text = Math.abs(value).toFixed(digits);
  if (Number(text) === 0) return text;
  return `${value > 0 ? "+" : "−"}${text}`;
}

/**
 * Draws the plot: radial BSP rings (round numbers in the display unit) and
 * angular TWA spokes, every curve in its source colour, the blend thicker,
 * sample dots in their track's colour or their band's of the local solar
 * day (filtered ones dimmed, excluded ones hollow, selected ones ringed),
 * and the hovered point.
 */
function draw(canvas: HTMLCanvasElement, result: PolarPlotResult | null, dots: DotPacket, hover: Hover | null,
  colours: ReadonlyMap<number, SourceStyle>, selected: ReadonlySet<number>, unit: SpeedUnit, asymmetric: boolean,
  dotColour: DotColourMode, measuring: Measuring | null, centred: boolean, pdfMode: boolean, heat: PdfHeat | null) {
  const ctx = canvas.getContext("2d");
  if (!ctx) return;
  const dpr = window.devicePixelRatio || 1;
  const width = canvas.clientWidth;
  const height = canvas.clientHeight;
  canvas.width = Math.max(1, Math.round(width * dpr));
  canvas.height = Math.max(1, Math.round(height * dpr));
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.clearRect(0, 0, width, height);
  if (width <= 1 || height <= 1) return;

  const style = getComputedStyle(canvas);
  const line = style.getPropertyValue("--border-subtle").trim() || "#3a506a";
  const muted = style.getPropertyValue("--muted").trim() || "#b3c9de";
  const accent = style.getPropertyValue("--accent").trim() || "#8fb8de";

  const curves = result?.curves ?? [];
  const blend = result?.blend ?? [];
  const observedMaxBsp = plotMaxBsp(result, dots);
  // Gaussian tails extend beyond the fastest observed value. Keep room for
  // those tails in PDF mode instead of clipping them at the plot's rim.
  const maxBsp = pdfMode ? (heat?.maxBsp ?? 0) : observedMaxBsp;
  if (maxBsp <= 0) return;
  const layout = fitLayout(width, height, maxBsp, 28, asymmetric, centred);

  ctx.font = "10px sans-serif";
  ctx.textBaseline = "top";
  ctx.textAlign = "left";
  ctx.lineWidth = 1;
  ctx.strokeStyle = line;
  ctx.fillStyle = muted;
  const factor = SPEED_FACTOR[unit];
  for (const tick of speedTicks(maxBsp, factor)) {
    const r = tick.knots * layout.scale;
    ctx.beginPath();
    ctx.arc(layout.centerX, layout.centerY, r, -Math.PI / 2, asymmetric ? Math.PI * 1.5 : Math.PI / 2);
    ctx.stroke();
  }
  for (const angle of asymmetric ? FULL_ANGLE_TICKS : ANGLE_TICKS) {
    const edge = projectPoint(angle, maxBsp, layout);
    ctx.beginPath();
    ctx.moveTo(layout.centerX, layout.centerY);
    ctx.lineTo(edge.x, edge.y);
    ctx.stroke();
  }
  if (pdfMode) drawPdfHeat(ctx, width, height, dpr, layout, maxBsp, asymmetric, heat);
  for (const label of axisLabels(layout, maxBsp, (text) => ctx.measureText(text).width, 10, factor, asymmetric)) {
    ctx.fillText(label.text, label.x, label.y);
  }

  if (!pdfMode) for (const curve of curves) strokeCurve(ctx, curve, layout, 1.5);
  // The blend is drawn thicker (spec.md 9.2), in the Blend entry's colour,
  // over a contrasting outline when that colour is lost on the background.
  const background = style.getPropertyValue("--surface").trim() || "#253447";
  const text = style.getPropertyValue("--text").trim() || "#d6e6f5";
  for (const curve of pdfMode ? [] : blend) {
    if (needsOutline(curve.colour, background)) strokeCurve(ctx, { ...curve, colour: text }, layout, 5);
    strokeCurve(ctx, curve, layout, 3);
  }

  const many = dots.count > MANY_DOTS;
  const dotColours = dots.sources.map((id) => colours.get(id)?.colour ?? line);
  for (let k = 0; !pdfMode && k < dots.count; k++) {
    const { x, y } = projectPoint(dots.points[k * 3]!, dots.points[k * 3 + 2]!, layout);
    const colour = dotFill(dots, k, dotColour, dotColours);
    const flags = dots.flags[k]!;
    ctx.globalAlpha = flags & DOT_FILTERED ? 0.3 : 0.85;
    if (many) {
      ctx.fillStyle = colour;
      if (flags & DOT_EXCLUDED) ctx.globalAlpha *= 0.4;
      ctx.fillRect(x - 1, y - 1, 2, 2);
      continue;
    }
    ctx.beginPath();
    ctx.arc(x, y, 2.5, 0, Math.PI * 2);
    if (flags & DOT_EXCLUDED) {
      ctx.strokeStyle = colour;
      ctx.lineWidth = 1;
      ctx.stroke();
    } else {
      ctx.fillStyle = colour;
      ctx.fill();
    }
  }
  ctx.globalAlpha = 1;
  if (!pdfMode && selected.size > 0) {
    ctx.strokeStyle = accent;
    ctx.lineWidth = 1.5;
    for (let k = 0; k < dots.count; k++) {
      if (!selected.has(dotSampleId(dots, k))) continue;
      const { x, y } = projectPoint(dots.points[k * 3]!, dots.points[k * 3 + 2]!, layout);
      ctx.beginPath();
      ctx.arc(x, y, 4.5, 0, Math.PI * 2);
      ctx.stroke();
    }
  }

  if (!pdfMode && hover) {
    ctx.strokeStyle = accent;
    ctx.lineWidth = 2;
    ctx.beginPath();
    ctx.arc(hover.x, hover.y, 5, 0, Math.PI * 2);
    ctx.stroke();
  }

  // The Measure tool: a spoke at the pointer's wind angle, a ring at its
  // boat speed, a mark where the spoke crosses each curve, and the pinned
  // point A with the line from it to the pointer.
  if (!pdfMode && measuring) {
    const { cursor, pin } = measuring;
    const outer = Math.max(maxBsp, cursor.bsp);
    ctx.strokeStyle = accent;
    ctx.lineWidth = 1;
    ctx.setLineDash([4, 3]);
    const edge = projectPoint(cursor.twa, outer, layout);
    ctx.beginPath();
    ctx.moveTo(layout.centerX, layout.centerY);
    ctx.lineTo(edge.x, edge.y);
    ctx.stroke();
    ctx.beginPath();
    ctx.arc(layout.centerX, layout.centerY, cursor.bsp * layout.scale, -Math.PI / 2, asymmetric ? Math.PI * 1.5 : Math.PI / 2);
    ctx.stroke();
    ctx.setLineDash([]);
    for (const crossing of measuring.crossings) {
      const at = projectPoint(cursor.twa, crossing.bsp, layout);
      ctx.beginPath();
      ctx.arc(at.x, at.y, 3.5, 0, Math.PI * 2);
      ctx.fillStyle = crossing.colour;
      ctx.fill();
      ctx.lineWidth = 1.5;
      ctx.strokeStyle = text;
      ctx.stroke();
    }
    if (pin) {
      const a = projectPoint(pin.twa, pin.bsp, layout);
      const b = projectPoint(cursor.twa, cursor.bsp, layout);
      ctx.strokeStyle = accent;
      ctx.lineWidth = 1.5;
      ctx.beginPath();
      ctx.moveTo(a.x, a.y);
      ctx.lineTo(b.x, b.y);
      ctx.stroke();
      ctx.beginPath();
      ctx.moveTo(a.x - 5, a.y);
      ctx.lineTo(a.x + 5, a.y);
      ctx.moveTo(a.x, a.y - 5);
      ctx.lineTo(a.x, a.y + 5);
      ctx.stroke();
      ctx.fillStyle = text;
      ctx.textAlign = "left";
      ctx.textBaseline = "bottom";
      ctx.fillText("A", a.x + 6, a.y - 3);
    }
  }
}

/**
 * The 2D polar plot (spec.md 9.2): boat speed against true wind angle, one
 * curve per visible polar source at the chosen true wind speed (or one per
 * source per wind speed it has, in "All"), sample dots within a band of the
 * slice, and the blend thicker. Rust computes every point; this only draws
 * and hit-tests what it is given.
 *
 * Refetches whenever the project changes — `revision` is bumped by every
 * command, including undo and redo, and by a source's colour, visibility,
 * weight or label — and whenever the chosen wind speed changes.
 */
export default function PolarPlot({ project, onProject, units = DEFAULT_UNITS, variant, unit = "kn", hidden = false }: {
  onProject?: (project: ProjectSummary) => void;
  units?: Units;
  project: ProjectSummary;
  /** Kept mounted while another stage shows: it answers no key then. */
  hidden?: boolean;
  /** The display speed unit (Settings); every value arrives and is kept in knots. */
  unit?: SpeedUnit;
  /**
   * `"stage"`: the 2D stage, filling the centre with the 0° axis down its
   * middle (asked 2026-10-02); `"panel"`: the compact form, the fan against
   * the left edge (kept for a narrow place; no panel shows it now).
   */
  variant: "panel" | "stage";
}) {
  const api = useBoatApi();
  const readOnly = useBoatReadOnly();
  const { selectSamples, clearSamples } = useBoatSelection();
  const [waveRanges, updateWaveRanges] = useLiveEdit(project.blend.wave_ranges,
    ranges => api.setWaveRanges(ranges).then(next => onProject?.(next)), project.id);
  const [waveMaxima, setWaveMaxima] = useState<readonly (number | null)[]>([null, null, null]);
  const [tool, setTool] = useState<"select" | "lasso" | "box">("select");
  const [selectionPath, setSelectionPath] = useState<number[] | null>(null);
  const selecting = useRef<{ pointer: number; x: number; y: number; path: number[]; tool: "lasso" | "box" } | null>(null);
  const suppressClick = useRef(false);
  const t = useT();
  const wrap = useRef<HTMLDivElement>(null);
  const canvas = useRef<HTMLCanvasElement>(null);
  const [result, setResult] = useState<PolarPlotResult | null>(null);
  const [received, setDots] = useState<DotPacket>(emptyDots);
  const [tws, setTws] = useState<number | null>(null);
  const [rangeEnabled, setRangeEnabled] = useState(false);
  const [twsRange, setTwsRange] = useState<[number, number]>([0, 30]);
  const rangeDrag = useRef<{ pointerId: number; mode: "move" | "low" | "high"; startX: number; low: number; high: number } | null>(null);
  const domainMin = result?.tws_min ?? null;
  const domainMax = result?.tws_max ?? null;
  const rangeLow = domainMin === null ? twsRange[0] : Math.min(domainMax!, Math.max(domainMin, twsRange[0]));
  const rangeHigh = domainMax === null ? twsRange[1] : Math.max(rangeLow, Math.min(domainMax, twsRange[1]));
  const [hover, setHover] = useState<Hover | null>(null);
  // The blend cell behind a hovered blend point (spec.md 9.2). `blendAsked`
  // names the cell last asked of Rust, so moving within one point asks
  // nothing more and an answer for a cell since left is dropped.
  const [blendCell, setBlendCell] = useState<BlendCell | null>(null);
  const blendAsked = useRef("");
  const [showFiltered, setShowFiltered] = useState(false);
  // Excluded dots are hidden unless asked for (asked 2026-10-04); Rust
  // sends them either way, so ticking the box asks it for nothing.
  const [showExcluded, setShowExcluded] = useState(false);
  const [dotColour, setDotColour] = useState<DotColourMode>("source");
  const [pdfFit, setPdfFit] = useState<PolarPdfResult | null>(null);
  const [pdfMode, setPdfMode] = useState(false);
  // The Measure tool (spec.md 9.2): on or off, where the pointer is, and
  // the pinned point A. View state only; nothing of it is saved.
  const [measure, setMeasure] = useState(false);
  const [cursor, setCursor] = useState<PolarPoint | null>(null);
  const [pin, setPin] = useState<PolarPoint | null>(null);
  const request = useRef(0);
  const selection = useSampleSelection();

  // Keep the slice local as well as asking Rust for it. This makes changing
  // from All to one wind speed immediately and consistently narrow the dots,
  // including when an older/in-flight all-speeds response arrives.
  const dots = useMemo(() => {
    const visible = showExcluded ? received : withoutExcluded(received);
    if (rangeEnabled) {
      const keep: number[] = [];
      for (let k = 0; k < visible.count; k++) {
        const speed = visible.points[k * 3 + 1]!;
        if (speed >= rangeLow && speed <= rangeHigh) keep.push(k);
      }
      if (keep.length === visible.count) return visible;
      const points = new Float32Array(keep.length * 3);
      const source = new Uint32Array(keep.length);
      const ids = new Uint32Array(keep.length * 2);
      const flags = new Uint32Array(keep.length);
      keep.forEach((sourceIndex, index) => {
        points.set(visible.points.subarray(sourceIndex * 3, sourceIndex * 3 + 3), index * 3);
        source[index] = visible.source[sourceIndex]!;
        ids.set(visible.ids.subarray(sourceIndex * 2, sourceIndex * 2 + 2), index * 2);
        flags[index] = visible.flags[sourceIndex]!;
      });
      return { ...visible, count: keep.length, points, source, ids, flags };
    }
    if (tws === null) return visible;
    const band = result?.band_kn ?? 1;
    const keep: number[] = [];
    for (let k = 0; k < visible.count; k++) {
      if (Math.abs(visible.points[k * 3 + 1]! - tws) <= band) keep.push(k);
    }
    if (keep.length === visible.count) return visible;
    const points = new Float32Array(keep.length * 3);
    const source = new Uint32Array(keep.length);
    const ids = new Uint32Array(keep.length * 2);
    const flags = new Uint32Array(keep.length);
    keep.forEach((sourceIndex, index) => {
      points.set(visible.points.subarray(sourceIndex * 3, sourceIndex * 3 + 3), index * 3);
      source[index] = visible.source[sourceIndex]!;
      ids.set(visible.ids.subarray(sourceIndex * 2, sourceIndex * 2 + 2), index * 2);
      flags[index] = visible.flags[sourceIndex]!;
    });
    return { ...visible, count: keep.length, points, source, ids, flags };
  }, [received, showExcluded, tws, rangeEnabled, rangeLow, rangeHigh, result?.band_kn]);
  const excludedExist = useMemo(() => {
    for (let k = 0; k < received.count; k++) if (received.flags[k]! & DOT_EXCLUDED) return true;
    return false;
  }, [received]);

  const visibleCount = useMemo(
    () => project.sources.filter((source) => source.visible).length,
    [project.sources],
  );
  const tracksShown = project.sources.some((source) => source.visible && source.kind === "track");
  // Only a track's samples have a time of day; without one, by source.
  const shownDotColour: DotColourMode = tracksShown ? dotColour : "source";

  // Refetches on `project.revision`, which every edit bumps — a source's
  // colour, visibility, weight or label, undo and redo, adding or removing a
  // source — and on `project.id`, so switching projects does not show a
  // stale plot at the same revision number. `tws` refetches the slice.
  useEffect(() => {
    const id = ++request.current;
    const queryTws = rangeEnabled ? null : tws;
    void Promise.all([api.polarPlot(queryTws), api.polarPlotDots(queryTws, showFiltered)])
      .then(([next, nextDots]) => {
        if (request.current !== id) return;
        setResult(next);
        setDots(nextDots);
        setHover(null);
      })
      .catch((err) => { if (request.current === id) reportFailure(err); });
  }, [project.id, project.revision, tws, rangeEnabled, showFiltered]);

  useEffect(() => {
    let cancelled = false;
    setPdfFit(null);
    if (!pdfMode) return;
    void api.polarPdf({ samples: true, nodes: true, filtered: showFiltered,
      excluded: showExcluded, source: null, split: null,
    }).then((next) => { if (!cancelled) setPdfFit(next); })
      .catch((error: unknown) => { if (!cancelled) reportFailure(error); });
    return () => { cancelled = true; };
  }, [api, project.id, project.revision, pdfMode, showFiltered, showExcluded]);
  const heat = useMemo(() => pdfMode
    ? pdfHeat(pdfFit, rangeEnabled ? null : tws, rangeEnabled ? [rangeLow, rangeHigh] : null)
    : null, [pdfMode, pdfFit, tws, rangeEnabled, rangeLow, rangeHigh]);

  useEffect(() => {
    let cancelled = false;
    void api.waveRangeMaxima().then(next => { if (!cancelled) setWaveMaxima(next); })
      .catch((error: unknown) => { if (!cancelled) reportFailure(error); });
    return () => { cancelled = true; };
  }, [api, project.id, project.revision]);

  const selectedDots = useMemo(() => {
    const ids: number[] = [];
    let excluded = 0;
    for (let k = 0; k < dots.count; k++) {
      const id = dotSampleId(dots, k);
      if (!selection.ids.has(id)) continue;
      ids.push(id);
      if (dots.flags[k]! & DOT_EXCLUDED) excluded++;
    }
    return { ids, excluded, included: ids.length - excluded };
  }, [dots, selection.ids]);
  const excludeSelection = (excluded: boolean) => {
    if (readOnly || !selectedDots.ids.length) return;
    void api.setExcluded([], selectedDots.ids, excluded).then(next => onProject?.(next)).catch(reportFailure);
  };
  const cancelSelection = () => { selecting.current = null; setSelectionPath(null); };
  useEffect(cancelSelection, [project.id, project.revision, tws, rangeEnabled, rangeLow, rangeHigh, pdfMode, hidden, showFiltered, showExcluded]);

  const sourcesById = useMemo(() => {
    const map = new Map<number, SourceStyle>();
    for (const source of project.sources) map.set(source.id, { label: source.label, colour: source.colour });
    return map;
  }, [project.sources]);

  // Every curve drawn, the blend's with its label in the interface language.
  const shownResult = useMemo(() => {
    if (!result || !rangeEnabled) return result;
    const min = rangeLow, max = rangeHigh;
    return { ...result, curves: result.curves.filter((curve) => curve.tws >= min && curve.tws <= max),
      blend: result.blend.filter((curve) => curve.tws >= min && curve.tws <= max) };
  }, [result, rangeEnabled, rangeLow, rangeHigh]);
  const allCurves = useMemo(() => [...(shownResult?.curves ?? []), ...blendCurves(shownResult, t)], [shownResult, t]);
  const measuring: Measuring | null = useMemo(
    () => (measure && cursor ? { cursor, crossings: crossings(allCurves, cursor.twa), pin } : null),
    [measure, cursor, pin, allCurves],
  );

  const redraw = useCallback(() => {
    if (canvas.current) draw(canvas.current, shownResult, dots, hover, sourcesById, selection.ids, unit, project.blend.asymmetric, shownDotColour, measuring, variant === "stage", pdfMode, heat);
  }, [shownResult, dots, hover, sourcesById, selection, unit, project.blend.asymmetric, shownDotColour, measuring, variant, pdfMode, heat]);

  useEffect(redraw, [redraw]);

  useEffect(() => {
    const element = wrap.current;
    if (!element || typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(redraw);
    observer.observe(element);
    return () => observer.disconnect();
  }, [redraw]);

  useEffect(() => onThemeChange(redraw), [redraw]);

  // The MCP service's screenshot reads the full-size plot when it is the
  // stage on show (the panel's small plot is not a stage).
  useEffect(() => {
    const element = canvas.current;
    return variant === "stage" && element ? registerCapture(element, redraw) : undefined;
  }, [variant, redraw, visibleCount]);

  /** The pointer's place on the plot, or null where the fan is not (the port side of a symmetric plot). */
  const pointed = useCallback((event: MouseEvent<HTMLCanvasElement>): { point: PolarPoint; scale: number } | null => {
    const rect = event.currentTarget.getBoundingClientRect();
    const layout = fitLayout(rect.width, rect.height, plotMaxBsp(shownResult, dots), 28, project.blend.asymmetric, variant === "stage");
    const x = event.clientX - rect.left;
    if (!project.blend.asymmetric && x < layout.centerX - 1) return null;
    const point = unproject(x, event.clientY - rect.top, layout);
    // A symmetric plot's angles stop at 180°: a hair to the left of the axis is the axis.
    if (!project.blend.asymmetric && point.twa > 180) point.twa = point.twa > 270 ? 0 : 180;
    return { point, scale: layout.scale };
  }, [shownResult, dots, project.blend.asymmetric]);

  const onMove = useCallback((event: MouseEvent<HTMLCanvasElement>) => {
    if (!shownResult || pdfMode || selecting.current) return;
    if (measure) {
      // Measuring replaces the hover: the readout says more than a tooltip.
      setCursor(pointed(event)?.point ?? null);
      return;
    }
    const rect = event.currentTarget.getBoundingClientRect();
    const layout = fitLayout(rect.width, rect.height, plotMaxBsp(shownResult, dots), 28, project.blend.asymmetric, variant === "stage");
    const hit = nearestPoint(
      allCurves, dots, sourcesById,
      event.clientX - rect.left, event.clientY - rect.top, layout, HOVER_DISTANCE_PX,
    );
    setHover(hit);
    // A blend point that is an output-grid cell names the sources behind
    // it; one between cells (a slice off the grid's wind speeds, a spline's
    // in-between angle) has no single cell to name and keeps the plain text.
    const on = (axis: readonly number[], value: number) => axis.findIndex((v) => Math.abs(v - value) <= ON_GRID);
    const i = hit?.blend ? on(project.blend.twa, hit.twa) : -1;
    const j = hit?.blend ? on(project.blend.tws, hit.tws) : -1;
    const key = i >= 0 && j >= 0 ? `${project.id}:${project.revision}:${i}:${j}` : "";
    if (key === blendAsked.current) return;
    blendAsked.current = key;
    setBlendCell(null);
    if (key === "") return;
    void api.blendCell(i, j)
      .then((cell) => { if (blendAsked.current === key) setBlendCell(cell); })
      // A cell that cannot be read keeps the plain tooltip.
      .catch(() => undefined);
  }, [shownResult, dots, sourcesById, allCurves, measure, pointed, pdfMode, project.id, project.revision, project.blend.twa, project.blend.tws, project.blend.asymmetric, api]);
  const leave = useCallback(() => {
    setHover(null);
    setCursor(null);
    blendAsked.current = "";
    setBlendCell(null);
  }, []);

  /** Pins point A where the pointer is: on a curve's own value when the click is on its mark. */
  const onClick = useCallback((event: MouseEvent<HTMLCanvasElement>) => {
    if (suppressClick.current) { suppressClick.current = false; return; }
    if (pdfMode) return;
    if (!measure) {
      const rect = event.currentTarget.getBoundingClientRect();
      const layout = fitLayout(rect.width, rect.height, plotMaxBsp(shownResult, dots), 28, project.blend.asymmetric, variant === "stage");
      const x = event.clientX - rect.left, y = event.clientY - rect.top;
      let closest: number | null = null, distance = HOVER_DISTANCE_PX ** 2;
      for (let k = 0; k < dots.count; k++) {
        const at = projectPoint(dots.points[k * 3]!, dots.points[k * 3 + 2]!, layout);
        const d = (at.x - x) ** 2 + (at.y - y) ** 2;
        if (d <= distance) { distance = d; closest = dotSampleId(dots, k); }
      }
      const next = new Set(event.shiftKey ? selection.ids : []);
      if (closest !== null) {
        if (event.shiftKey && next.has(closest)) next.delete(closest);
        else next.add(closest);
      }
      selectSamples(next, "plot");
      return;
    }
    const at = pointed(event);
    if (!at) return;
    const here = crossings(allCurves, at.point.twa);
    const near = here[nearestCrossing(here, at.point.bsp)];
    const snapped = near !== undefined && Math.abs(near.bsp - at.point.bsp) * at.scale <= PIN_SNAP_PX;
    setPin({ twa: at.point.twa, bsp: snapped ? near.bsp : at.point.bsp });
    setCursor(at.point);
  }, [measure, pointed, allCurves, pdfMode, dots, shownResult, selection.ids, selectSamples, project.blend.asymmetric, variant]);

  const chooseTool = (next: "lasso" | "box") => {
    cancelSelection();
    setTool(current => !measure && current === next ? "select" : next);
    setMeasure(false); setPin(null); setCursor(null); setHover(null);
  };
  const onSelectionDown = (event: ReactPointerEvent<HTMLCanvasElement>) => {
    suppressClick.current = false;
    if (event.button !== 0 || tool === "select" || measure || pdfMode) return;
    const rect = event.currentTarget.getBoundingClientRect();
    const x = event.clientX - rect.left, y = event.clientY - rect.top;
    selecting.current = { pointer: event.pointerId, x, y, path: [x, y], tool };
    leave();
    setSelectionPath([x, y]);
    event.currentTarget.setPointerCapture?.(event.pointerId);
    event.preventDefault();
  };
  const onSelectionMove = (event: ReactPointerEvent<HTMLCanvasElement>) => {
    const g = selecting.current;
    if (!g || g.pointer !== event.pointerId) return;
    const rect = event.currentTarget.getBoundingClientRect();
    const x = event.clientX - rect.left, y = event.clientY - rect.top;
    if (g.tool === "lasso") {
      if (Math.hypot(x - g.path[g.path.length - 2]!, y - g.path[g.path.length - 1]!) >= 2) g.path.push(x, y);
    } else g.path = [g.x, g.y, x, g.y, x, y, g.x, y];
    setSelectionPath([...g.path]);
  };
  const onSelectionUp = (event: ReactPointerEvent<HTMLCanvasElement>) => {
    const g = selecting.current;
    if (!g || g.pointer !== event.pointerId) return;
    onSelectionMove(event);
    const rect = event.currentTarget.getBoundingClientRect();
    const x = event.clientX - rect.left, y = event.clientY - rect.top;
    cancelSelection();
    if (event.currentTarget.hasPointerCapture?.(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId);
    if (Math.hypot(x - g.x, y - g.y) < 4 && g.path.length < 10) return;
    suppressClick.current = true;
    const layout = fitLayout(rect.width, rect.height, plotMaxBsp(shownResult, dots), 28, project.blend.asymmetric, variant === "stage");
    const screen = new Float32Array(dots.count * 2);
    for (let k = 0; k < dots.count; k++) {
      const point = projectPoint(dots.points[k * 3]!, dots.points[k * 3 + 2]!, layout);
      screen[k * 2] = point.x; screen[k * 2 + 1] = point.y;
    }
    const hits = lassoSelect(screen, g.path);
    const next = new Set(event.shiftKey ? selection.ids : []);
    for (const k of hits) next.add(dotSampleId(dots, k));
    selectSamples(next, "plot");
  };
  useEffect(() => {
    if (hidden) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== "Escape" || event.target instanceof HTMLElement && event.target.closest("input, textarea, select, [contenteditable=true], [role=dialog]")) return;
      if (selecting.current) { event.preventDefault(); cancelSelection(); return; }
      if (!measure && selection.ids.size) { event.preventDefault(); clearSamples("plot"); }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [hidden, measure, selection.ids, clearSamples]);

  /** Turns the Measure tool on or off; off forgets the pinned point. */
  const toggleMeasure = useCallback(() => {
    setMeasure((on) => !on);
    setPin(null);
    setCursor(null);
    setHover(null);
    blendAsked.current = "";
    setBlendCell(null);
  }, []);

  // Escape lets the pinned point go, before anything else hears it (the
  // full-size overlay closes on Escape): one press unpins, the next closes.
  useEffect(() => {
    if (!measure || !pin || hidden) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      event.stopImmediatePropagation();
      event.preventDefault();
      setPin(null);
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [measure, pin, hidden]);

  const hasDomain = domainMin !== null && domainMax !== null;
  const sliderValue = tws ?? (hasDomain ? Math.round(((domainMin as number) + (domainMax as number)) / 2) : 0);
  const rangeValueAt = (clientX: number, bounds: DOMRect) => {
    const fraction = Math.max(0, Math.min(1, (clientX - bounds.left) / Math.max(1, bounds.width)));
    const raw = (domainMin ?? 0) + fraction * ((domainMax ?? 30) - (domainMin ?? 0));
    return Math.max(domainMin ?? 0, Math.min(domainMax ?? 30, Math.round(raw * 2) / 2));
  };
  const onRangePointerDown = (event: ReactPointerEvent<HTMLDivElement>) => {
    if (!hasDomain || event.button !== 0 || event.target instanceof HTMLInputElement) return;
    const value = rangeValueAt(event.clientX, event.currentTarget.getBoundingClientRect());
    const mode = value >= rangeLow && value <= rangeHigh
      ? "move"
      : Math.abs(value - rangeLow) <= Math.abs(value - rangeHigh) ? "low" : "high";
    rangeDrag.current = { pointerId: event.pointerId, mode, startX: event.clientX, low: rangeLow, high: rangeHigh };
    event.currentTarget.setPointerCapture(event.pointerId);
    event.preventDefault();
    if (mode === "low") setTwsRange([Math.min(value, rangeHigh), rangeHigh]);
    if (mode === "high") setTwsRange([rangeLow, Math.max(value, rangeLow)]);
  };
  const onRangePointerMove = (event: ReactPointerEvent<HTMLDivElement>) => {
    const drag = rangeDrag.current;
    if (!drag || drag.pointerId !== event.pointerId) return;
    const value = rangeValueAt(event.clientX, event.currentTarget.getBoundingClientRect());
    if (drag.mode === "move") {
      const step = 0.5;
      const trackWidth = Math.max(1, event.currentTarget.getBoundingClientRect().width);
      const delta = Math.round(((event.clientX - drag.startX) / trackWidth
        * ((domainMax ?? 30) - (domainMin ?? 0))) / step) * step;
      const width = drag.high - drag.low;
      const low = Math.max(domainMin!, Math.min(domainMax! - width, drag.low + delta));
      setTwsRange([low, low + width]);
    } else if (drag.mode === "low") setTwsRange([Math.min(value, rangeHigh), rangeHigh]);
    else setTwsRange([rangeLow, Math.max(value, rangeLow)]);
  };
  const onRangePointerEnd = (event: ReactPointerEvent<HTMLDivElement>) => {
    if (rangeDrag.current?.pointerId === event.pointerId) rangeDrag.current = null;
  };
  const hasAnyPoint = [...(shownResult?.curves ?? []), ...(shownResult?.blend ?? [])].some((curve) => curve.points.length > 0)
    || dots.count > 0;
  const hasPdfData = heat !== null;
  // The readout's numbers: every one arrives in knots and leaves in the display unit.
  const symbol = SPEED_SYMBOL[unit];
  const shownSpeed = (knots: number) => `${(knots * SPEED_FACTOR[unit]).toFixed(2)} ${symbol}`;
  const shownAngle = (twa: number) => Math.min(twa, 360 - twa).toFixed(0);
  const reference = measuring ? measuring.crossings[nearestCrossing(measuring.crossings, measuring.cursor.bsp)] : undefined;
  const pinned = measuring?.pin ? measureBetween(measuring.pin, measuring.cursor) : null;

  return (
    <div className={`polar-plot polar-plot-${variant}`}>
      <div className="polar-plot-controls">
        <label className="polar-plot-all">
          <input type="checkbox" data-feature="plot:all" checked={tws === null && !rangeEnabled}
            title={t("Show every wind speed a source has, instead of one slice")}
            onChange={(event) => { setRangeEnabled(false); setTws(event.target.checked ? null : sliderValue); }} />
          {t("All")}
        </label>
        <label className="polar-plot-range-toggle" title={t("Show data within a wind speed range")}>
          <input type="checkbox" data-feature="plot:tws-range" checked={rangeEnabled} disabled={!hasDomain}
            onChange={(event) => {
              if (event.target.checked) {
                const low = tws === null ? domainMin! : Math.max(domainMin!, tws - 1);
                const high = tws === null ? domainMax! : Math.min(domainMax!, tws + 1);
                setTwsRange([low, Math.max(low, high)]);
                setRangeEnabled(true);
                setTws(null);
              } else {
                setRangeEnabled(false);
                setTws(Math.round(((rangeLow + rangeHigh) / 2) * 2) / 2);
              }
            }} />
          {t("Range")}
        </label>
        {rangeEnabled ? (
          <div className="polar-plot-range-sliders" aria-label={t("Wind speed range")}
            style={{ "--range-start": `${(rangeLow - (domainMin ?? 0)) / Math.max((domainMax ?? 30) - (domainMin ?? 0), 1) * 100}%`,
              "--range-end": `${(rangeHigh - (domainMin ?? 0)) / Math.max((domainMax ?? 30) - (domainMin ?? 0), 1) * 100}%` } as CSSProperties}
            onPointerDown={onRangePointerDown} onPointerMove={onRangePointerMove}
            onPointerUp={onRangePointerEnd} onPointerCancel={onRangePointerEnd}>
            <input className="range-lower" type="range" min={domainMin ?? 0} max={domainMax ?? 30} step={0.5}
              value={rangeLow} disabled={!hasDomain} aria-label={t("Minimum wind speed")}
              onChange={(event) => setTwsRange([Math.min(Number(event.target.value), rangeHigh), rangeHigh])} />
            <input className="range-upper" type="range" min={domainMin ?? 0} max={domainMax ?? 30} step={0.5}
              value={rangeHigh} disabled={!hasDomain} aria-label={t("Maximum wind speed")}
              onChange={(event) => setTwsRange([rangeLow, Math.max(Number(event.target.value), rangeLow)])} />
          </div>
        ) : (
          <input type="range" data-feature="plot:tws" min={domainMin ?? 0} max={domainMax ?? 30} step={0.5}
            value={sliderValue} disabled={tws === null || !hasDomain}
            aria-label={t("Wind speed")} title={t("The true wind speed the plot slices at")}
            onChange={(event) => setTws(Number(event.target.value))} />
        )}
        <span className="polar-plot-tws-value"
          title={tracksShown && tws !== null && !rangeEnabled ? t("Sample dots within {band} {unit} of this wind speed (Settings)", {
            band: displaySpeed(result?.band_kn ?? 1, unit, 2), unit: SPEED_SYMBOL[unit],
          }) : undefined}>
          {rangeEnabled
            ? `${displaySpeed(rangeLow, unit, 1)}–${displaySpeed(rangeHigh, unit, 1)} ${SPEED_SYMBOL[unit]}`
            : tws === null ? t("All") : t("{tws} {unit}", { tws: displaySpeed(tws, unit, 1), unit: SPEED_SYMBOL[unit] })}
        </span>
        <label className="polar-plot-display">
          {t("Display")}
          <select data-feature="plot:display" value={pdfMode ? "pdf" : "polar"}
            title={t("Show the polar curves or probability density")}
            onChange={(event) => {
              const nextPdf = event.target.value === "pdf";
              setPdfMode(nextPdf);
              setHover(null);
              if (nextPdf) { setMeasure(false); setPin(null); setCursor(null); }
            }}>
            <option value="polar">{t("Polar")}</option>
            <option value="pdf">{t("Probability density (PDF)")}</option>
          </select>
        </label>
        {pdfMode && <span className="polar-plot-pdf-legend" title={t("Density is probability per knot of boat speed")}>
          {t("Probability density")}
          <span className="polar-plot-pdf-gradient" aria-hidden="true" />
          <span>{t("Low")}</span><span>{t("High")}</span>
        </span>}
        <label className="polar-plot-filtered"
          title={tracksShown ? t("Also draw the samples the filters take out, dimmed") : t("No visible track has samples to filter")}>
          <input type="checkbox" data-feature="plot:show-filtered" checked={showFiltered} disabled={!tracksShown}
            onChange={(event) => setShowFiltered(event.target.checked)} />
          {t("Filtered")}
        </label>
        <label className="polar-plot-excluded"
          title={excludedExist ? t("Also draw the samples you excluded, hollow") : t("Nothing is excluded")}>
          <input type="checkbox" data-feature="plot:show-excluded" checked={showExcluded} disabled={!excludedExist && !showExcluded}
            onChange={(event) => setShowExcluded(event.target.checked)} />
          {t("Excluded")}
        </label>
        <label className="polar-plot-colour">
          {t("Colour")}
          <select data-feature="plot:colour" value={shownDotColour} disabled={!tracksShown}
            title={tracksShown ? t("What the dots' colour shows") : t("No visible track has samples to colour")}
            onChange={(event) => setDotColour(event.target.value as DotColourMode)}>
            <option value="source">{t("By source")}</option>
            <option value="timeOfDay">{t("By time of day")}</option>
          </select>
        </label>
        <button className="small" data-feature="plot:measure" aria-pressed={measure}
          disabled={pdfMode || (!hasAnyPoint && !measure)} onClick={toggleMeasure}
          title={t("Measure boat speeds on the plot: point to compare every curve at one wind angle, click to pin a point and measure from it")}>
          {t("Measure")}
        </button>
      </div>
      <div className="polar-plot-selection-tools">
        <button className="small" data-feature="plot:lasso" aria-pressed={!measure && tool === "lasso"} disabled={pdfMode}
          title={t("Draw round dots to select them; Shift adds to the selection")} onClick={() => chooseTool("lasso")}>{t("Lasso")}</button>
        <button className="small" data-feature="plot:box" aria-pressed={!measure && tool === "box"} disabled={pdfMode}
          title={t("Drag a box round dots to select them; Shift adds to the selection")} onClick={() => chooseTool("box")}>{t("Box")}</button>
        <span role="status">{t("{count} selected", { count: selectedDots.ids.length })}</span>
        <button className="small" data-feature="plot:exclude" disabled={pdfMode || readOnly || !selectedDots.included}
          title={t("Remove the selected dots from the blend (undoable)")} onClick={() => excludeSelection(true)}>{t("Exclude")}</button>
        <button className="small" data-feature="plot:include" disabled={pdfMode || readOnly || !selectedDots.excluded}
          title={t("Put the selected dots back into the blend (undoable)")} onClick={() => excludeSelection(false)}>{t("Include")}</button>
        <button className="small" data-feature="plot:clear-selection" disabled={!selection.ids.size}
          title={t("Select nothing (Escape)")} onClick={() => clearSamples("plot")}>{t("Clear")}</button>
      </div>
      {shownDotColour === "timeOfDay" && <DayBandLegend className="polar-plot-bands" />}
      {visibleCount === 0 ? (
        <div className="plot-placeholder muted">{t("The polar plot appears here once the project has a source.")}</div>
      ) : (
        <div className="polar-plot-canvas-wrap" ref={wrap}>
          <canvas ref={canvas} className={measure ? "measuring" : tool !== "select" && !pdfMode ? "selecting" : undefined}
            onMouseMove={onMove} onMouseLeave={leave} onClick={onClick}
            onPointerDown={onSelectionDown} onPointerMove={onSelectionMove} onPointerUp={onSelectionUp}
            onPointerCancel={cancelSelection} onLostPointerCapture={cancelSelection} />
          {selectionPath && <svg className="polar-plot-selection-path" aria-hidden="true"><polygon
            points={Array.from({ length: selectionPath.length / 2 }, (_, k) => `${selectionPath[k * 2]},${selectionPath[k * 2 + 1]}`).join(" ")} /></svg>}
          {measuring && (
            <div className="polar-plot-measure" role="status">
              <h4>{t("At TWA {twa}°", { twa: shownAngle(measuring.cursor.twa) })}</h4>
              <div className="muted">{t("Pointer: {bsp}", { bsp: shownSpeed(measuring.cursor.bsp) })}</div>
              {measuring.crossings.length === 0 ? (
                <div className="muted">{t("No curve at this angle")}</div>
              ) : (
                <ul>
                  {measuring.crossings.map((crossing, index) => (
                    <li key={index} className={crossing === reference ? "reference" : undefined}>
                      <span className="polar-plot-measure-name">
                        <span className="day-band-swatch" style={{ background: crossing.colour }} />
                        {crossing.label} · {displaySpeed(crossing.tws, unit, 1)} {symbol}
                      </span>
                      <span>{shownSpeed(crossing.bsp)}</span>
                      {crossing === reference || !reference ? (
                        <span className="muted polar-plot-measure-delta">{t("pointed at")}</span>
                      ) : (
                        <>
                          <span>{signed((crossing.bsp - reference.bsp) * SPEED_FACTOR[unit], 2)} {symbol}</span>
                          <span>{reference.bsp > 0 ? `${signed((crossing.bsp / reference.bsp - 1) * 100, 1)}%` : ""}</span>
                        </>
                      )}
                    </li>
                  ))}
                </ul>
              )}
              {measuring.pin && pinned ? (
                <div className="polar-plot-measure-pin">
                  <div>{t("A: {bsp} at {twa}°", { bsp: shownSpeed(measuring.pin.bsp), twa: shownAngle(measuring.pin.twa) })}</div>
                  <div>{pinned.ratio === null
                    ? t("B − A: {delta}, {angle}° apart", {
                      delta: `${signed(pinned.deltaBsp * SPEED_FACTOR[unit], 2)} ${symbol}`, angle: pinned.deltaTwa.toFixed(0),
                    })
                    : t("B − A: {delta} ({percent}%), {angle}° apart", {
                      delta: `${signed(pinned.deltaBsp * SPEED_FACTOR[unit], 2)} ${symbol}`,
                      percent: signed((pinned.ratio - 1) * 100, 1), angle: pinned.deltaTwa.toFixed(0),
                    })}</div>
                </div>
              ) : null}
              <div className="muted">{measuring.pin ? t("Click to move A; Esc lets it go") : t("Click to pin point A")}</div>
            </div>
          )}
          {hover && blendCell && (
            <BlendCellTooltip cell={blendCell} sources={project.sources} colour={project.blend.colour} unit={unit}
              smoothing={project.blend.smoothing} x={hover.x + 10} y={hover.y + 10} />
          )}
          {hover && !blendCell && (
            <div role="tooltip" className="polar-plot-tooltip" style={{ left: hover.x + 10, top: hover.y + 10 }}>
              <strong style={{ color: hover.colour }}>{hover.label}</strong>
              <div>{t("TWA {twa}°, TWS {tws} {unit}, {speed} {value} {unit}", {
                twa: Math.min(hover.twa, 360 - hover.twa).toFixed(0), tws: (hover.tws * SPEED_FACTOR[unit]).toFixed(1),
                speed: hover.speed, value: (hover.bsp * SPEED_FACTOR[unit]).toFixed(2), unit: SPEED_SYMBOL[unit],
              })}</div>
            </div>
          )}
          {result && (pdfMode ? !hasPdfData : !hasAnyPoint) && (
            <div className="polar-plot-empty muted">{pdfMode ? t("No visible points pass the current filters to form a PDF.") : t("No source has data at this wind speed.")}</div>
          )}
        </div>
      )}
      <WaveRangeControls namespace="plot" maxima={waveMaxima} ranges={waveRanges} units={units} count={dots.count} disabled={readOnly}
        onChange={ranges => { void updateWaveRanges(() => ranges); }} />
    </div>
  );
}
