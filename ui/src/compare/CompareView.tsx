import { useBoatApi } from "../boats/context";
import { useCallback, useEffect, useMemo, useRef, useState, type PointerEvent as ReactPointerEvent, type KeyboardEvent as ReactKeyboardEvent } from "react";

import { registerRedraw } from "../automation";
import { registerCapture } from "../mcp/capture";
import { describeError, reportFailure } from "../errors";
import type { AppSettings } from "../generated/AppSettings";
import type { CompareOperand } from "../generated/CompareOperand";
import type { ProjectSummary } from "../generated/ProjectSummary";
import { later, setHint } from "../hint";
import { msg, useT } from "../i18n";

import type { Layout } from "../polar/geometry3d";
import { PolarScene, SHAPE_CROSS } from "../polar/scene3d";
import { buildGuides, presetView, SPEED_FACTOR, SPEED_SYMBOL, type CameraPreset, type GuideLabel } from "../polar/view3d";
import { onThemeChange } from "../settings/themes";
import { CLASS_A_ONLY, CLASS_B_ONLY, CLASS_BOTH, emptyCompare, type ComparePacket } from "./comparePacket";
import { operandOf, setCompareChoice, swapOperands, useCompareChoice } from "./compareState";
import {
  compareBounds, compareCell, compareSurfaces, DEFAULT_COMPARE_TOGGLES, heatCellAt, heatImage, heatLayout, labelStep, operandInfo,
  operandKey, regionRows, singleMarkers, spanText, type CompareToggles, type HeatCell, type HeatImage,
} from "./compareModel";
import { currentScheme, legendGradient, POLES, type Scheme } from "./diverging";

const LAYOUT_NAMES: Record<Layout, string> = { tower: msg("Polar tower"), cartesian: msg("Cartesian") };
const CAMERAS: readonly { id: CameraPreset; label: string; tip: string }[] = [
  { id: "top", label: msg("Top"), tip: msg("Look down the wind-speed axis: the classic polar diagram") },
  { id: "side", label: msg("Side"), tip: msg("Look across the wind-speed axis: each wind speed a level") },
  { id: "iso", label: msg("Isometric"), tip: msg("The three-quarter view") },
];
const KIND_GLYPH: Record<string, string> = { orc: "◆", polar_file: "▦", track: "〰", blend: "◎" };

function cssColour(name: string, fallback: string): string {
  if (typeof document === "undefined") return fallback;
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim() || fallback;
}

/**
 * The Compare stage (spec.md 11): two operands, A and B — any source's
 * polar, any track's polar segment, or the blend — as translucent surfaces
 * in 3D with the difference surface ΔBSP = A − B between them on a
 * diverging scale centred on zero; cells only one operand covers drawn grey
 * and hatched. Beside it: the scale's legend and range, the summary
 * (overlap, mean and max |Δ|, the regions where each is faster) and a 2D
 * heat map of Δ. Rust computes every value; the stage refetches whenever
 * the project changes, so an edit reaches it at once.
 */
export default function CompareView({ project, settings }: {
  project: ProjectSummary;
  settings: AppSettings | null;
}) {
  const api = useBoatApi();
  const t = useT();
  const canvas = useRef<HTMLCanvasElement>(null);
  const labelsHost = useRef<HTMLDivElement>(null);
  const scene = useRef<PolarScene | null>(null);
  const frame = useRef(0);
  const labels = useRef<GuideLabel[]>([]);
  const request = useRef(0);
  const fitted = useRef(false);
  const [unavailable, setUnavailable] = useState<string | null>(null);
  const [packet, setPacket] = useState<ComparePacket>(emptyCompare);
  const [arrived, setArrived] = useState(false);
  const [layout, setLayout] = useState<Layout>("tower");
  const [toggles, setToggles] = useState<CompareToggles>(DEFAULT_COMPARE_TOGGLES);
  const [scheme, setScheme] = useState<Scheme>(currentScheme);
  const [hovered, setHovered] = useState<HeatCell | null>(null);
  const [surfaceHover, setSurfaceHover] = useState<{ i: number; j: number; x: number; y: number; left: boolean; above: boolean } | null>(null);
  const hoverFrame = useRef(0);
  const compareReady = useRef(false);
  const markerCells = useRef<Uint32Array>(new Uint32Array(0));
  /** Why the last comparison failed, shown on the stage; null when it did not. */
  const [failure, setFailure] = useState<{ text: string; detail: string | null } | null>(null);
  const choice = useCompareChoice(project);
  const { a, b, percent, thresholdKn } = choice;
  const unit = settings?.units.speed ?? "kn";
  const factor = SPEED_FACTOR[unit];
  const symbol = SPEED_SYMBOL[unit];

  const clearSurfaceHover = useCallback(() => {
    cancelAnimationFrame(hoverFrame.current);
    hoverFrame.current = 0;
    setSurfaceHover(null);
  }, []);

  const hoverSurface = (event: ReactPointerEvent<HTMLCanvasElement>) => {
    if (event.buttons !== 0 || !compareReady.current) { clearSurfaceHover(); return; }
    const element = event.currentTarget;
    const rect = element.getBoundingClientRect();
    const x = event.clientX - rect.left, y = event.clientY - rect.top;
    // Pick at most once per frame, using the latest pointer position.
    cancelAnimationFrame(hoverFrame.current);
    hoverFrame.current = requestAnimationFrame(() => {
      hoverFrame.current = 0;
      const current = scene.current;
      if (!current || !compareReady.current) return;
      const hit = current.pickSurface(x, y);
      // An isolated one-only cell has a cross instead of a surface quad.
      const marker = hit ? undefined : markerCells.current[current.pick(x, y, 8)];
      const i = hit?.twaIndex ?? (marker === undefined ? -1 : Math.floor(marker / packet.tws.length));
      const j = hit?.twsIndex ?? (marker === undefined ? -1 : marker % packet.tws.length);
      if (!compareCell(packet, i, j, percent)) { setSurfaceHover(null); return; }
      const left = x > rect.width / 2, above = y > rect.height / 2;
      setSurfaceHover({ i, j, x: element.offsetLeft + x + (left ? -12 : 12),
        y: element.offsetTop + y + (above ? -12 : 12), left, above });
    });
  };

  const paint = useCallback(() => {
    const current = scene.current;
    if (!current) return;
    current.render();
    const host = labelsHost.current;
    if (!host) return;
    labels.current.forEach((label, k) => {
      const span = host.children[k] as HTMLElement | undefined;
      if (!span) return;
      const at = current.toScreen(...label.at);
      span.style.display = at ? "" : "none";
      if (at) span.style.transform = `translate(${at[0]}px, ${at[1]}px)`;
    });
  }, []);

  const draw = useCallback(() => {
    if (frame.current !== 0) return;
    frame.current = requestAnimationFrame(() => {
      frame.current = 0;
      paint();
    });
  }, [paint]);

  // The scene lives as long as the stage.
  useEffect(() => {
    const element = canvas.current;
    if (!element) return;
    let made: PolarScene;
    try {
      made = new PolarScene(element);
    } catch {
      setUnavailable(msg("The 3D view needs WebGL, which this system does not offer."));
      return;
    }
    scene.current = made;
    const offRedraw = registerRedraw(element, paint);
    // The MCP service's screenshot reads this canvas (every build).
    const offCapture = registerCapture(element, paint);
    made.setBackground(cssColour("--inset", "#1f2c3c"));
    made.enableControls(element, draw, clearSurfaceHover);
    const resize = () => {
      // Hidden with its stage (kept mounted): keep the size it had.
      if (element.clientWidth === 0 || element.clientHeight === 0) return;
      clearSurfaceHover();
      made.resize(element.clientWidth, element.clientHeight);
      draw();
    };
    resize();
    const observer = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(resize);
    observer?.observe(element);
    const offTheme = onThemeChange(() => {
      made.setBackground(cssColour("--inset", "#1f2c3c"));
      setScheme(currentScheme());
      draw();
    });
    return () => {
      observer?.disconnect();
      offTheme();
      offRedraw();
      offCapture();
      cancelAnimationFrame(frame.current);
      cancelAnimationFrame(hoverFrame.current);
      hoverFrame.current = 0;
      frame.current = 0;
      made.dispose();
      scene.current = null;
    };
  }, [draw, paint, clearSurfaceHover]);

  useEffect(() => { fitted.current = false; }, [project.id]);

  // Refetched on every document change (an edit anywhere may move either
  // operand) and on every new choice.
  const operands = `${operandKey(a)}|${operandKey(b)}`;
  useEffect(() => {
    const id = ++request.current;
    compareReady.current = false;
    clearSurfaceHover();
    setHovered(null);
    void api.comparePolars(a, b, thresholdKn)
      .then((next) => {
        if (request.current !== id) return;
        setPacket(next);
        setArrived(true);
        setFailure(null);
        compareReady.current = true;
      })
      .catch((error: unknown) => {
        if (request.current !== id) return;
        // Nothing stale stays on screen under the new choice.
        setPacket(emptyCompare());
        setArrived(false);
        setHovered(null);
        setFailure(describeError(error));
        reportFailure(error);
      });
    return () => { compareReady.current = false; request.current += 1; };
  }, [project.id, project.revision, operands, thresholdKn, clearSurfaceHover]);

  const infoA = operandInfo(project, a);
  const infoB = operandInfo(project, b);
  const bounds = useMemo(() => compareBounds(packet, layout), [packet, layout]);

  useEffect(() => {
    const current = scene.current;
    if (!current) return;
    clearSurfaceHover();
    setHovered(null);
    // Lone cells only one operand covers have no whole hatched quad: each
    // such node is marked with a cross (spec.md 11).
    const markers = toggles.delta ? singleMarkers(packet, scheme) : { points: new Float32Array(0), colors: new Float32Array(0), cells: new Uint32Array(0), count: 0 };
    markerCells.current = markers.cells;
    current.setData({
      samples: markers.points, colors: markers.colors, shapes: new Float32Array(markers.count).fill(SHAPE_CROSS), layout,
      surfaces: compareSurfaces(packet, toggles, { a: infoA.colour, b: infoB.colour }, percent, scheme),
    });
    const guides = buildGuides(bounds, layout, unit, project.blend.asymmetric);
    current.setGuides(guides.segments, cssColour("--muted", "#b3c9de"));
    labels.current = guides.labels;
    const host = labelsHost.current;
    if (host) {
      host.replaceChildren(...guides.labels.map((label) => {
        const span = document.createElement("span");
        span.textContent = label.text;
        return span;
      }));
    }
    // Framed once the first comparison has arrived, not round the empty one.
    if (!fitted.current && arrived) {
      current.setView(presetView("iso", bounds, current.camera.fov));
      fitted.current = true;
    }
    draw();
  }, [packet, arrived, toggles, layout, bounds, unit, percent, scheme, infoA.colour, infoB.colour, project.blend.asymmetric, draw, clearSurfaceHover]);

  const camera = (preset: CameraPreset) => {
    clearSurfaceHover();
    const current = scene.current;
    if (!current) return;
    current.setView(presetView(preset, bounds, current.camera.fov));
    draw();
  };

  const chooseLayout = (next: Layout) => {
    clearSurfaceHover();
    setLayout(next);
    const current = scene.current;
    if (current) current.setView(presetView("iso", compareBounds(packet, next), current.camera.fov));
  };

  /** Δ in the unit shown, signed. */
  const delta = (value: number) => {
    if (!Number.isFinite(value)) return "—";
    const sign = value > 0 ? "+" : value < 0 ? "−" : "";
    return percent ? `${sign}${Math.abs(value).toFixed(1)} %` : `${sign}${Math.abs(value * factor).toFixed(2)} ${symbol}`;
  };
  const size = (value: number) => (!Number.isFinite(value) ? "—"
    : percent ? `${value.toFixed(1)} %` : `${(value * factor).toFixed(2)} ${symbol}`);
  const speed = (knots: number) => (Number.isFinite(knots) ? `${(knots * factor).toFixed(2)} ${symbol}` : "—");
  const windSpeed = (knots: number) => `${Number((knots * factor).toFixed(1))} ${symbol}`;
  const cellDelta = (cell: HeatCell) => cell.cls === CLASS_BOTH
    ? Number.isFinite(cell.delta) ? delta(cell.delta) : t("not comparable in %")
    : cell.cls === CLASS_A_ONLY ? t("only A") : cell.cls === CLASS_B_ONLY ? t("only B") : "—";
  const surfaceCell = surfaceHover ? compareCell(packet, surfaceHover.i, surfaceHover.j, percent) : null;

  const stats = percent ? packet.pct : packet.kn;
  const where = (cell: number) => {
    if (cell < 0) return null;
    const nj = packet.tws.length;
    return { twa: packet.twa[Math.floor(cell / nj)]!, tws: packet.tws[cell % nj]! };
  };
  const maxAt = stats ? where(stats.maxCell) : null;
  const regions = useMemo(() => regionRows(packet), [packet]);
  const heat = useMemo(() => heatImage(packet, percent, scheme), [packet, percent, scheme]);

  const [thresholdText, setThresholdText] = useState<string | null>(null);
  const shownThreshold = thresholdText ?? String(Number((thresholdKn * factor).toFixed(3)));
  const commitThreshold = (text: string) => {
    const value = Number(text.replace(",", "."));
    setThresholdText(null);
    if (Number.isFinite(value) && value >= 0) setCompareChoice(project, { thresholdKn: value / factor });
  };

  return (
    <div className="view3d compare" tabIndex={-1}>
      <canvas ref={canvas} className="view3d-canvas compare-canvas"
        onPointerEnter={() => setHint(later(msg("Drag to turn, right-drag to pan, scroll to zoom.")))}
        onPointerMove={hoverSurface} onPointerDown={clearSurfaceHover} onPointerCancel={clearSurfaceHover}
        onLostPointerCapture={clearSurfaceHover} onWheel={clearSurfaceHover}
        onPointerLeave={() => { setHint(null); clearSurfaceHover(); }} />
      <div className="view3d-labels compare-labels" ref={labelsHost} aria-hidden="true" />
      {surfaceHover && surfaceCell && <div role="tooltip" className="view3d-tooltip compare-tooltip" style={{
        left: surfaceHover.x, top: surfaceHover.y,
        transform: `translate(${surfaceHover.left ? "-100%" : "0"}, ${surfaceHover.above ? "-100%" : "0"})`,
      }}>
        <dl>
          <div><dt>TWA</dt><dd>{Number(surfaceCell.twa.toFixed(2))}°</dd></div>
          <div><dt>TWS</dt><dd>{windSpeed(surfaceCell.tws)}</dd></div>
          <div><dt className="compare-tooltip-source" style={{ color: infoA.colour }}>A · {infoA.label ?? t("Blend")}</dt><dd>{speed(surfaceCell.a)}</dd></div>
          <div><dt className="compare-tooltip-source" style={{ color: infoB.colour }}>B · {infoB.label ?? t("Blend")}</dt><dd>{speed(surfaceCell.b)}</dd></div>
          <div><dt>Δ</dt><dd>{cellDelta(surfaceCell)}</dd></div>
        </dl>
      </div>}
      {unavailable !== null && <p className="view3d-unavailable muted">{t(unavailable)}</p>}
      {failure !== null && (
        <p className="view3d-empty compare-empty compare-failure" role="alert" title={failure.detail ?? undefined}>
          {t("The comparison could not be made: {reason}", { reason: failure.text })}</p>
      )}
      {unavailable === null && failure === null && arrived && packet.overlap === 0 && (
        <p className="view3d-empty compare-empty muted">{t("A and B share no cell with a value: there is nothing to compare. Cells only one of them covers are drawn grey and hatched.")}</p>
      )}

      <div className="view3d-toolbar compare-toolbar">
        <select data-feature="compare:layout" aria-label={t("Layout")} value={layout}
          title={t("Polar tower: angle and radius are TWA and BSP, height is TWS. Cartesian: TWA, TWS and BSP on straight axes.")}
          onChange={(event) => chooseLayout(event.target.value as Layout)}>
          {(Object.keys(LAYOUT_NAMES) as Layout[]).map((id) => <option key={id} value={id}>{t(LAYOUT_NAMES[id])}</option>)}
        </select>
        <span className="view3d-group" role="group" aria-label={t("Camera")}>
          {CAMERAS.map((c) => (
            <button key={c.id} className="small" data-feature={`compare:camera-${c.id}`} title={t(c.tip)} onClick={() => camera(c.id)}>
              {t(c.label)}
            </button>
          ))}
        </span>
      </div>

      <div className="compare-side">
        <section className="compare-operands" aria-label={t("Operands")}>
          <OperandPicker which="a" project={project} value={a}
            onChange={(next) => setCompareChoice(project, { a: next })} />
          <button className="small compare-swap" data-feature="compare:swap" title={t("Swap A and B")}
            aria-label={t("Swap A and B")} onClick={() => swapOperands(project)}>⇅</button>
          <OperandPicker which="b" project={project} value={b}
            onChange={(next) => setCompareChoice(project, { b: next })} />
        </section>

        <fieldset className="view3d-show compare-show">
          <legend>{t("Show")}</legend>
          <label title={t("Operand A as a translucent surface in its colour")}>
            <input type="checkbox" data-feature="compare:show-a" checked={toggles.a}
              onChange={(event) => setToggles({ ...toggles, a: event.target.checked })} />
            {t("Surface A")}
          </label>
          <label title={t("Operand B as a translucent surface in its colour")}>
            <input type="checkbox" data-feature="compare:show-b" checked={toggles.b}
              onChange={(event) => setToggles({ ...toggles, b: event.target.checked })} />
            {t("Surface B")}
          </label>
          <label title={t("Midway between A and B, coloured by the difference A − B; grey and hatched where only one has a value")}>
            <input type="checkbox" data-feature="compare:show-delta" checked={toggles.delta}
              onChange={(event) => setToggles({ ...toggles, delta: event.target.checked })} />
            {t("Difference surface")}
          </label>
          <label title={t("Show the difference as a percentage of B instead of a speed")}>
            <input type="checkbox" data-feature="compare:percent" checked={percent}
              onChange={(event) => setCompareChoice(project, { percent: event.target.checked })} />
            {t("Δ as percent of B")}
          </label>
          <label className="compare-threshold"
            title={t("A cell counts as A faster or B faster only when the difference is more than this. Not saved.")}>
            {t("Threshold")}
            <input type="text" inputMode="decimal" data-feature="compare:threshold" value={shownThreshold}
              aria-label={t("Threshold")}
              onChange={(event) => setThresholdText(event.target.value)}
              onBlur={(event) => commitThreshold(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter") commitThreshold(event.currentTarget.value);
                if (event.key === "Escape") setThresholdText(null);
              }} />
            <span className="muted">{symbol}</span>
          </label>
        </fieldset>

        <section className="compare-legend" aria-label={t("Difference scale")} data-feature="compare:legend">
          <div className="compare-legend-ends">
            <span>{t("B faster")}</span><span>{t("A faster")}</span>
          </div>
          <div className="compare-legend-bar" style={{ background: legendGradient(scheme) }} />
          <div className="compare-legend-ends muted">
            <span>{delta(-Math.max(Math.abs(stats.min), Math.abs(stats.max)))}</span>
            <span>0</span>
            <span>{delta(Math.max(Math.abs(stats.min), Math.abs(stats.max)))}</span>
          </div>
          <div className="muted">
            {Number.isFinite(stats.min)
              ? t("Range: {min} to {max}", { min: delta(stats.min), max: delta(stats.max) })
              : t("No compared cell")}
          </div>
          <div className="compare-legend-single">
            <span className="compare-hatch-swatch" style={hatchStyle(scheme)} />
            {t("Only one of A and B has a value")}
          </div>
          {percent && (
            <div className="compare-legend-single">
              <span className="compare-hatch-swatch" style={{ background: POLES[scheme].single }} />
              {t("Not comparable in %: B under 0.1 kn")}
            </div>
          )}
        </section>

        <section className="compare-summary" aria-label={t("Summary")} data-feature="compare:summary">
          <h3>{t("Summary")}</h3>
          <div>{t("{count} cells compared", { count: packet.overlap })}</div>
          <div className="muted">{t("{a} only A, {b} only B (hatched)", { a: packet.aOnly, b: packet.bOnly })}</div>
          {percent && packet.pctExcluded > 0 && (
            <div className="muted">{t("{count} cells not comparable in % (B under 0.1 kn)", { count: packet.pctExcluded })}</div>
          )}
          <div>{t("Mean |Δ| {mean}", { mean: size(stats.meanAbs) })}</div>
          <div>
            {maxAt
              ? t("Max |Δ| {max} at {twa}°, {tws}", { max: size(stats.maxAbs), twa: Number(maxAt.twa.toFixed(2)), tws: windSpeed(maxAt.tws) })
              : t("Max |Δ| {max}", { max: size(stats.maxAbs) })}
          </div>
          <h4>{t("Where each is faster (more than {threshold})", { threshold: `${Number((packet.thresholdKn * factor).toFixed(3))} ${symbol}` })}</h4>
          {regions.length === 0
            ? <div className="muted">{t("Nowhere: A and B are within the threshold wherever both have a value.")}</div>
            : (
              <ul className="compare-regions">
                {regions.map((row) => (
                  <li key={row.tws}>
                    <span className="compare-region-tws">{windSpeed(row.tws)}</span>
                    <span className="compare-region-spans">
                      {row.a.length > 0 && (
                        <span className="compare-region-a">{t("A faster {spans}", { spans: row.a.map(spanText).join(", ") })}</span>
                      )}
                      {row.b.length > 0 && (
                        <span className="compare-region-b">{t("B faster {spans}", { spans: row.b.map(spanText).join(", ") })}</span>
                      )}
                    </span>
                  </li>
                ))}
              </ul>
            )}
        </section>

        <section className="compare-heat" aria-label={t("Difference heat map")} data-feature="compare:heat-map">
          <h3>{t("Difference heat map")}</h3>
          <HeatMap packet={packet} image={heat} percent={percent} scheme={scheme} unitFactor={factor} onHover={setHovered} />
          <div className="compare-heat-readout muted" aria-live="polite">
            {hovered
              ? t("TWA {twa}°, TWS {tws}: A {a}, B {b}, Δ {delta}", {
                twa: Number(hovered.twa.toFixed(2)),
                tws: windSpeed(hovered.tws),
                a: speed(hovered.a), b: speed(hovered.b),
                delta: cellDelta(hovered),
              })
              : t("Hover a cell for its values. Rows are TWA, columns TWS.")}
          </div>
        </section>
      </div>
    </div>
  );
}

function hatchStyle(scheme: Scheme) {
  const p = POLES[scheme];
  return { background: `repeating-linear-gradient(45deg, ${p.hatch} 0 1.5px, ${p.single} 1.5px 5px)` };
}

/** Up to this many cells the heat map draws a rectangle per cell; beyond, one scaled image. */
const DIRECT_CELLS = 20_000;

/**
 * The 2D Δ heat map (spec.md 11): TWA rows, TWS columns, the 0° row left
 * out, drawn on a canvas — one pixel per cell scaled up, then the hatch
 * masked onto the cells only one operand covers — so a 512 × 512 grid is
 * one image, not 262,144 elements. Hovering finds the cell by arithmetic.
 */
function HeatMap({ packet, image, percent, scheme, unitFactor, onHover }: {
  packet: ComparePacket;
  image: HeatImage;
  percent: boolean;
  scheme: Scheme;
  unitFactor: number;
  onHover: (cell: HeatCell | null) => void;
}) {
  const t = useT();
  const canvas = useRef<HTMLCanvasElement>(null);
  const [width, setWidth] = useState(300);
  useEffect(() => {
    const element = canvas.current;
    if (!element || typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(() => setWidth(element.clientWidth || 300));
    observer.observe(element);
    setWidth(element.clientWidth || 300);
    return () => observer.disconnect();
  }, []);
  const layout = useMemo(() => heatLayout(image, width), [image, width]);
  const height = Math.ceil(layout.top + image.rows * layout.cellHeight + 2);

  useEffect(() => {
    const element = canvas.current;
    const ctx = element?.getContext("2d");
    if (!element || !ctx) return;
    const ratio = Math.min(globalThis.devicePixelRatio ?? 1, 2);
    element.width = Math.round(width * ratio);
    element.height = Math.round(height * ratio);
    ctx.setTransform(ratio, 0, 0, ratio, 0, 0);
    ctx.clearRect(0, 0, width, height);
    const cellsWidth = image.cols * layout.cellWidth, cellsHeight = image.rows * layout.cellHeight;
    if (image.rows > 0 && image.cols > 0) {
      const source = (pixels: Uint8ClampedArray<ArrayBuffer>) => {
        const small = document.createElement("canvas");
        small.width = image.cols;
        small.height = image.rows;
        small.getContext("2d")?.putImageData(new ImageData(pixels, image.cols, image.rows), 0, 0);
        return small;
      };
      const p = POLES[scheme];
      const tile = document.createElement("canvas");
      tile.width = tile.height = Math.round(6 * ratio);
      const tctx = tile.getContext("2d");
      if (tctx) {
        tctx.fillStyle = p.single;
        tctx.fillRect(0, 0, tile.width, tile.height);
        tctx.strokeStyle = p.hatch;
        tctx.lineWidth = 1.5 * ratio;
        tctx.beginPath();
        tctx.moveTo(0, tile.height);
        tctx.lineTo(tile.width, 0);
        tctx.moveTo(-tile.width / 2, tile.height / 2);
        tctx.lineTo(tile.width / 2, -tile.height / 2);
        tctx.moveTo(tile.width / 2, tile.height * 1.5);
        tctx.lineTo(tile.width * 1.5, tile.height / 2);
        tctx.stroke();
      }
      if (image.rows * image.cols <= DIRECT_CELLS) {
        // Few cells: each is a rectangle, with crisp edges whatever the
        // browser does when scaling an image.
        const pattern = tctx ? ctx.createPattern(tile, "repeat") : null;
        pattern?.setTransform?.(new DOMMatrix().scale(1 / ratio));
        for (let r = 0; r < image.rows; r++) {
          for (let c = 0; c < image.cols; c++) {
            const at = (r * image.cols + c) * 4;
            const x = layout.left + c * layout.cellWidth, y = layout.top + r * layout.cellHeight;
            if (image.rgba[at + 3]) {
              ctx.fillStyle = `rgb(${image.rgba[at]}, ${image.rgba[at + 1]}, ${image.rgba[at + 2]})`;
              ctx.fillRect(x, y, layout.cellWidth, layout.cellHeight);
            } else if (image.single[at + 3] && pattern) {
              ctx.fillStyle = pattern;
              ctx.fillRect(x, y, layout.cellWidth, layout.cellHeight);
            }
          }
        }
      } else {
        // Many cells: one pixel each, scaled up, and the hatch masked onto
        // the one-only cells — one image rather than a rectangle per cell.
        ctx.imageSmoothingEnabled = false;
        ctx.drawImage(source(image.rgba), layout.left, layout.top, cellsWidth, cellsHeight);
        const hatch = document.createElement("canvas");
        hatch.width = element.width;
        hatch.height = element.height;
        const hctx = hatch.getContext("2d");
        const pattern = hctx && tctx ? hctx.createPattern(tile, "repeat") : null;
        if (hctx && pattern) {
          hctx.fillStyle = pattern;
          hctx.fillRect(0, 0, hatch.width, hatch.height);
          hctx.globalCompositeOperation = "destination-in";
          hctx.imageSmoothingEnabled = false;
          hctx.drawImage(source(image.single), layout.left * ratio, layout.top * ratio, cellsWidth * ratio, cellsHeight * ratio);
          ctx.setTransform(1, 0, 0, 1, 0, 0);
          ctx.drawImage(hatch, 0, 0);
          ctx.setTransform(ratio, 0, 0, ratio, 0, 0);
        }
      }
      // Gaps between cells while they are big enough to show them.
      if (layout.cellWidth >= 6 && layout.cellHeight >= 6) {
        ctx.strokeStyle = cssColour("--panel", "#253447");
        ctx.lineWidth = 2;
        ctx.beginPath();
        for (let c = 0; c <= image.cols; c++) {
          ctx.moveTo(layout.left + c * layout.cellWidth, layout.top);
          ctx.lineTo(layout.left + c * layout.cellWidth, layout.top + cellsHeight);
        }
        for (let r = 0; r <= image.rows; r++) {
          ctx.moveTo(layout.left, layout.top + r * layout.cellHeight);
          ctx.lineTo(layout.left + cellsWidth, layout.top + r * layout.cellHeight);
        }
        ctx.stroke();
      }
    }
    // Axis labels, thinned so they never overlap.
    ctx.fillStyle = cssColour("--muted", "#b3c9de");
    ctx.font = "9px system-ui, sans-serif";
    ctx.textAlign = "center";
    ctx.textBaseline = "alphabetic";
    const colStep = labelStep(layout.cellWidth, 22);
    for (let j = 0; j < image.cols; j += colStep) {
      ctx.fillText(`${Number((packet.tws[j]! * unitFactor).toFixed(1))}`, layout.left + (j + 0.5) * layout.cellWidth, 11);
    }
    ctx.textAlign = "right";
    ctx.textBaseline = "middle";
    const rowStep = labelStep(layout.cellHeight, 11);
    for (let r = 0; r < image.rows; r += rowStep) {
      ctx.fillText(`${Number(packet.twa[image.rowIndex[r]!]!.toFixed(1))}°`, layout.left - 4, layout.top + (r + 0.5) * layout.cellHeight);
    }
  }, [image, layout, width, height, scheme, packet, unitFactor]);

  const hover = (event: ReactPointerEvent<HTMLCanvasElement>) => {
    const rect = event.currentTarget.getBoundingClientRect();
    onHover(heatCellAt(packet, image, layout, percent, event.clientX - rect.left, event.clientY - rect.top));
  };
  return (
    <canvas ref={canvas} className="compare-heat-map" style={{ height }} role="img" aria-label={t("Difference heat map")}
      onPointerMove={hover} onPointerLeave={() => onHover(null)} />
  );
}

/**
 * One operand's picker (spec.md 11): the blend and every source, each by
 * its colour and name, a track standing for its polar segment. A native
 * list cannot show colours, so this is a button and a list box.
 */
function OperandPicker({ which, project, value, onChange }: {
  which: "a" | "b";
  project: ProjectSummary;
  value: CompareOperand;
  onChange: (operand: CompareOperand) => void;
}) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const host = useRef<HTMLDivElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);
  const list = useRef<HTMLUListElement>(null);
  useEffect(() => {
    if (!open) return;
    const close = (event: PointerEvent) => {
      if (!host.current?.contains(event.target as Node)) setOpen(false);
    };
    window.addEventListener("pointerdown", close);
    return () => window.removeEventListener("pointerdown", close);
  }, [open]);
  const info = operandInfo(project, value);
  const options: { operand: CompareOperand; colour: string; label: string; kind: string; hidden: boolean }[] = [
    { operand: { kind: "blend" }, colour: project.blend.colour, label: t("Blend"), kind: "blend", hidden: false },
    ...project.sources.map((s) => ({ operand: operandOf(s), colour: s.colour, label: s.label, kind: s.kind, hidden: !s.visible })),
  ];
  const selectedIndex = Math.max(0, options.findIndex((o) => operandKey(o.operand) === operandKey(value)));
  // The list opens with the chosen operand focused, as a native list does.
  useEffect(() => {
    if (open) list.current?.querySelectorAll<HTMLElement>('[role="option"]')[selectedIndex]?.focus();
  }, [open]);
  const letter = which === "a" ? "A" : "B";
  const kindName = (kind: string) => (kind === "track" ? t("polar segment") : kind === "blend" ? t("the current blend") : t("polar"));
  const close = () => {
    setOpen(false);
    trigger.current?.focus();
  };
  /** Arrow keys, Home and End move between options; Escape closes; Enter and Space choose (the option is a button). */
  const onListKey = (event: ReactKeyboardEvent<HTMLUListElement>) => {
    const items = [...(list.current?.querySelectorAll<HTMLElement>('[role="option"]') ?? [])];
    const at = items.indexOf(document.activeElement as HTMLElement);
    const move = (to: number) => { event.preventDefault(); items[Math.max(0, Math.min(items.length - 1, to))]?.focus(); };
    if (event.key === "ArrowDown") move(at + 1);
    else if (event.key === "ArrowUp") move(at - 1);
    else if (event.key === "Home") move(0);
    else if (event.key === "End") move(items.length - 1);
    else if (event.key === "Escape") { event.preventDefault(); event.stopPropagation(); close(); }
    else if (event.key === "Tab") setOpen(false);
  };
  return (
    <div className={`compare-picker compare-picker-${which}`} ref={host}>
      <span className="compare-picker-letter">{letter}</span>
      <button ref={trigger} className="compare-picker-button" data-feature={`compare:operand-${which}`} aria-haspopup="listbox"
        aria-expanded={open}
        title={which === "a" ? t("Operand A: a source's polar, a track's polar segment, or the blend")
          : t("Operand B: a source's polar, a track's polar segment, or the blend")}
        onClick={() => setOpen(!open)}
        onKeyDown={(event) => {
          if ((event.key === "ArrowDown" || event.key === "ArrowUp") && !open) { event.preventDefault(); setOpen(true); }
        }}>
        <span className="swatch" style={{ background: info.colour }} />
        <span className="compare-picker-name">{info.label ?? t("Blend")}</span>
        <span className="muted">{kindName(info.kind)}</span>
        <span aria-hidden="true">▾</span>
      </button>
      {open && (
        <ul ref={list} className="compare-picker-list" role="listbox" aria-label={which === "a" ? t("Operand A") : t("Operand B")}
          onKeyDown={onListKey}>
          {options.map((option, index) => {
            const selected = index === selectedIndex;
            return (
              <li key={operandKey(option.operand)} role="presentation">
                <button role="option" aria-selected={selected} tabIndex={selected ? 0 : -1}
                  className={selected ? "selected" : undefined}
                  onClick={() => { onChange(option.operand); close(); }}>
                  <span className="swatch" style={{ background: option.colour }} />
                  <span aria-hidden="true">{KIND_GLYPH[option.kind] ?? ""}</span>
                  <span className="compare-picker-name">{option.label}</span>
                  <span className="muted">{kindName(option.kind)}{option.hidden ? ` · ${t("hidden")}` : ""}</span>
                </button>
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}
