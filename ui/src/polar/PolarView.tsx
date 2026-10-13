import { useBoatApi, useBoatReadOnly } from "../boats/context";
import { useFleetSync, correspondingCell, correspondingDot, type HoverPoint } from "../boats/synchronization";
import type { BoatViewState } from "../boats/synchronization";
import WaveRangeControls from "./WaveRangeControls";
import BlendCellTooltip from "./BlendCellTooltip";
import PolarDotTooltip from "./PolarDotTooltip";
import { useLiveEdit } from "../panels/useLiveEdit";
import PriorityFilters from "../panels/PriorityFilters";
import { SampleFiltersEditor } from "../panels/Tracks";
import { NO_SAMPLE_FILTERS } from "../panels/sampleFilters";
import { DEFAULT_UNITS } from "../panels/filterUnits";
import { useCallback, useEffect, useMemo, useRef, useState, type PointerEvent as ReactPointerEvent } from "react";

import { registerPolarInspection, registerRedraw } from "../automation";
import { registerCapture } from "../mcp/capture";
import { needsOutline } from "../colourContrast";
import { reportFailure } from "../errors";
import type { AppSettings } from "../generated/AppSettings";
import type { BlendCell } from "../generated/BlendCell";
import type { PolarPdfResult } from "../generated/PolarPdfResult";
import type { ProjectSummary } from "../generated/ProjectSummary";
import { later, setHint } from "../hint";
import DayBandLegend from "../DayBandLegend";
import { msg, useT } from "../i18n";

import { useSampleSelection, useBoatSelection } from "../selection";
import { onThemeChange } from "../settings/themes";
import EditPanel, { cellCode } from "./EditPanel";
import { useEditFocus, useBoatEditing } from "./editFocus";
import { place, type Layout } from "./geometry3d";
import { PolarScene, type SurfaceHit, type View } from "./scene3d";
import {
  bucketCentre, CELL_NONE, cellRect, cellTotals, correspondingInCells, uncrowded, waveCells, WAVE_SPLIT_COUNTS, type Rect, type WaveSense,
} from "./waveSplit";
import { emptyScene, ScenePacketError, type ScenePacket, type SplitPacket } from "./scenePacket";
import {
  availableModes, buildGuides, buildSurfaces, combine, DEFAULT_TOGGLES, drawnOnly, editCells, emptyKeys,
  buildPdfDensity, exclusionTargets, modeValues, focusIndex, hasExcluded, hasFiltered, keysOf, mergeDots, nearestIndex, nodeDots, nodesAtCells, sampleDots, presetView, range, rescaleView, resolveKeys, sampleIdsOf, sceneBounds,
  SPEED_FACTOR, SPEED_SYMBOL, summarise, type CameraPreset, type ColourMode, type Focus, type GuideLabel, type SelectionKeys,
  type Bounds, type Toggles,
} from "./view3d";

type Tool = "rotate" | "lasso" | "box" | "drag";
type DisplayMode = "points" | "pdf";

/** Shift snaps a dragged node to this step, knots (spec.md 10.4). */
export const DRAG_SNAP_KN = 0.05;
/** A drag along a BSP axis seen end-on moves this many pixels per knot, straight up. */
const FALLBACK_PX_PER_KN = 20;

/**
 * The boat speed a node dragged by (`dx`, `dy`) pixels reaches: the drag
 * projected on the screen direction of one knot more BSP (`axis`, pixels),
 * snapped to 0.05 kn with Shift, and kept to 0–60 kn.
 */
export function draggedSpeed(start: number, axis: readonly [number, number], dx: number, dy: number, snap: boolean): number {
  let [ax, ay] = axis;
  if (Math.hypot(ax, ay) < 5) [ax, ay] = [0, -FALLBACK_PX_PER_KN];
  let value = start + (dx * ax + dy * ay) / (ax * ax + ay * ay);
  if (snap) value = Math.round(value / DRAG_SNAP_KN) * DRAG_SNAP_KN;
  return Math.round(Math.min(60, Math.max(0, value)) * 1000) / 1000;
}

/** A pointer that moved less than this, in pixels, clicked rather than dragged. */
const CLICK_SLOP_PX = 4;
/** A click picks the nearest dot within this many pixels. */
const PICK_RADIUS_PX = 8;

const LAYOUT_NAMES: Record<Layout, string> = { tower: msg("Polar tower"), cartesian: msg("Cartesian") };
const MODE_NAMES: Record<ColourMode, string> = {
  source: msg("By source"), hs: msg("By wave height"),
  wavePeriod: msg("By wave period"), waveAngle: msg("By wave angle"), waveWindAngle: msg("By wave angle to wind"),
  current: msg("By current speed"), time: msg("By time"), timeOfDay: msg("By time of day"),
};
const CAMERAS: readonly { id: CameraPreset; label: string; tip: string }[] = [
  { id: "top", label: msg("Top"), tip: msg("Look down the wind-speed axis: the classic polar diagram") },
  { id: "side", label: msg("Side"), tip: msg("Look across the wind-speed axis: each wind speed a level") },
  { id: "iso", label: msg("Isometric"), tip: msg("The three-quarter view") },
];
const TOOLS: readonly { id: Tool; label: string; tip: string }[] = [
  { id: "rotate", label: msg("Rotate"), tip: msg("Drag to turn the view, right-drag to pan, scroll to zoom; click a dot to select it") },
  { id: "lasso", label: msg("Lasso"), tip: msg("Draw round dots to select them; Shift adds to the selection") },
  { id: "box", label: msg("Box"), tip: msg("Drag a box round dots to select them; Shift adds to the selection") },
  { id: "drag", label: msg("Drag"), tip: msg("Drag a node of the source being edited to change its boat speed; Shift snaps to 0.05 kn") },
];

let dragGestures = 0;

/** A direction in whole degrees where it is whole, to a tenth where it is not (22.5°). */
const degrees = (angle: number) => (Number.isInteger(angle) ? String(angle) : angle.toFixed(1));

/** Standard normal CDF, using a compact erf approximation for hover feedback. */
function normalCdf(value: number): number {
  const sign = value < 0 ? -1 : 1;
  const x = Math.abs(value) / Math.SQRT2;
  const t = 1 / (1 + 0.3275911 * x);
  const erf = 1 - (((((1.061405429 * t - 1.453152027) * t) + 1.421413741) * t - 0.284496736) * t + 0.254829592) * t * Math.exp(-x * x);
  return 0.5 * (1 + sign * erf);
}

/** Percentile within the displayed Gaussian truncated to the supported SOG range. */
function pdfPercentileAt(sog: number, stats: { mean: number; stddev: number; mass: number }): number {
  const below = normalCdf((sog - stats.mean) / stats.stddev) - normalCdf(-stats.mean / stats.stddev);
  return Math.max(0, Math.min(100, (below / stats.mass) * 100));
}

/**
 * The big arrow of a Split Wave Angle copy (spec.md 10.5): a boat seen from
 * above, bow up, and the waves against it. From: the arrow comes in from
 * the copy's direction to the boat. To: it leaves the boat toward it.
 */
/** A copy's blend at the output-grid cell nearest a wind, or null where it has none. */
function copyBsp(split: SplitPacket, cell: number, twa: number, tws: number): number | null {
  const surface = split.surfaces.find((s) => s.cell === cell);
  if (!surface) return null;
  const i = nearestIndex(surface.twa, twa), j = nearestIndex(surface.tws, tws);
  if (i < 0 || j < 0) return null;
  const bsp = surface.bsp[i * surface.tws.length + j]!;
  return Number.isFinite(bsp) ? bsp : null;
}

/** A copy narrower than this (CSS px) is drawn compactly, without axis labels. */
const SMALL_COPY = 150;
const NO_DOT_CELLS = new Int16Array(0);
const NO_PDF_DOTS = { points: new Float32Array(0), colors: new Float32Array(0), shapes: new Float32Array(0) };
const EMPTY_PDF = { volumes: [], count: 0, maxDensity: 0, cells: [] };

/**
 * A copy's glyph: a boat drawn bow up, the big arrow of its waves (pointing
 * at the boat from their direction, or away from it where they go), and
 * round the boat an arc of the directions the copy holds — its share of
 * the circle, `width` degrees centred on `direction` — on a faint ring.
 */
function WaveArrow({ direction, width, sense }: { direction: number; width: number; sense: WaveSense }) {
  const from = ((direction - width / 2) % 360 + 360) % 360, to = (from + width) % 360;
  return (
    <svg className="wave-split-arrow" viewBox="-52 -52 104 104" aria-hidden="true" data-direction={degrees(direction)} data-sense={sense}>
      <circle className="wave-split-ring" r={ARC_RADIUS} />
      <path className="wave-split-arc" d={arcPath(direction - width / 2, direction + width / 2)}
        data-from={degrees(from)} data-to={degrees(to)} />
      <path className="wave-split-boat" d="M0,-12 C5,-5 5,7 3,12 L-3,12 C-5,7 -5,-5 0,-12 Z" />
      <g transform={`rotate(${direction})`}>
        {sense === "from"
          ? <path className="wave-split-shaft" d="M-6,-50 L6,-50 L6,-36 L15,-36 L0,-23 L-15,-36 L-6,-36 Z" />
          : <path className="wave-split-shaft" d="M-6,-23 L6,-23 L6,-37 L15,-37 L0,-50 L-15,-37 L-6,-37 Z" />}
      </g>
    </svg>
  );
}

const ARC_RADIUS = 17;

/** An SVG arc on the ring from one bearing to another, clockwise, degrees from the bow. */
function arcPath(from: number, to: number): string {
  const at = (bearing: number) => {
    const a = (bearing * Math.PI) / 180;
    return `${(ARC_RADIUS * Math.sin(a)).toFixed(2)},${(-ARC_RADIUS * Math.cos(a)).toFixed(2)}`;
  };
  return `M${at(from)} A${ARC_RADIUS},${ARC_RADIUS} 0 ${to - from > 180 ? 1 : 0} 1 ${at(to)}`;
}

/**
 * The part of the canvas a grid of copies may use: what the toolbar, the
 * view's own side panel and wave filters, and the workspace's docked panels
 * leave free, so no copy is drawn behind a control. A stage too small to
 * measure (or not laid out at all, as in a test) is used whole.
 */
function freeRegion(canvas: HTMLCanvasElement): Rect {
  const stage = canvas.getBoundingClientRect();
  const whole = { x: 0, y: 0, width: Math.max(1, stage.width), height: Math.max(1, stage.height) };
  const view = canvas.parentElement;
  if (!view) return whole;
  let left = 0, right = stage.width, top = 0, bottom = stage.height;
  const box = (root: Element | null | undefined, selector: string) => root?.querySelector(selector)?.getBoundingClientRect() ?? null;
  const workspace = view.closest(".workspace");
  const dockLeft = box(workspace, ".sidebar.left"), dockRight = box(workspace, ".sidebar.right");
  if (dockLeft && dockLeft.width > 0) left = Math.max(left, dockLeft.right - stage.left);
  if (dockRight && dockRight.width > 0) right = Math.min(right, dockRight.left - stage.left);
  const toolbar = box(view, ".view3d-toolbar"), side = box(view, ".view3d-side"), waves = box(view, ".wave-display-ranges");
  if (toolbar && toolbar.height > 0) top = Math.max(top, toolbar.bottom - stage.top + 6);
  if (side && side.width > 0) right = Math.min(right, side.left - stage.left - 6);
  if (waves && waves.height > 0) bottom = Math.min(bottom, waves.top - stage.top - 6);
  const free = { x: left + 4, y: top, width: right - left - 8, height: bottom - top };
  return free.width >= 80 && free.height >= 80 ? free : whole;
}

function cssColour(name: string, fallback: string): string {
  if (typeof document === "undefined") return fallback;
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim() || fallback;
}

/**
 * The 3D polar stage (spec.md 10.1–10.3): every visible polar source as a
 * translucent surface over its own grid, its nodes and every sample as dots,
 * in the polar-tower or Cartesian layout, with preset cameras, an axis
 * legend in the display units, and selection by click, Shift-click, lasso
 * or box. Exclude and Include are undoable changes made in Rust; the view
 * refetches the scene whenever the project's revision moves.
 *
 * The WebGL renderer, its geometries and the orbit controls are disposed on
 * unmount, as the map disposes its own.
 */
export default function PolarView({ project, settings, onProject, compact = false, comparison = false, active = false, viewState }: {
  compact?: boolean;
  /** A pane of split or four-way view: no Split Wave Angle there (spec.md 10.5). */
  comparison?: boolean;
  /** The pane whose starting camera establishes a linked comparison view. */
  active?: boolean;
  /** Fleet-owned camera and axes layout, retained while this canvas unmounts. */
  viewState?: BoatViewState;
  project: ProjectSummary;
  settings: AppSettings | null;
  onProject: (summary: ProjectSummary) => void;
}) {
  const internalViewState = useRef<BoatViewState>({ camera: null, layout: null });
  const savedViewState = viewState ?? internalViewState.current;
  const savedViewStateRef = useRef(savedViewState);
  savedViewStateRef.current = savedViewState;
  const { editSource } = useBoatEditing();
  const { selectSamples, focusMap } = useBoatSelection();
  const api = useBoatApi();
  const readOnly = useBoatReadOnly();
  const sync = useFleetSync();
  const syncRef = useRef(sync);
  syncRef.current = sync;
  const linkedLayout = useRef<Layout>("tower");
  const t = useT();
  const canvas = useRef<HTMLCanvasElement>(null);
  const labelsHost = useRef<HTMLDivElement>(null);
  const scene = useRef<PolarScene | null>(null);
  /** This boat's own camera, kept while the fleet cameras are linked. */
  const singleView = useRef<View | null>(savedViewState.camera);
  /** And the axes arrangement that goes with that camera. */
  const singleLayout = useRef<Layout | null>(savedViewState.layout);
  const restoringSingleView = useRef(false);
  const comparisonRef = useRef(comparison);
  comparisonRef.current = comparison;
  const activeRef = useRef(active);
  activeRef.current = active;
  const boundsRef = useRef<Bounds | null>(null);
  const frame = useRef(0);
  const hoverFrame = useRef<ReturnType<typeof setTimeout> | null>(null);
  const hoverPoint = useRef<{ x: number; y: number; width: number; height: number } | null>(null);
  const labels = useRef<GuideLabel[]>([]);
  const cellsRef = useRef<{ cells: Int16Array | null; waveCount: number; region: Rect }>({ cells: null, waveCount: 0, region: { x: 0, y: 0, width: 1, height: 1 } });
  const gesture = useRef<{ x: number; y: number; path: number[] } | null>(null);
  const request = useRef(0);
  const fitted = useRef(false);
  /** The box the camera was last framed for, so a new one can be followed. */
  const framedFor = useRef<Bounds | null>(null);
  const selectionKeys = useRef<SelectionKeys>(emptyKeys());
  const shared = useSampleSelection();
  const rememberSingleView = (view: View) => {
    singleView.current = view;
    savedViewStateRef.current.camera = view;
  };

  const [unavailable, setUnavailable] = useState<string | null>(null);
  const [packet, setPacket] = useState<ScenePacket>(emptyScene);
  const [layout, setLayout] = useState<Layout>(() => savedViewState.layout ?? "tower");
  linkedLayout.current = layout;
  const [noHoverMatch, setNoHoverMatch] = useState(false);
  const [toggles, setToggles] = useState<Toggles>(DEFAULT_TOGGLES);
  const [waveRanges, updateWaveRanges] = useLiveEdit(project.blend.wave_ranges, (ranges) => api.setWaveRanges(ranges).then(onProject), project.id);
  const [mode, setMode] = useState<ColourMode>("source");
  const [displayMode, setDisplayMode] = useState<DisplayMode>("points");
  const [pdfFit, setPdfFit] = useState<PolarPdfResult | null>(null);
  const [pdfFocus, setPdfFocus] = useState<{ windCell: number; copy: number } | null>(null);
  const [pdfHover, setPdfHover] = useState<({ windCell: number; copy: number; sog: number; x: number; y: number }) | null>(null);
  const [tool, setTool] = useState<Tool>("rotate");
  const [selection, setSelection] = useState<number[]>([]);
  const [path, setPath] = useState<number[] | null>(null);
  const [hover, setHover] = useState<{ index: number; x: number; y: number } | null>(null);
  // The blend cell under the pointer (spec.md 10.1). `blendAsked` names the
  // cell last asked of Rust and holds its answer, so moving within one cell
  // asks nothing more, and an answer for a cell since left is dropped.
  const [blendHover, setBlendHover] = useState<{ cell: BlendCell; x: number; y: number } | null>(null);
  const pdfViewBeforeFocus = useRef<View | null>(null);
  const pdfPointerDown = useRef<{ x: number; y: number } | null>(null);
  const blendAsked = useRef<{ key: string; cell: BlendCell | null }>({ key: "", cell: null });
  const clearBlendHover = useCallback(() => {
    blendAsked.current = { key: "", cell: null };
    setBlendHover(null);
  }, []);
  const clearHover = useCallback(() => {
    if (hoverFrame.current !== null) clearTimeout(hoverFrame.current);
    hoverFrame.current = null;
    hoverPoint.current = null;
    setHover(null);
    setPdfHover(null);
    setNoHoverMatch(false);
    setMarks([]);
    clearBlendHover();
  }, [clearBlendHover]);
  // Split Wave Angle (spec.md 10.5): the view as one copy per wave direction.
  const [waveSplit, setWaveSplit] = useState(false);
  const [waveCountIndex, setWaveCountIndex] = useState(1);
  const [waveSense, setWaveSense] = useState<WaveSense>("from");
  const splitOn = waveSplit && !comparison;
  const waveCount = WAVE_SPLIT_COUNTS[waveCountIndex] ?? WAVE_SPLIT_COUNTS[0];
  /** Each copy's own blend, made from its direction's samples; asked of Rust for every scene the view holds. */
  const [splitBlends, setSplitBlends] = useState<SplitPacket | null>(null);
  useEffect(() => {
    if (!splitOn) { setSplitBlends(null); return; }
    let live = true;
    api.polarSceneSplit({ count: waveCount, sense: waveSense })
      .then((found) => { if (live) setSplitBlends(found); })
      // A split that cannot be read (the project closed under it) draws no copy's blend.
      .catch(() => { if (live) setSplitBlends(null); });
    return () => { live = false; };
  }, [splitOn, waveCount, waveSense, packet]);
  const [region, setRegion] = useState<Rect>({ x: 0, y: 0, width: 1, height: 1 });
  /** For a dot or blend cell hovered in one copy: the same wind in the others, each with its speed. */
  const [marks, setMarks] = useState<{ cell: number; x: number; y: number; bsp: number }[]>([]);
  const [hideOthers, setHideOthers] = useState(false);
  const [dragValue, setDragValue] = useState<{ x: number; y: number; bsp: number } | null>(null);
  const held = useRef<ScenePacket | null>(null);
  const drag = useRef<{
    start: number; axis: [number, number]; x: number; y: number; cell: { twa_index: number; tws_index: number };
    gesture: string; busy: boolean; pending: number | null; source: number;
  } | null>(null);
  const unit = settings?.units.speed ?? "kn";
  const requested = useEditFocus();
  // Edit mode needs the source to exist: a removed source ends it.
  const focus = requested !== null && project.sources.some((s) => s.id === requested) ? requested : null;
  useEffect(() => { if (requested !== null && focus === null) editSource(null); }, [requested, focus]);

  /** Renders now, and moves the axis labels to where their points now are. */
  const paint = useCallback(() => {
    const current = scene.current;
    if (!current) return;
    current.render();
    const host = labelsHost.current;
    if (!host) return;
    const places = labels.current.map((label) => current.toScreen(...label.at));
    // In a copy of the view the labels have a fraction of the room: the ones
    // that would be written over another are left out (about 6 px a letter).
    // Copies too small for them have none: the frame's own words need the room.
    const split = cellsRef.current;
    const keep = !split.cells ? null
      : cellRect(0, split.waveCount, split.region).width < SMALL_COPY ? places.map(() => false)
      : uncrowded(places.map((at, k) => at && { x: at[0], y: at[1], width: labels.current[k]!.text.length * 6 + 4, height: 12 }));
    places.forEach((at, k) => {
      const span = host.children[k] as HTMLElement | undefined;
      if (!span) return;
      const shown = at !== null && (keep === null || keep[k]!);
      span.style.display = shown ? "" : "none";
      if (at) span.style.transform = `translate(${at[0]}px, ${at[1]}px)`;
    });
  }, []);

  /** Paints on the next frame, once. */
  const draw = useCallback(() => {
    if (frame.current !== 0) return;
    frame.current = requestAnimationFrame(() => {
      frame.current = 0;
      paint();
    });
  }, [paint]);

  // The scene lives as long as the stage: made on mount, disposed on unmount.
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
    // The WebDriver screenshot's synchronous redraw (development builds only).
    const offRedraw = registerRedraw(element, paint);
    // The MCP service's screenshot reads this canvas (every build).
    const offCapture = registerCapture(element, paint);
    const offInspection = registerPolarInspection(element, () => ({
      view: made.getView(), points: Array.from(made.projected()),
      // Where a (TWA, TWS, BSP) falls on the canvas in copy `cell`, and what the surface pick answers at a canvas point.
      at: (cell: number, twa: number, tws: number, bsp: number) => made.toScreenIn(cell, ...place(twa, tws, bsp, linkedLayout.current)),
      surfaceAt: (x: number, y: number) => made.pickSurface(x, y),
    }));
    made.setBackground(cssColour("--inset", "#1f2c3c"));
    made.enableControls(element, draw, () => {
      const view = made.getView();
      if (!comparisonRef.current) rememberSingleView(view);
      syncRef.current?.publish({ kind: "camera", boat: project.id, view, layout: linkedLayout.current });
    });
    const initialView = (comparisonRef.current ? syncRef.current?.camera?.view : savedViewStateRef.current.camera)
      ?? presetView("top", sceneBounds(emptyScene(), "tower"), made.camera.fov, "origin");
    made.setView(initialView);
    if (!comparisonRef.current) rememberSingleView(initialView);
    const resize = () => {
      // Hidden with its stage (kept mounted): keep the size it had.
      if (element.clientWidth === 0 || element.clientHeight === 0) return;
      made.resize(element.clientWidth, element.clientHeight);
      draw();
    };
    resize();
    const observer = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(resize);
    observer?.observe(element);
    const offTheme = onThemeChange(() => {
      made.setBackground(cssColour("--inset", "#1f2c3c"));
      draw();
    });
    return () => {
      observer?.disconnect();
      offTheme();
      offRedraw();
      offCapture();
      offInspection();
      cancelAnimationFrame(frame.current);
      frame.current = 0;
      made.dispose();
      scene.current = null;
    };
  }, [draw, paint]);

  // A new project is framed afresh when its first dots arrive.
  useEffect(() => { fitted.current = false; framedFor.current = null; }, [project.id]);

  // Refetches on every document change (`revision` moves with every command,
  // undo and redo included) and on switching project.
  // Only the samples' flags travel when no sample moved (the scene held
  // names its samples key); a scene that no longer matches is fetched whole.
  // A scene held from another project is never a base: it is dropped in the
  // same effect, before the fetch, so no effect order can send its key.
  const heldProject = useRef<number | null>(null);
  useEffect(() => {
    const id = ++request.current;
    if (heldProject.current !== project.id) {
      held.current = null;
      heldProject.current = project.id;
    }
    const base = held.current;
    void api.polarScene(focus, base)
      .catch((error: unknown) => {
        if (error instanceof ScenePacketError && base !== null) return api.polarScene(focus, null);
        throw error;
      })
      .then((next) => {
        if (request.current !== id) return;
        held.current = next;
        setPacket(next);
      })
      .catch((error: unknown) => { if (request.current === id) reportFailure(error); });
  }, [project.id, project.revision, focus]);

  // Leaving edit mode puts the Drag tool down.
  useEffect(() => {
    if (focus === null && tool === "drag") chooseTool("rotate");
  }, [focus]);

  /**
   * Sets the selection, remembering it by stable keys for the next refetch,
   * and shares its samples with the map (spec.md 9.1).
   */
  const select = useCallback((next: number[]) => {
    selectionKeys.current = keysOf(packet, next);
    setSelection(next);
    selectSamples(sampleIdsOf(packet, next), "3d");
  }, [packet]);

  // A refetched scene keeps the selection: dots are matched by source and
  // cell, or sample id, not by position in the buffer.
  useEffect(() => {
    setSelection(resolveKeys(packet, selectionKeys.current));
  }, [packet]);

  // A selection made on the map (or elsewhere) selects those samples here.
  useEffect(() => {
    if (shared.origin === "3d" || shared.origin === null) return;
    selectionKeys.current = { nodes: new Map(), samples: new Set(shared.ids) };
    setSelection(resolveKeys(packet, selectionKeys.current));
    // Only a new shared selection re-selects; a refetch keeps its own.
  }, [shared.version]);

  // The axes follow what is shown: excluded and filtered-out points count
  // while their checkbox is ticked (asked 2026-10-04).
  const bounds = useMemo(
    () => sceneBounds(packet, layout, waveRanges, toggles),
    [packet, layout, waveRanges, toggles.excluded, toggles.filtered],
  );
  boundsRef.current = bounds;
  // The active boat chooses a fresh isometric starting camera as comparison
  // begins. Every pane receives that same view; each keeps its own saved
  // single-view camera to restore when comparison ends.
  useEffect(() => {
    const current = scene.current;
    if (!current) return;
    if (comparison) {
      if (activeRef.current) {
        const view = presetView("iso", boundsRef.current ?? sceneBounds(emptyScene(), "tower"), current.camera.fov, "origin");
        if (!singleView.current) rememberSingleView(current.getView());
        if (!singleLayout.current) {
          singleLayout.current = layout;
          savedViewStateRef.current.layout = layout;
        }
        current.setView(view);
        syncRef.current?.publish({ kind: "camera", boat: project.id, view, layout: linkedLayout.current });
      }
    } else if (singleView.current) {
      restoringSingleView.current = singleLayout.current !== null && singleLayout.current !== layout;
      current.setView(singleView.current);
      if (singleLayout.current) {
        setLayout(singleLayout.current);
        savedViewStateRef.current.layout = singleLayout.current;
      }
      draw();
    }
  // Deliberately run only when the fleet enters or leaves comparison mode.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [comparison]);
  const modes = useMemo(() => availableModes(packet), [packet]);
  const filteredExist = useMemo(() => hasFiltered(packet), [packet]);
  const excludedExist = useMemo(() => hasExcluded(packet), [packet]);
  // A mode whose data has gone (the tracks were removed) falls back to source colours.
  const shownMode: ColourMode = modes[mode] ? mode : "source";
  const focused = useMemo(() => focusIndex(packet, focus), [packet, focus]);
  // The blend surface is drawn in the Blend entry's colour (spec.md 8; M7
  // carry), opaque (spec.md 10.1).
  const blendColour = project.blend.colour;
  // A blend colour lost on the background (a white blend on Paper) keeps
  // its grid lines in the theme's text colour, as an outline.
  const blendLine = needsOutline(blendColour, cssColour("--inset", "#1f2c3c")) ? cssColour("--text", "#d6e6f5") : undefined;
  const focusStyle: Focus | null = useMemo(
    () => (focus === null ? null : { index: focused, hideOthers }),
    [focus, focused, hideOthers],
  );
  const surfaces = useMemo(
    () => (displayMode === "points" && toggles.surfaces ? buildSurfaces(packet, blendColour, focusStyle, blendLine, splitOn ? splitBlends : null) : []),
    [packet, blendColour, focusStyle, blendLine, toggles.surfaces, splitOn, splitBlends, displayMode],
  );
  /**
   * Shows a cell of this boat's output grid, asked of Rust once per cell and
   * per revision. `where` places the tooltip once the cell is known: at the
   * pointer in the pane being hovered, at the cell itself in a linked one.
   */
  const showBlendCell = (i: number, j: number, where: (cell: BlendCell) => { x: number; y: number }, copy?: number) => {
    const split = copy === undefined ? "" : `:${waveCount}:${waveSense}:${copy}`;
    const key = `${project.id}:${project.revision}:${i}:${j}${split}`;
    if (blendAsked.current.key === key) {
      const cell = blendAsked.current.cell;
      if (cell) setBlendHover({ cell, ...where(cell) });
      return;
    }
    blendAsked.current = { key, cell: null };
    void (copy === undefined ? api.blendCell(i, j) : api.blendCellSplit(i, j, { count: waveCount, sense: waveSense }, copy))
      .then((cell) => {
        if (blendAsked.current.key !== key) return;
        blendAsked.current = { key, cell };
        setBlendHover({ cell, ...where(cell) });
      })
      // A cell that cannot be read (the grid changed under the pointer) shows nothing.
      .catch(() => { if (blendAsked.current.key === key) clearBlendHover(); });
  };
  /**
   * Shows the blend cell under a surface hit: the output-grid cell nearest
   * the hit node (the drawn surface is finer than the grid in spline mode).
   * Answers the cell's wind, for the panes linked to this one.
   */
  const hoverBlend = (hit: SurfaceHit, x: number, y: number): HoverPoint | null => {
    const surface = surfaces[hit.surface];
    const grid = surface?.grid;
    const i = grid ? nearestIndex(project.blend.twa, grid.twa[hit.twaIndex]!) : -1;
    const j = grid ? nearestIndex(project.blend.tws, grid.tws[hit.twsIndex]!) : -1;
    if (i < 0 || j < 0) { clearBlendHover(); return null; }
    showBlendCell(i, j, (found) => {
      // In a split view the same cell is marked in the other copies, each
      // with its own blend's speed there, so the eye finds the same place
      // in each and sees how the waves change it.
      const current = scene.current;
      const own = current?.cellAt(x, y) ?? -1;
      if (cells && current) {
        const next: typeof marks = [];
        for (let k = 0; k < waveCount; k++) {
          if (k === own) continue;
          const bsp = splitBlends ? copyBsp(splitBlends, k, found.twa, found.tws) : found.bsp;
          if (bsp === null) continue;
          const at = current.toScreenIn(k, ...place(found.twa, found.tws, bsp, layout));
          if (at) next.push({ cell: k, x: at[0], y: at[1], bsp });
        }
        setMarks(next);
      }
      return { x, y };
    }, surface?.cell);
    return { twa: project.blend.twa[i]!, tws: project.blend.tws[j]! };
  };
  /**
   * The blend cell hovered in a linked pane (split and four-way view), shown
   * here for this boat: its cell at the same wind, the tooltip beside that
   * cell on this boat's own surface. Read through a ref by the subscription,
   * which is made once per pane.
   */
  const showLinkedBlend = (point: HoverPoint | null) => {
    const cell = point && correspondingCell(project.blend.twa, project.blend.tws, point);
    if (!cell) { clearBlendHover(); return; }
    showBlendCell(cell.twa, cell.tws, (found) => {
      const size = canvas.current?.getBoundingClientRect();
      const at = found.bsp === null ? null : scene.current?.toScreen(...place(found.twa, found.tws, found.bsp, layout)) ?? null;
      // A cell with no speed has no place on the surface: the corner, as the linked dots' tooltip.
      if (!at || !size) return { x: 12, y: 58 };
      return { x: Math.max(8, Math.min(at[0] + 12, size.width - 280)), y: Math.max(8, Math.min(at[1] + 12, size.height - 260)) };
    });
  };
  const showLinkedBlendRef = useRef(showLinkedBlend);
  showLinkedBlendRef.current = showLinkedBlend;
  // The samples' dots are rebuilt only when they, their colours or how they
  // are shown change: an edit of a polar node rebuilds the nodes alone
  // (spec.md 13, plan.md M13).
  const colourKey = packet.sources.map((s) => `${s.id}${s.colour}`).join(",");
  const sampleDotsBuilt = useMemo(
    () => sampleDots(packet, toggles, shownMode, focusStyle, waveRanges),
    [packet.samples, colourKey, packet.nodes.count, toggles, shownMode, focusStyle, waveRanges],
  );
  const dots = useMemo(
    () => mergeDots(nodeDots(packet, toggles, focusStyle), sampleDotsBuilt),
    [packet, toggles, focusStyle, sampleDotsBuilt],
  );
  // The copy of every drawn dot, and how many samples each copy holds. Made
  // from the dots drawn, so the show toggles, the track and global filters
  // and the wave ranges all still decide what there is to split.
  const cells = useMemo(
    () => (splitOn ? waveCells(packet.nodes.count, packet.samples.waveBearing, dots.refs, waveCount, waveSense) : null),
    [splitOn, packet, dots, waveCount, waveSense],
  );
  useEffect(() => {
    let cancelled = false;
    setPdfFit(null);
    if (displayMode !== "pdf") return;
    void api.polarPdf({ samples: toggles.samples, nodes: toggles.nodes,
      filtered: toggles.filtered, excluded: toggles.excluded,
      source: hideOthers ? focus : null,
      split: splitOn ? { count: waveCount, sense: waveSense } : null,
    }).then((next) => { if (!cancelled) setPdfFit(next); })
      .catch((error: unknown) => { if (!cancelled) reportFailure(error); });
    return () => { cancelled = true; };
  }, [api, project.id, project.revision, displayMode, toggles.samples, toggles.nodes,
    toggles.filtered, toggles.excluded, hideOthers, focus, splitOn, waveCount, waveSense]);
  const pdf = useMemo(
    () => displayMode === "pdf" ? buildPdfDensity(pdfFit, splitOn, pdfFocus) : EMPTY_PDF,
    [displayMode, pdfFit, splitOn, pdfFocus],
  );
  const focusedPdfStats = pdfFocus
    ? pdf.cells.find((cell) => cell.windCell === pdfFocus.windCell && cell.copy === pdfFocus.copy) ?? null
    : null;
  const hoveredPdfStats = pdfHover
    ? pdf.cells.find((cell) => cell.windCell === pdfHover.windCell && cell.copy === pdfHover.copy) ?? null
    : null;
  const displayDots = displayMode === "pdf" ? NO_PDF_DOTS : dots;
  const totals = useMemo(() => (cells ? cellTotals(cells, waveCount) : null), [cells, waveCount]);
  // The samples drawn in no copy: their waves' direction is not known.
  const unplaced = useMemo(() => (cells ? cells.reduce((n, cell) => n + (cell === CELL_NONE ? 1 : 0), 0) : 0), [cells]);
  const displayCells = displayMode === "pdf" ? (splitOn ? NO_DOT_CELLS : null) : cells;
  cellsRef.current = { cells: displayCells, waveCount, region };
  useEffect(() => {
    if (!sync) { setNoHoverMatch(false); return; }
    return sync.subscribe(event => {
      if (displayMode === "pdf" && event.kind !== "camera") return;
      if (event.boat === project.id) return;
      if (event.kind === "camera") {
        clearHover();
        setLayout(event.layout);
        scene.current?.setView(event.view);
        draw();
      } else if (event.kind === "blend") {
        showLinkedBlendRef.current(event.point);
      } else if (event.point === null) {
        setHover(null); setNoHoverMatch(false);
      } else {
        const local = correspondingDot(dots.points, event.point);
        setNoHoverMatch(local < 0);
        if (local < 0) { setHover(null); return; }
        // Beside the dot itself, as this pane's own hover would be (asked
        // 2026-10-02); the corner only when the dot is behind the camera.
        const screen = scene.current?.projected();
        const size = canvas.current?.getBoundingClientRect();
        const x = screen?.[local * 2], y = screen?.[local * 2 + 1];
        const at = x !== undefined && y !== undefined && Number.isFinite(x) && Number.isFinite(y)
          ? size && size.width > 0
            ? { x: Math.max(8, Math.min(x + 12, size.width - 280)), y: Math.max(8, Math.min(y + 12, size.height - 260)) }
            : { x: x + 12, y: y + 12 }
          : { x: 12, y: 58 };
        setHover({ index: dots.refs[local]!, ...at });
      }
    });
  }, [sync, project.id, dots, displayMode, draw, clearHover]);
  useEffect(() => { clearHover(); return () => { if (hoverFrame.current !== null) clearTimeout(hoverFrame.current); }; }, [dots, layout, displayMode, clearHover]);
  // Only what is drawn is counted and acted on: a dot the toggles hide
  // cannot be seen to be selected.
  const acting = useMemo(
    () => drawnOnly(selection, dots.refs, packet.nodes.count + packet.samples.count),
    [selection, dots, packet],
  );

  useEffect(() => {
    const current = scene.current;
    if (!current) return;
    current.setData({ samples: displayDots.points, colors: displayDots.colors, shapes: displayDots.shapes, layout, surfaces,
      pdfVolumes: displayMode === "pdf" ? pdf.volumes : [] });
    // New points are in every copy until told otherwise: say whose they are.
    const held = cellsRef.current;
    current.setCells(held.cells && held.cells.length === displayDots.points.length / 3
      ? { of: held.cells, count: held.waveCount, region: held.region } : null);
    const guides = buildGuides(bounds, layout, unit, project.blend.asymmetric);
    current.setGuides(guides.segments, cssColour("--muted", "#b3c9de"));
    labels.current = guides.labels;
    const host = labelsHost.current;
    if (host) {
      host.replaceChildren(...guides.labels.map((label) => {
        const span = document.createElement("span");
        span.textContent = label.text;
        // In a split view the next paint decides which labels have room:
        // none shows before it, even while frames are held back.
        if (cellsRef.current.cells) span.style.display = "none";
        return span;
      }));
    }
    if (!fitted.current && (packet.nodes.count > 0 || packet.samples.count > 0)) {
      // The origin in the middle of what the panels and the view's own
      // controls leave in sight, and all of the polar there (asked
      // 2026-10-02 and 2026-10-03).
      // Once: opening or closing a panel later leaves the framing alone.
      const element = canvas.current;
      const size = element?.getBoundingClientRect();
      const region = element && size && size.width > 0 ? freeRegion(element) : null;
      const linked = syncRef.current?.camera?.view;
      let view = linked ?? presetView("top", bounds, current.camera.fov, "origin");
      if (region && size) {
        current.setViewCentre(region.x + region.width / 2, region.y + region.height / 2);
        const shrink = size.height / Math.max(1, Math.min(region.width, region.height));
        if (!linked && shrink > 1) {
          view = { ...view, position: view.position.map((p, a) => view.target[a]! + (p - view.target[a]!) * shrink) as [number, number, number] };
        }
      }
      current.setView(view);
      if (!comparisonRef.current) rememberSingleView(current.getView());
      fitted.current = true;
      framedFor.current = bounds;
    } else if (fitted.current && framedFor.current && framedFor.current !== bounds) {
      // An outlier excluded or filtered out, or one let back in: the camera
      // follows the box, keeping its angle (asked 2026-10-04).
      if (restoringSingleView.current) restoringSingleView.current = false;
      else current.setView(rescaleView(current.getView(), framedFor.current, bounds));
      if (!comparisonRef.current) rememberSingleView(current.getView());
      framedFor.current = bounds;
    }
    draw();
  }, [dots, displayDots, packet, layout, surfaces, pdf, displayMode, bounds, unit, draw, project.blend.asymmetric]);

  useEffect(() => {
    const current = scene.current;
    if (!current) return;
    if (pdfFocus) {
      if (!pdfViewBeforeFocus.current) pdfViewBeforeFocus.current = current.getView();
      current.focusPdfCell(pdfFocus.windCell, pdfFocus.copy);
    } else if (pdfViewBeforeFocus.current) {
      current.setView(pdfViewBeforeFocus.current);
      pdfViewBeforeFocus.current = null;
    }
    draw();
  }, [pdfFocus, pdf, draw]);

  useEffect(() => {
    scene.current?.setPdfHighlight(pdfHover, pdfFocus);
    draw();
  }, [pdfHover, pdfFocus, pdf, draw]);

  // The grid of copies follows the split's settings and the room there is.
  useEffect(() => {
    const current = scene.current;
    if (!current) return;
    current.setCells(displayCells && displayCells.length === displayDots.points.length / 3 ? { of: displayCells, count: waveCount, region } : null);
    setMarks([]);
    draw();
  }, [cells, displayCells, waveCount, region, dots, displayDots, draw]);

  // The room there is: measured while the split is on, and again whenever
  // the stage is resized or a panel over it opens, closes or changes size.
  useEffect(() => {
    const element = canvas.current;
    if (!splitOn || !element) return;
    const measure = () => {
      const next = freeRegion(element);
      setRegion((now) => (now.x === next.x && now.y === next.y && now.width === next.width && now.height === next.height ? now : next));
    };
    measure();
    const resized = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(measure);
    const changed = typeof MutationObserver === "undefined" ? null : new MutationObserver(measure);
    const view = element.parentElement, workspace = element.closest(".workspace");
    resized?.observe(element);
    for (const selector of [".view3d-toolbar", ".view3d-side", ".wave-display-ranges"]) {
      const panel = view?.querySelector(selector);
      if (panel) resized?.observe(panel);
    }
    // The docked panels mount and unmount beside the stage.
    if (workspace) changed?.observe(workspace, { childList: true });
    if (view) changed?.observe(view, { childList: true });
    window.addEventListener("resize", measure);
    return () => { resized?.disconnect(); changed?.disconnect(); window.removeEventListener("resize", measure); };
  }, [splitOn, waveCount]);

  // Selection is by global index; the scene highlights by drawn index.
  useEffect(() => {
    const current = scene.current;
    if (!current) return;
    if (displayMode === "pdf") { current.setSelection([]); return; }
    const local = new Map<number, number>();
    dots.refs.forEach((g, d) => local.set(g, d));
    current.setSelection(selection.flatMap((g) => (local.has(g) ? [local.get(g)!] : [])));
    draw();
  }, [selection, dots, displayMode, draw]);

  const camera = (preset: CameraPreset) => {
    const current = scene.current;
    if (!current) return;
    clearHover();
    current.setView(presetView(preset, bounds, current.camera.fov, "origin"));
    if (!comparisonRef.current) rememberSingleView(current.getView());
    sync?.publish({ kind: "camera", boat: project.id, view: current.getView(), layout });
    draw();
  };

  const chooseLayout = (next: Layout) => {
    setLayout(next);
    const current = scene.current;
    const nextBounds = sceneBounds(packet, next, waveRanges, toggles);
    if (current) current.setView(presetView("iso", nextBounds, current.camera.fov, "origin"));
    if (current && !comparisonRef.current) {
      rememberSingleView(current.getView());
      singleLayout.current = next;
      savedViewStateRef.current.layout = next;
    }
    framedFor.current = nextBounds;
    if (current) sync?.publish({ kind: "camera", boat: project.id, view: current.getView(), layout: next });
  };

  function chooseTool(next: Tool) {
    setTool(next);
    scene.current?.setRotateEnabled(next === "rotate");
  }

  const chooseDisplay = (next: DisplayMode) => {
    clearHover();
    setPdfFocus(null);
    pdfPointerDown.current = null;
    gesture.current = null;
    drag.current = null;
    setPath(null);
    if (next === "pdf") {
      setTool("rotate");
      scene.current?.setRotateEnabled(true);
    }
    setDisplayMode(next);
  };

  /** Sends a dragged node's speed: one call in flight, the newest value next. */
  const sendDrag = (value: number) => {
    const d = drag.current;
    if (!d) return;
    if (d.busy) { d.pending = value; return; }
    d.busy = true;
    api.editPolar(d.source, { type: "drag", bsp: value }, [d.cell], d.gesture)
      .then(onProject)
      .catch(reportFailure)
      .finally(() => {
        d.busy = false;
        if (d.pending !== null) {
          const next = d.pending;
          d.pending = null;
          if (drag.current === d) sendDrag(next);
          else {
            // The pointer is up: the last value still lands, in the same gesture.
            d.busy = true;
            api.editPolar(d.source, { type: "drag", bsp: next }, [d.cell], d.gesture)
              .then(onProject).catch(reportFailure).finally(() => { d.busy = false; });
          }
        }
      });
  };

  /** Starts dragging the focused source's node under the pointer, if there is one. */
  const startDrag = (x: number, y: number): boolean => {
    const current = scene.current;
    if (!current || focus === null || focused < 0) return false;
    const hit = current.pick(x, y, PICK_RADIUS_PX);
    if (hit < 0) return false;
    const g = dots.refs[hit]!;
    if (g >= packet.nodes.count || packet.nodes.source[g] !== focused) return false;
    const [twa, tws, bsp] = [packet.nodes.points[g * 3]!, packet.nodes.points[g * 3 + 1]!, packet.nodes.points[g * 3 + 2]!];
    const from = current.toScreen(...place(twa, tws, bsp, layout));
    const to = current.toScreen(...place(twa, tws, bsp + 1, layout));
    const axis: [number, number] = from && to ? [to[0] - from[0], to[1] - from[1]] : [0, 0];
    const cell = packet.nodes.cell[g]!;
    dragGestures += 1;
    drag.current = {
      start: bsp, axis, x, y, cell: { twa_index: cell & 0xffff, tws_index: cell >>> 16 },
      gesture: `drag-${dragGestures}`, busy: false, pending: null, source: focus,
    };
    select([g]);
    return true;
  };

  const toGlobal = (locals: ArrayLike<number>) => Array.from(locals, (d) => dots.refs[d]!);

  const onPointerDown = (event: ReactPointerEvent<HTMLCanvasElement>) => {
    clearHover();
    if (displayMode === "pdf") {
      const rect = event.currentTarget.getBoundingClientRect();
      pdfPointerDown.current = event.button === 0
        ? { x: event.clientX - rect.left, y: event.clientY - rect.top }
        : null;
      return;
    }
    if (event.button !== 0) return;
    const rect = event.currentTarget.getBoundingClientRect();
    const x = event.clientX - rect.left, y = event.clientY - rect.top;
    if (tool === "drag") {
      if (startDrag(x, y)) event.currentTarget.setPointerCapture?.(event.pointerId);
      return;
    }
    gesture.current = { x, y, path: [x, y] };
    if (tool !== "rotate") event.currentTarget.setPointerCapture?.(event.pointerId);
  };
  const onPointerMove = (event: ReactPointerEvent<HTMLCanvasElement>) => {
    if (displayMode === "pdf") {
      if (!event.buttons) {
        const rect = event.currentTarget.getBoundingClientRect();
        hoverPoint.current = { x: event.clientX - rect.left, y: event.clientY - rect.top, width: rect.width, height: rect.height };
        if (hoverFrame.current === null) hoverFrame.current = setTimeout(() => {
          hoverFrame.current = null;
          const at = hoverPoint.current;
          if (!at) return;
          const hit = scene.current?.pickPdfCell(at.x, at.y) ?? null;
          setPdfHover(hit ? { ...hit, x: Math.max(8, Math.min(at.x + 12, at.width - 280)),
            y: Math.max(8, Math.min(at.y + 12, at.height - 140)) } : null);
        }, 16);
      }
      return;
    }
    if (!gesture.current && !drag.current && event.buttons === 0) {
      const rect = event.currentTarget.getBoundingClientRect();
      const x = event.clientX - rect.left, y = event.clientY - rect.top;
      hoverPoint.current = { x, y, width: rect.width, height: rect.height };
      // Throttle hit-testing without relying on animation frames, which WebKit
      // suspends in an obscured desktop window. Read the latest pointer position.
      if (hoverFrame.current === null) hoverFrame.current = setTimeout(() => {
        hoverFrame.current = null;
        const at = hoverPoint.current;
        if (!at) return;
        const hit = scene.current?.pick(at.x, at.y, PICK_RADIUS_PX) ?? -1;
        const tipX = Math.max(8, Math.min(at.x + 12, at.width - 280)), tipY = Math.max(8, Math.min(at.y + 12, at.height - 260));
        setNoHoverMatch(false);
        setHover(hit < 0 ? null : { index: dots.refs[hit]!, x: tipX, y: tipY });
        // A dot wins; otherwise the blend's surface, if it is under the pointer.
        const surface = hit < 0 ? scene.current?.pickSurface(at.x, at.y) ?? null : null;
        const cell = surface ? hoverBlend(surface, tipX, tipY) : null;
        if (!surface) clearBlendHover();
        // In a split view the same wind is marked in the other copies, with
        // its speed there (spec.md 10.5).
        const current = scene.current;
        const own = cells && hit >= 0 ? cells[hit]! : -1;
        if (cells && current && own >= 0) {
          const twa = dots.points[hit * 3]!, tws = dots.points[hit * 3 + 1]!;
          const found = correspondingInCells(dots.points, cells, waveCount, { twa, tws }, own);
          const next: typeof marks = [];
          found.forEach((dot, k) => {
            if (dot < 0) return;
            const bsp = dots.points[dot * 3 + 2]!;
            const at = current.toScreenIn(k, ...place(dots.points[dot * 3]!, dots.points[dot * 3 + 1]!, bsp, layout));
            if (at) next.push({ cell: k, x: at[0], y: at[1], bsp });
          });
          setMarks(next);
        } else if (!surface) {
          setMarks([]);
        }
        sync?.publish({ kind: "hover", boat: project.id, point: hit < 0 ? null : { twa: dots.points[hit * 3]!, tws: dots.points[hit * 3 + 1]! } });
        // The linked panes show their own blend at the same wind (spec.md 10.1).
        sync?.publish({ kind: "blend", boat: project.id, point: cell });
      }, 16);
      return;
    }
    const d = drag.current;
    if (d) {
      const rect = event.currentTarget.getBoundingClientRect();
      const x = event.clientX - rect.left, y = event.clientY - rect.top;
      const value = draggedSpeed(d.start, d.axis, x - d.x, y - d.y, event.shiftKey);
      setDragValue({ x, y, bsp: value });
      sendDrag(value);
      return;
    }
    const g = gesture.current;
    if (!g || tool === "rotate") return;
    const rect = event.currentTarget.getBoundingClientRect();
    const x = event.clientX - rect.left, y = event.clientY - rect.top;
    if (tool === "lasso") g.path.push(x, y);
    else g.path = [g.x, g.y, x, g.y, x, y, g.x, y];
    setPath([...g.path]);
  };
  const onPointerUp = (event: ReactPointerEvent<HTMLCanvasElement>) => {
    if (displayMode === "pdf") {
      const down = pdfPointerDown.current;
      pdfPointerDown.current = null;
      if (!down || Math.hypot(event.clientX - event.currentTarget.getBoundingClientRect().left - down.x,
        event.clientY - event.currentTarget.getBoundingClientRect().top - down.y) >= CLICK_SLOP_PX) return;
      const rect = event.currentTarget.getBoundingClientRect();
      const hit = scene.current?.pickPdfCell(event.clientX - rect.left, event.clientY - rect.top) ?? null;
      setPdfFocus((current) => hit && (current?.windCell !== hit.windCell || current?.copy !== hit.copy) ? hit : null);
      event.currentTarget.parentElement?.focus({ preventScroll: true });
      return;
    }
    if (drag.current) {
      drag.current = null;
      setDragValue(null);
      return;
    }
    const g = gesture.current;
    gesture.current = null;
    setPath(null);
    const current = scene.current;
    if (!g || !current) return;
    const rect = event.currentTarget.getBoundingClientRect();
    const x = event.clientX - rect.left, y = event.clientY - rect.top;
    const add = event.shiftKey;
    if (Math.hypot(x - g.x, y - g.y) < CLICK_SLOP_PX) {
      const hit = current.pick(x, y, PICK_RADIUS_PX);
      if (hit >= 0) select(combine(selection, [dots.refs[hit]!], add));
      else if (!add) select([]);
      return;
    }
    if (tool === "lasso") select(combine(selection, toGlobal(current.lasso(g.path)), add));
    if (tool === "box") select(combine(selection, toGlobal(current.box(g.x, g.y, x, y)), add));
  };

  const summary = useMemo(() => summarise(packet, acting), [packet, acting]);
  const selectedCells = useMemo(
    () => new Set(editCells(packet, acting, focused).map((c) => cellCode(c.twa_index, c.tws_index))),
    [packet, acting, focused],
  );
  const selectCells = (codes: number[], add: boolean) => {
    const globals = nodesAtCells(packet, focused, new Set(codes));
    select(add ? combine(selection, globals, true) : combine([], globals, false));
  };
  const labelOf = useMemo(() => new Map(project.sources.map((s) => [s.id, s])), [project.sources]);

  const exclude = (excluded: boolean) => {
    if (readOnly) return;
    const targets = exclusionTargets(packet, acting);
    api.setExcluded(targets.nodes, targets.samples, excluded).then(onProject).catch(reportFailure);
  };

  const factor = SPEED_FACTOR[unit];
  const symbol = SPEED_SYMBOL[unit];
  const speed = (knots: number) => `${(knots * factor).toFixed(1)} ${symbol}`;
  // "By source" and "by time of day" have no ramp: the second has its bands.
  const legendValues = modeValues(packet, shownMode);
  const legendRange = legendValues ? range(legendValues) : null;
  const legendValue = (value: number) => shownMode === "time"
    ? `${new Date((packet.timeOrigin + value) * 1000).toISOString().replace("T", " ").slice(0, 19)} UTC`
    : `${value.toFixed(1)}${shownMode === "wavePeriod" ? " s" : shownMode === "waveAngle" || shownMode === "waveWindAngle" ? "°" : ""}`;
  const unavailableModeTip = msg("Colouring by wave height, current or time needs track samples with their wind, which arrives with the environment fetch");
  const noFilteredTip = msg("No sample with wind is filtered out");
  const noExcludedTip = msg("Nothing is excluded");
  const empty = displayMode === "pdf" ? pdf.volumes.length === 0 : packet.nodes.count === 0 && packet.samples.count === 0;

  return (
    <div className={`view3d${compact ? " compact-comparison" : ""}`} tabIndex={-1}
      onKeyDown={(event) => {
        if (event.key === "Escape" && pdfFocus) { event.stopPropagation(); setPdfFocus(null); return; }
        if (event.key === "Escape" && selection.length > 0) { event.stopPropagation(); select([]); }
      }}>
      <canvas ref={canvas} className={`view3d-canvas tool-${tool}${displayMode === "pdf" ? " pdf-interactive" : ""}`}
        onPointerEnter={() => setHint(later(displayMode === "pdf"
          ? msg("Hover a cell to highlight it. Click to expand its PDF and show percentiles; press Escape to return. Drag to turn, right-drag to pan, scroll to zoom.")
          : msg("Drag to turn, right-drag to pan, scroll to zoom. Click a dot to select it; Shift adds.")))}
        onPointerLeave={() => {
          setHint(null); clearHover();
          sync?.publish({ kind: "hover", boat: project.id, point: null });
          sync?.publish({ kind: "blend", boat: project.id, point: null });
        }} onWheel={clearHover}
        onPointerDown={onPointerDown} onPointerMove={onPointerMove} onPointerUp={onPointerUp}
        onPointerCancel={() => { gesture.current = null; drag.current = null; setPath(null); setDragValue(null); }} />
      <div className="view3d-labels" ref={labelsHost} aria-hidden="true" />
      {cells && totals && (
        <div className="wave-split-cells">
          {Array.from({ length: waveCount }, (_, k) => {
            const at = cellRect(k, waveCount, region);
            const angle = degrees(bucketCentre(k, waveCount));
            return (
              <div key={k} className={at.width < SMALL_COPY ? "wave-split-cell small" : "wave-split-cell"} style={{ left: at.x, top: at.y, width: at.width, height: at.height }}>
                <WaveArrow direction={bucketCentre(k, waveCount)} width={360 / waveCount} sense={waveSense} />
                <span className="wave-split-name">{waveSense === "from" ? t("From {angle}°", { angle }) : t("To {angle}°", { angle })}</span>
                <span className="wave-split-total">{t("{count} samples", { count: totals[k]! })}</span>
              </div>
            );
          })}
          {marks.map((mark) => (
            <span key={mark.cell} className="wave-split-mark" data-cell={mark.cell} style={{ left: mark.x, top: mark.y }}>{speed(mark.bsp)}</span>
          ))}
        </div>
      )}
      {hover && <PolarDotTooltip packet={packet} index={hover.index} sources={project.sources}
        units={settings?.units ?? DEFAULT_UNITS} x={hover.x} y={hover.y} />}
      {hoveredPdfStats && pdfHover && <div className="view3d-tooltip" role="tooltip"
        style={{ left: pdfHover.x, top: pdfHover.y }}>
        {t("TWA {twa}°, TWS {tws}", { twa: hoveredPdfStats.twa.toFixed(0), tws: speed(hoveredPdfStats.tws) })}
        <br />{t("{count} samples", { count: hoveredPdfStats.count })}
        {pdfFocus?.windCell === pdfHover.windCell && pdfFocus.copy === pdfHover.copy && <>
          <br />{t("SOG: {speed}", { speed: speed(pdfHover.sog) })}
          <br />{t("Percentile: {percentile}%", { percentile: pdfPercentileAt(pdfHover.sog, hoveredPdfStats).toFixed(1) })}
        </>}
      </div>}
      {!hover && blendHover && <BlendCellTooltip cell={blendHover.cell} sources={project.sources} colour={blendColour}
        unit={unit} smoothing={project.blend.smoothing} x={blendHover.x} y={blendHover.y} />}
      {noHoverMatch && <div className="view3d-tooltip" role="tooltip" style={{ left: 12, top: 58 }}>{t("No point at matching wind conditions")}</div>}
      {path && path.length >= 4 && (
        <svg className="view3d-gesture" aria-hidden="true">
          <polygon points={path.join(" ")} />
        </svg>
      )}
      {dragValue && (
        <div className="view3d-drag-value" style={{ left: dragValue.x + 12, top: dragValue.y - 12 }}>
          {t("BSP {bsp} {unit}", { bsp: (dragValue.bsp * factor).toFixed(2), unit: symbol })}
        </div>
      )}
      {unavailable !== null && <p className="view3d-unavailable muted">{t(unavailable)}</p>}
      {unavailable === null && empty && (
        <p className="view3d-empty muted">{displayMode === "pdf" ? t("No visible points pass the current filters to form a PDF.") : t("The 3D view shows the project's visible polars and samples.")}</p>
      )}

      <div className="view3d-toolbar">
        <select data-feature="view3d:display" aria-label={t("Display")} value={displayMode}
          title={t("Show individual samples and polar nodes, or their filtered probability density functions")}
          onChange={(event) => chooseDisplay(event.target.value as DisplayMode)}>
          <option value="points">{t("Points and polars")}</option>
          <option value="pdf">{t("Probability density (PDF)")}</option>
        </select>
        <select data-feature="view3d:layout" aria-label={t("Layout")} value={layout}
          title={t("Polar tower: angle and radius are TWA and BSP, height is TWS. Cartesian: TWA, TWS and BSP on straight axes.")}
          onChange={(event) => chooseLayout(event.target.value as Layout)}>
          {(Object.keys(LAYOUT_NAMES) as Layout[]).map((id) => <option key={id} value={id}>{t(LAYOUT_NAMES[id])}</option>)}
        </select>
        <span className="view3d-group" role="group" aria-label={t("Camera")}>
          {CAMERAS.map((c) => (
            <button key={c.id} className="small" data-feature={`view3d:camera-${c.id}`} title={t(c.tip)} onClick={() => camera(c.id)}>
              {t(c.label)}
            </button>
          ))}
        </span>
        {displayMode === "points" && <span className="view3d-group" role="group" aria-label={t("Tool")}>
          {TOOLS.filter((tl) => tl.id !== "drag" || focus !== null).map((tl) => (
            <button key={tl.id} className={tool === tl.id ? "small selected" : "small"} aria-pressed={tool === tl.id}
              data-feature={`view3d:tool-${tl.id}`} title={t(tl.tip)} onClick={() => chooseTool(tl.id)}>
              {t(tl.label)}
            </button>
          ))}
        </span>}
        {!comparison && (
          <span className="view3d-group wave-split-controls" role="group" aria-label={t("Split Wave Angle")}>
            <button className={splitOn ? "small selected" : "small"} aria-pressed={splitOn} data-feature="view3d:wave-split"
              title={t("Draw the view once per wave direction, as seen from the boat: each copy holds only the samples whose waves came from, or went to, its direction")}
              onClick={() => { clearHover(); setWaveSplit((on) => !on); }}>
              {t("Split Wave Angle")}
            </button>
            {splitOn && (
              <>
                <input type="range" min={0} max={WAVE_SPLIT_COUNTS.length - 1} step={1} value={waveCountIndex}
                  data-feature="view3d:wave-split-count" aria-label={t("Wave directions")}
                  aria-valuetext={t("{count} directions", { count: waveCount })}
                  title={t("How many wave directions to split into: 4, 8, 16, 18, 24 or 36. One is always centred on the bow, so none begins or ends at 0°")}
                  onChange={(event) => { clearHover(); setWaveCountIndex(Number(event.target.value)); }} />
                <span className="wave-split-count">{t("{count} directions", { count: waveCount })}</span>
                <select data-feature="view3d:wave-split-sense" aria-label={t("Wave direction sense")} value={waveSense}
                  title={t("From: each copy is where the waves come from. To: where they go")}
                  onChange={(event) => { clearHover(); setWaveSense(event.target.value as WaveSense); }}>
                  <option value="from">{t("From")}</option>
                  <option value="to">{t("To")}</option>
                </select>
                {unplaced > 0 && (
                  <span className="wave-split-none muted" title={t("Samples with no wave direction are in no copy")}>
                    {t("{count} without a wave direction", { count: unplaced })}
                  </span>
                )}
              </>
            )}
          </span>
        )}
      </div>

      {focus !== null && (
        <EditPanel project={project} sourceId={focus} unit={unit} selected={selectedCells} hideOthers={hideOthers}
          onHideOthers={setHideOthers} onSelectCells={selectCells} onProject={onProject}
          onDone={() => editSource(null)} />
      )}

      <WaveRangeControls disabled={readOnly} samples={packet.samples} ranges={waveRanges} onChange={(ranges) => { void updateWaveRanges(() => ranges); }}
        units={settings?.units ?? DEFAULT_UNITS} count={sampleDotsBuilt.refs.length} />

      <div className="view3d-side">
        {project.sources.some((source) => source.kind === "track") && <PriorityFilters project={project} units={settings?.units ?? DEFAULT_UNITS} onProject={onProject} />}
        {project.sources.some((source) => source.kind === "track") && (
          <details className="global-filters">
            <summary data-feature="view3d:global-filters">{t("Global point filters")}</summary>
            <div className="point-filter-body">
              <p className="muted">{t("Applied after each track's filters, including in the blend.")}</p>
              <label><input type="checkbox" data-feature="view3d:global-filters-enabled"
                checked={project.blend.global_filters != null}
                onChange={(e) => { api.setGlobalFilters(e.target.checked ? NO_SAMPLE_FILTERS : null).then(onProject).catch(reportFailure); }} />
                {t("Enable global filters")}</label>
              {project.blend.global_filters && <SampleFiltersEditor prefix="global-filters"
                filters={project.blend.global_filters} units={settings?.units ?? DEFAULT_UNITS}
                onChange={(filters) => api.setGlobalFilters(filters).then(onProject)} />}
            </div>
          </details>
        )}
        <fieldset className="view3d-show">
          <legend>{t("Show")}</legend>
          <label title={t("Every track sample as a dot")}>
            <input type="checkbox" data-feature="view3d:show-samples" checked={toggles.samples}
              onChange={(event) => setToggles({ ...toggles, samples: event.target.checked })} />
            {t("Samples")}
          </label>
          <label title={t("The grid points of every visible polar source")}>
            <input type="checkbox" data-feature="view3d:show-nodes" checked={toggles.nodes}
              onChange={(event) => setToggles({ ...toggles, nodes: event.target.checked })} />
            {t("Polar nodes")}
          </label>
          <label title={t("Each visible polar source as a translucent surface")}>
            <input type="checkbox" data-feature="view3d:show-surfaces" checked={toggles.surfaces} disabled={displayMode === "pdf"}
              onChange={(event) => setToggles({ ...toggles, surfaces: event.target.checked })} />
            {t("Surfaces")}
          </label>
          <label title={filteredExist ? t("Samples the filters remove, drawn dimmed") : t(noFilteredTip)}>
            <input type="checkbox" data-feature="view3d:show-filtered" checked={toggles.filtered} disabled={!filteredExist}
              onChange={(event) => setToggles({ ...toggles, filtered: event.target.checked })} />
            {t("Filtered samples")}
          </label>
          <label title={excludedExist ? t("Samples and polar points you excluded, drawn hollow or crossed") : t(noExcludedTip)}>
            <input type="checkbox" data-feature="view3d:show-excluded" checked={toggles.excluded} disabled={!excludedExist && !toggles.excluded}
              onChange={(event) => setToggles({ ...toggles, excluded: event.target.checked })} />
            {t("Excluded points")}
          </label>
          {displayMode === "points" && <label className="view3d-colour">
            {t("Colour")}
            <select data-feature="view3d:colour" value={shownMode}
              title={modes.hs && modes.current && modes.time ? t("What the dots' colour shows") : t(unavailableModeTip)}
              onChange={(event) => setMode(event.target.value as ColourMode)}>
              {(Object.keys(MODE_NAMES) as ColourMode[]).map((id) => (
                <option key={id} value={id} disabled={!modes[id]}>{t(MODE_NAMES[id])}</option>
              ))}
            </select>
          </label>}
          {displayMode === "points" && legendRange && (
            <div className={`view3d-ramp${shownMode === "time" ? " view3d-ramp-time" : ""}`}>
              <span>{legendValue(legendRange[0])}</span><span className="view3d-ramp-bar" /><span>{legendValue(legendRange[1])}</span>
            </div>
          )}
          {displayMode === "points" && shownMode === "timeOfDay" && <DayBandLegend className="view3d-bands" />}
        </fieldset>

        {displayMode === "pdf" && <div className="view3d-pdf-legend" role="status">
          {focusedPdfStats ? <>
            <strong>{t("TWA {twa}°, TWS {tws}", { twa: focusedPdfStats.twa.toFixed(0), tws: speed(focusedPdfStats.tws) })}</strong>
            <span>{t("{count} samples", { count: focusedPdfStats.count })}</span>
            <div className="view3d-pdf-percentiles">
              {[10, 25, 50, 75, 90].map((percentile, i) => (
                <span key={percentile}>{t("P{percentile}", { percentile })}: {speed(focusedPdfStats.percentiles[i]!)}</span>
              ))}
            </div>
            <button className="small" onClick={() => setPdfFocus(null)}>{t("Show all cells")}</button>
          </> : <>
            <span>{t("{count} wind-condition PDFs", { count: pdf.count })}</span>
            <div className="view3d-ramp"><span>0</span><span className="view3d-ramp-bar" aria-hidden="true" />
              <span>{t("{value} per knot", { value: pdf.maxDensity.toFixed(3) })}</span></div>
            <span>{t("Hover to highlight; click a cell to expand it.")}</span>
          </>}
        </div>}

        <div className="view3d-legend" aria-label={t("Axes")}>
          {layout === "tower"
            ? t("Angle: TWA (°) · Radius: BSP ({unit}) · Height: TWS ({unit})", { unit: symbol })
            : t("Across: TWA (°) · Depth: TWS ({unit}) · Height: BSP ({unit})", { unit: symbol })}
        </div>

        {displayMode === "points" && <section className="view3d-selection" aria-label={t("Selection")}>
          <h3>{summary.count === 0 ? t("Nothing selected") : t("{count} selected", { count: summary.count })}</h3>
          {summary.count > 0 && (
            <>
              <div>{t("Mean TWA {twa}°, TWS {tws}, BSP {bsp}", {
                twa: summary.meanTwa.toFixed(0), tws: speed(summary.meanTws), bsp: speed(summary.meanBsp),
              })}</div>
              <ul className="view3d-breakdown">
                {summary.bySource.map((entry) => {
                  const source = labelOf.get(entry.sourceId);
                  return (
                    <li key={entry.sourceId}>
                      <span className="swatch" style={{ background: source?.colour }} />
                      {t("{label}: {count}", { label: source?.label ?? t("Unknown source"), count: entry.count })}
                    </li>
                  );
                })}
              </ul>
            </>
          )}
          <div className="view3d-actions">
            <button className="small" data-feature="view3d:exclude" disabled={readOnly || summary.included === 0}
              title={t("Remove the selected dots from the blend (undoable)")} onClick={() => exclude(true)}>
              {t("Exclude")}
            </button>
            <button className="small" data-feature="view3d:include" disabled={readOnly || summary.excluded === 0}
              title={t("Put the selected dots back into the blend (undoable)")} onClick={() => exclude(false)}>
              {t("Include")}
            </button>
            <button className="small" data-feature="view3d:show-on-map" disabled={summary.samples === 0}
              title={t("Show the selected samples on the map")}
              onClick={() => { selectSamples(sampleIdsOf(packet, acting), "3d"); focusMap({ kind: "selection" }); }}>
              {t("Show on map")}
            </button>
            <button className="small" data-feature="view3d:clear-selection" disabled={summary.count === 0}
              title={t("Select nothing (Escape)")} onClick={() => select([])}>
              {t("Clear")}
            </button>
          </div>
        </section>}
      </div>
    </div>
  );
}
