/**
 * The three.js scene of the 3D polar view (spec.md 10.1, D16): every dot —
 * a track sample or a polar source's grid node — in one draw call, every
 * visible polar source a translucent surface with its grid lines, the blend
 * an opaque one. three.js is bundled by Vite from npm; nothing is fetched
 * (invariant 4).
 *
 * The dots are one `THREE.Points` over a single buffer (200,000 dots are one
 * draw, not 200,000), drawn by a small shader at a fixed pixel size in one of
 * three shapes: a disc, a ring (an excluded sample, drawn hollow) or a cross
 * (an excluded polar node) (spec.md 10.3). Selection rewrites one attribute
 * rather than rebuilding anything.
 *
 * The scene draws what it is given and knows nothing of sources, units or
 * IPC; `view3d.ts` turns the packed scene into its input.
 */
import * as THREE from "three";
import { OrbitControls } from "three/examples/jsm/controls/OrbitControls.js";

import { CARTESIAN_TWA_SCALE, dotPositions, hatchLines, lassoSelect, project, surfaceMesh, type Layout, type PolarGrid } from "./geometry3d";
import { cellAt, cellRect, type Rect } from "./waveSplit";
import { intersectPdfBox, pdfBoxBounds, pdfBoxEdges, pdfProxyGeometry, type PdfBox, type PdfVolumeInput } from "./pdfVolume";
import { PDF_VOLUME_VERTEX, PDF_VOLUME_FRAGMENT, PDF_OUTLINE_VERTEX, PDF_OUTLINE_FRAGMENT } from "./pdfVolumeShader";
export type { PdfVolumeInput } from "./pdfVolume";

/** A disc: a sample or a node in the blend. */
export const SHAPE_DISC = 0;
/** A ring: an excluded sample (hollow). */
export const SHAPE_RING = 1;
/** A cross: an excluded polar node. */
export const SHAPE_CROSS = 2;
/** A square: a polar node whose cell holds an edit (spec.md 10.4). */
export const SHAPE_SQUARE = 3;

/** One surface. */
export interface SurfaceInput {
  grid: PolarGrid;
  /** `#rrggbb`, the source's stored colour. */
  color: string;
  /** The blend is opaque, and so is the source being edited; sources are translucent (spec.md 10.1). */
  opaque?: boolean;
  /** A translucent surface's opacity: 0.18 unless faded behind the source being edited (spec.md 10.4). */
  opacity?: number;
  /** Its grid lines' colour instead of one derived from `color`: the outline of a blend lost on the background. */
  lineColor?: string;
  /**
   * A colour per grid node, (r, g, b) in 0–1, indexed `j * ni + i`: the
   * Compare stage's difference surface (spec.md 11). `color` is then unused
   * for the faces.
   */
  vertexColors?: Float32Array;
  /**
   * Whether `pickSurface` finds this surface under the pointer: the blend
   * and Compare surfaces, whose cells have something to say (spec.md 10.1, 11).
   */
  pickable?: boolean;
  /** Nodes (indexed `j * ni + i`, non-zero marked) whose quads are hatched, in `hatchColor`. */
  hatched?: Uint8Array;
  hatchColor?: string;
  /**
   * The one copy of a split view this surface is drawn in (spec.md 10.5):
   * a copy's own blend. Absent, the surface is in every copy; with no
   * copies (`setCells(null)`) a surface given one is not drawn at all.
   */
  cell?: number;
}

/** What the scene draws. */
export interface SceneInput {
  /** (TWA, TWS, BSP) triples, one per dot. */
  samples: Float32Array;
  /** (r, g, b) in 0–1, one triple per dot: exactly as long as `samples`. */
  colors: Float32Array;
  /** One `SHAPE_*` per dot; all discs when absent. */
  shapes?: Float32Array;
  surfaces: readonly SurfaceInput[];
  /** Continuous probability fields grouped by wind-condition cell. */
  pdfVolumes?: readonly PdfVolumeInput[];
  layout: Layout;
}

/** How long the last `setData` took, milliseconds. */
export interface BuildTimes {
  dots: number;
  surfaces: number;
}

/**
 * The part of `THREE.WebGLRenderer` the scene uses, so a test can hand it a
 * stand-in where there is no WebGL.
 */
export interface RendererLike {
  setPixelRatio(ratio: number): void;
  getPixelRatio(): number;
  setSize(width: number, height: number, updateStyle?: boolean): void;
  render(scene: THREE.Object3D, camera: THREE.Camera): void;
  dispose(): void;
  // Drawing the scene as a grid of copies (`setCells`); CSS pixels, y from the bottom.
  setViewport?(x: number, y: number, width: number, height: number): void;
  setScissor?(x: number, y: number, width: number, height: number): void;
  setScissorTest?(on: boolean): void;
  setClearColor?(color: THREE.ColorRepresentation): void;
  clear?(): void;
}

/**
 * The scene drawn as a grid of copies (Split Wave Angle, spec.md 10.5): one
 * camera, one canvas, each copy drawn into its own part of it. A page may
 * hold only so many WebGL contexts (WebKit: sixteen), so thirty-six copies
 * cannot each have a canvas.
 */
export interface Cells {
  /** The copy of each dot: its index, `CELL_EVERY` (-1) or `CELL_NONE` (-2). */
  of: Int16Array;
  /** How many copies. */
  count: number;
  /** The part of the canvas the grid fills, CSS pixels from its top left. */
  region: Rect;
}

/** A pickable surface under the pointer, and the grid node of it nearest the hit. */
export interface SurfaceHit {
  /** The surface's index in the `SceneInput.surfaces` it was given in. */
  surface: number;
  /** The node's index on the surface's own TWA axis. */
  twaIndex: number;
  /** The node's index on the surface's own TWS axis. */
  twsIndex: number;
}

/** A camera placement. */
export interface View {
  position: readonly [number, number, number];
  target: readonly [number, number, number];
}

const VERTEX = `
attribute vec3 color;
attribute float selected;
attribute float shape;
attribute float cell;
uniform float size;
// The copy being drawn, or -1 when the scene is one view.
uniform float drawCell;
varying vec3 vColor;
varying float vShape;
void main() {
  vColor = mix(color, vec3(1.0), selected * 0.85);
  vShape = shape;
  // A dot of another copy, or of none, is put outside the clip volume: every
  // copy is one draw of all the dots, and the vertex stage keeps its own.
  if (drawCell > -0.5 && (cell < -1.5 || (cell > -0.5 && abs(cell - drawCell) > 0.5))) {
    gl_Position = vec4(2.0, 2.0, 2.0, 1.0);
    gl_PointSize = 0.0;
    return;
  }
  gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0);
  gl_PointSize = size * (1.0 + selected) * (shape > 2.5 ? 1.6 : (shape > 1.5 ? 1.8 : (shape > 0.5 ? 1.4 : 1.0)));
}`;

const FRAGMENT = `
varying vec3 vColor;
varying float vShape;
void main() {
  vec2 d = gl_PointCoord - vec2(0.5);
  float r2 = dot(d, d);
  if (vShape > 2.5) {
    // A square with a light rim: an edited node.
    float edge = max(abs(d.x), abs(d.y));
    gl_FragColor = vec4(edge > 0.36 ? vec3(1.0) : vColor, 1.0);
    return;
  }
  if (r2 > 0.25) discard;
  if (vShape > 1.5) {
    if (min(abs(d.x - d.y), abs(d.x + d.y)) > 0.14) discard;
  } else if (vShape > 0.5) {
    if (r2 < 0.1) discard;
  }
  gl_FragColor = vec4(vColor, 1.0);
}`;

function pdfOutlineMaterial(color: number, opacity: number): THREE.ShaderMaterial {
  return new THREE.ShaderMaterial({
    vertexShader: PDF_OUTLINE_VERTEX, fragmentShader: PDF_OUTLINE_FRAGMENT,
    uniforms: { drawCell: { value: -1 }, color: { value: new THREE.Color(color) }, opacity: { value: opacity } },
    transparent: true, depthTest: true, depthWrite: false,
  });
}

function disposeChildren(group: THREE.Group) {
  for (const child of [...group.children]) {
    group.remove(child);
    const drawn = child as THREE.Mesh | THREE.LineSegments;
    drawn.geometry.dispose();
    (drawn.material as THREE.Material).dispose();
  }
}

export class PolarScene {
  readonly renderer: RendererLike;
  readonly scene = new THREE.Scene();
  readonly camera = new THREE.PerspectiveCamera(40, 1, 0.1, 2000);
  private controls: OrbitControls | null = null;
  private dots: THREE.Points | null = null;
  private positions = new Float32Array(0);
  private selected = new Float32Array(0);
  private readonly surfaces = new THREE.Group();
  private readonly pdfVolumes = new THREE.Group();
  private readonly pdfCellOutlines = new THREE.Group();
  private readonly pdfSelectionOutline = new THREE.Group();
  private readonly pdfMaterials: THREE.ShaderMaterial[] = [];
  private readonly pdfOutlineMaterials: THREE.ShaderMaterial[] = [];
  private readonly pdfBoxEdges = new Map<string, { positions: Float32Array; copy: number }>();
  private pdfBoxes: PdfBox[] = [];
  private readonly guides = new THREE.Group();
  private screen = new Float32Array(0);
  private screenFresh = false;
  private readonly mvp = new THREE.Matrix4();
  private readonly raycaster = new THREE.Raycaster();
  /** The pickable surfaces' face meshes, with what `pickSurface` answers about each. */
  private pickable: { mesh: THREE.Mesh; surface: number; ni: number; cell: number | undefined }[] = [];
  /** What is drawn in one copy only, by copy. */
  private bound: { objects: THREE.Object3D[]; cell: number }[] = [];
  private width = 1;
  private height = 1;
  private layout: Layout = "tower";
  private orbitAngle = Math.PI / 4;
  /** The grid of copies, or null when the scene is one view. */
  private cells: Cells | null = null;
  /** Each dot's copy, as the shader reads it. */
  private cellOf = new Float32Array(0);
  private background: THREE.Color | null = null;

  /** `renderer` is for tests; the app lets the scene make a WebGL one, which throws where WebGL is missing. */
  constructor(canvas: HTMLCanvasElement, renderer?: RendererLike) {
    this.renderer = renderer ?? new THREE.WebGLRenderer({ canvas, antialias: true });
    this.renderer.setPixelRatio(Math.min(globalThis.devicePixelRatio ?? 1, 2));
    this.camera.up.set(0, 0, 1);
    this.scene.add(this.surfaces);
    this.scene.add(this.pdfVolumes);
    this.scene.add(this.pdfCellOutlines);
    this.scene.add(this.pdfSelectionOutline);
    this.scene.add(this.guides);
    this.orbit(this.orbitAngle);
  }

  /** How many dots are drawn. */
  get dotCount(): number {
    return this.positions.length / 3;
  }

  /**
   * Replaces everything drawn. Refuses (with a `RangeError`, drawing
   * nothing new) input whose arrays disagree on the number of dots: a
   * colour array one triple short would otherwise colour every later dot
   * from its neighbour.
   */
  setData(input: SceneInput): BuildTimes {
    if (input.samples.length % 3 !== 0) {
      throw new RangeError(`${input.samples.length} dot coordinates are not whole (TWA, TWS, BSP) triples`);
    }
    const n = input.samples.length / 3;
    if (input.colors.length !== input.samples.length) {
      throw new RangeError(`${input.colors.length / 3} colours for ${n} dots`);
    }
    if (input.shapes && input.shapes.length !== n) {
      throw new RangeError(`${input.shapes.length} shapes for ${n} dots`);
    }
    const t0 = performance.now();
    this.layout = input.layout;
    this.positions = dotPositions(input.samples, input.layout);
    this.screenFresh = false;
    this.selected = new Float32Array(n);
    // New dots are in every copy until the owner says which is whose: the
    // copies of the dots before these mean nothing for them.
    this.cellOf = new Float32Array(n).fill(-1);
    if (this.cells) this.cells = { ...this.cells, of: new Int16Array(n).fill(-1) };
    const geometry = new THREE.BufferGeometry();
    geometry.setAttribute("position", new THREE.BufferAttribute(this.positions, 3));
    geometry.setAttribute("color", new THREE.BufferAttribute(input.colors, 3));
    geometry.setAttribute("selected", new THREE.BufferAttribute(this.selected, 1));
    geometry.setAttribute("shape", new THREE.BufferAttribute(input.shapes ?? new Float32Array(n), 1));
    geometry.setAttribute("cell", new THREE.BufferAttribute(this.cellOf, 1));
    geometry.computeBoundingSphere();
    if (this.dots) {
      this.scene.remove(this.dots);
      this.dots.geometry.dispose();
      (this.dots.material as THREE.Material).dispose();
    }
    this.dots = new THREE.Points(geometry, new THREE.ShaderMaterial({
      vertexShader: VERTEX,
      fragmentShader: FRAGMENT,
      uniforms: { size: { value: 4 * this.renderer.getPixelRatio() }, drawCell: { value: -1 } },
    }));
    this.scene.add(this.dots);
    const t1 = performance.now();

    disposeChildren(this.pdfVolumes);
    disposeChildren(this.pdfCellOutlines);
    disposeChildren(this.pdfSelectionOutline);
    this.pdfMaterials.length = 0;
    this.pdfOutlineMaterials.length = 0;
    this.pdfBoxEdges.clear();
    this.pdfBoxes = [];
    const edgePositions: number[] = [], edgeCopies: number[] = [];
    for (const volume of input.pdfVolumes ?? []) {
      this.pdfBoxes.push(...volume.boxes);
      // Opaque volumes establish depth before the intentionally faded context.
      // A back-face proxy shades each ray once, even with the camera inside it.
      for (const translucent of [false, true]) {
        const boxes = volume.boxes.filter((box) => (box.opacity < 1) === translucent);
        if (boxes.length === 0) continue;
        const data = pdfProxyGeometry(boxes, input.layout);
        const geometry = new THREE.BufferGeometry();
        geometry.setAttribute("position", new THREE.BufferAttribute(data.positions, 3));
        geometry.setAttribute("pdfLower", new THREE.BufferAttribute(data.lower, 3));
        geometry.setAttribute("pdfUpper", new THREE.BufferAttribute(data.upper, 3));
        geometry.setAttribute("pdfParameters", new THREE.BufferAttribute(data.parameters, 3));
        geometry.setAttribute("pdfOpacity", new THREE.BufferAttribute(data.opacity, 1));
        geometry.setAttribute("cell", new THREE.BufferAttribute(data.copies, 1));
        geometry.setAttribute("windCell", new THREE.BufferAttribute(data.windCells, 1));
        geometry.setIndex(new THREE.BufferAttribute(data.indices, 1));
        const material = new THREE.ShaderMaterial({
          vertexShader: PDF_VOLUME_VERTEX, fragmentShader: PDF_VOLUME_FRAGMENT,
          uniforms: {
            drawCell: { value: -1 }, hoverWindCell: { value: -1 }, hoverCopy: { value: -1 },
            maxDensity: { value: volume.maxDensity }, tower: { value: input.layout === "tower" ? 1 : 0 },
            angleScale: { value: 180 / Math.PI / CARTESIAN_TWA_SCALE },
          },
          transparent: translucent, depthWrite: !translucent, depthTest: true, side: THREE.BackSide,
        });
        this.pdfMaterials.push(material);
        this.pdfVolumes.add(new THREE.Mesh(geometry, material));
      }
      for (const box of volume.boxes) {
        const positions = pdfBoxEdges(box, input.layout);
        this.pdfBoxEdges.set(`${Math.max(0, box.copy)}:${box.windCell}`, { positions, copy: box.copy });
        for (const value of positions) edgePositions.push(value);
        for (let i = 0; i < positions.length / 3; i++) edgeCopies.push(box.copy);
      }
    }
    if (edgePositions.length > 0) {
      const geometry = new THREE.BufferGeometry();
      geometry.setAttribute("position", new THREE.BufferAttribute(Float32Array.from(edgePositions), 3));
      geometry.setAttribute("cell", new THREE.BufferAttribute(Float32Array.from(edgeCopies), 1));
      const material = pdfOutlineMaterial(0x91a6c5, 0.2);
      this.pdfOutlineMaterials.push(material);
      const outlines = new THREE.LineSegments(geometry, material);
      outlines.renderOrder = 8;
      this.pdfCellOutlines.add(outlines);
    }
    disposeChildren(this.surfaces);
    this.pickable = [];
    this.bound = [];
    for (const [index, surface] of input.surfaces.entries()) {
      const own: THREE.Object3D[] = [];
      if (surface.cell !== undefined) this.bound.push({ objects: own, cell: surface.cell });
      const mesh = surfaceMesh(surface.grid, input.layout);
      const shared = new THREE.BufferAttribute(mesh.positions, 3);
      const faces = new THREE.BufferGeometry();
      faces.setAttribute("position", shared);
      faces.setIndex(new THREE.BufferAttribute(mesh.triangles, 1));
      const lines = new THREE.BufferGeometry();
      lines.setAttribute("position", shared);
      lines.setIndex(new THREE.BufferAttribute(mesh.lines, 1));
      const color = new THREE.Color(surface.color);
      // Translucent and not writing depth, so surfaces behind show through
      // and the dots inside them stay visible. The blend is opaque.
      const opaque = surface.opaque === true;
      const opacity = surface.opacity ?? 0.18;
      const painted = surface.vertexColors !== undefined && surface.vertexColors.length === mesh.positions.length;
      if (painted) faces.setAttribute("color", new THREE.BufferAttribute(surface.vertexColors!, 3));
      const drawn = new THREE.Mesh(faces, new THREE.MeshBasicMaterial({
        color: painted ? new THREE.Color(0xffffff) : color, vertexColors: painted,
        transparent: !opaque, opacity: opaque ? 1 : opacity, side: THREE.DoubleSide, depthWrite: opaque,
      }));
      this.surfaces.add(drawn);
      own.push(drawn);
      if (surface.pickable) this.pickable.push({ mesh: drawn, surface: index, ni: surface.grid.twa.length, cell: surface.cell });
      if (surface.hatched) {
        const hatch = hatchLines(surface.grid, surface.hatched);
        if (hatch.length > 0) {
          const crossing = new THREE.BufferGeometry();
          crossing.setAttribute("position", shared);
          crossing.setIndex(new THREE.BufferAttribute(hatch, 1));
          const hatching = new THREE.LineSegments(crossing, new THREE.LineBasicMaterial({
            color: new THREE.Color(surface.hatchColor ?? "#888888"), transparent: true, opacity: 0.9, depthWrite: false,
          }));
          this.surfaces.add(hatching);
          own.push(hatching);
        }
      }
      const outline = new THREE.LineSegments(lines, new THREE.LineBasicMaterial({
        // On an opaque surface the grid lines are lightened, or they vanish into it.
        color: surface.lineColor !== undefined ? new THREE.Color(surface.lineColor)
          : opaque ? color.clone().lerp(new THREE.Color(0xffffff), 0.6) : color, transparent: true,
        opacity: opaque ? 0.9 : Math.min(0.55, opacity * 3), depthWrite: false,
      }));
      this.surfaces.add(outline);
      own.push(outline);
    }
    this.showBound(-1);
    const t2 = performance.now();
    return { dots: t1 - t0, surfaces: t2 - t1 };
  }

  /** Axis guides: line segments as (x, y, z) pairs in model space. */
  setGuides(segments: Float32Array, color: string) {
    disposeChildren(this.guides);
    if (segments.length === 0) return;
    const geometry = new THREE.BufferGeometry();
    geometry.setAttribute("position", new THREE.BufferAttribute(segments, 3));
    this.guides.add(new THREE.LineSegments(geometry, new THREE.LineBasicMaterial({
      color: new THREE.Color(color), transparent: true, opacity: 0.45, depthWrite: false,
    })));
  }

  /** The colour behind everything, `#rrggbb` (follows the theme). */
  setBackground(color: string) {
    this.background = new THREE.Color(color);
    this.scene.background = this.background;
    // What lies between and around the copies of a grid is cleared to it.
    this.renderer.setClearColor?.(this.background);
  }

  /**
   * Draws the scene as a grid of copies, or as one view for null. Refuses
   * (with a `RangeError`) copies that do not name every dot: a dot with no
   * copy would be drawn by whichever value lay past the buffer.
   */
  setCells(cells: Cells | null) {
    if (cells && cells.of.length !== this.dotCount) {
      throw new RangeError(`${cells.of.length} copies for ${this.dotCount} dots`);
    }
    this.cells = cells;
    if (cells) this.cellOf.set(cells.of);
    else this.cellOf.fill(-1);
    const attribute = this.dots?.geometry.getAttribute("cell");
    if (attribute) attribute.needsUpdate = true;
    this.shape();
  }

  /** The copy under a canvas point, -1 for none; a scene that is one view is copy 0 everywhere. */
  cellAt(x: number, y: number): number {
    return this.cells ? cellAt(x, y, this.cells.count, this.cells.region) : 0;
  }

  /** Finds the PDF wind cell under the pointer and its split-wave copy. */
  pickPdfCell(x: number, y: number): { windCell: number; copy: number; sog: number } | null {
    const copy = this.cellAt(x, y);
    if (copy < 0 || this.pdfVolumes.children.length === 0) return null;
    const at = this.cellRect(copy);
    this.camera.updateMatrixWorld();
    this.raycaster.setFromCamera(new THREE.Vector2(((x - at.x) / at.width) * 2 - 1, 1 - ((y - at.y) / at.height) * 2), this.camera);
    const origin = this.raycaster.ray.origin.toArray() as [number, number, number];
    const direction = this.raycaster.ray.direction.toArray() as [number, number, number];
    let nearest = Infinity;
    let found: PdfBox | null = null;
    let foundOpaque = false;
    for (const box of this.pdfBoxes) {
      if ((this.cells && box.copy !== copy) || (!this.cells && box.copy >= 0)) continue;
      const intervals = intersectPdfBox(origin, direction, box, this.layout);
      const entry = intervals[0]?.[0];
      const opaque = box.opacity >= 1;
      // Faded context must not intercept the selected cell's SOG tooltip.
      if (entry === undefined || (foundOpaque && !opaque)) continue;
      if ((opaque && !foundOpaque) || entry < nearest) {
        nearest = entry; found = box; foundOpaque = opaque;
      }
    }
    if (!found) return null;
    const point = this.raycaster.ray.at(nearest, new THREE.Vector3());
    const sog = this.layout === "tower" ? Math.hypot(point.x, point.y) : point.z;
    return { windCell: found.windCell, copy, sog };
  }

  /** Highlights a hovered cell and outlines the selected PDF box. */
  setPdfHighlight(
    hovered: { windCell: number; copy: number } | null,
    selected: { windCell: number; copy: number } | null,
  ) {
    const hoveringSelected = selected !== null && hovered?.windCell === selected.windCell && hovered.copy === selected.copy;
    for (const material of this.pdfMaterials) {
      material.uniforms.hoverWindCell!.value = hoveringSelected ? -1 : hovered?.windCell ?? -1;
      material.uniforms.hoverCopy!.value = hovered?.copy ?? -1;
    }
    disposeChildren(this.pdfSelectionOutline);
    if (!selected || hoveringSelected) return;
    const edges = this.pdfBoxEdges.get(`${selected.copy}:${selected.windCell}`);
    if (!edges) return;
    const geometry = new THREE.BufferGeometry();
    geometry.setAttribute("position", new THREE.BufferAttribute(edges.positions, 3));
    geometry.setAttribute("cell", new THREE.BufferAttribute(new Float32Array(edges.positions.length / 3).fill(edges.copy), 1));
    const outline = new THREE.LineSegments(geometry, pdfOutlineMaterial(0xff4fa3, 1));
    outline.renderOrder = 10;
    this.pdfSelectionOutline.add(outline);
  }

  /** Zooms the camera to the currently rendered PDF cell. */
  focusPdfCell(windCell: number, copy: number) {
    const box = new THREE.Box3();
    for (const cell of this.pdfBoxes) {
      if (cell.windCell !== windCell || (this.cells ? cell.copy !== copy : cell.copy >= 0)) continue;
      const bounds = pdfBoxBounds(cell, this.layout);
      box.expandByPoint(new THREE.Vector3(...bounds.min));
      box.expandByPoint(new THREE.Vector3(...bounds.max));
    }
    if (box.isEmpty()) return;
    const target = box.getCenter(new THREE.Vector3());
    const radius = Math.max(box.getBoundingSphere(new THREE.Sphere()).radius, 0.5);
    const currentTarget = this.controls?.target ?? new THREE.Vector3(0, 0, 14);
    const direction = this.camera.position.clone().sub(currentTarget).normalize();
    // Leave room around the expanded cell so the low-alpha neighboring PDFs
    // remain visible as context.
    const distance = Math.max(radius / Math.sin(THREE.MathUtils.degToRad(this.camera.fov / 2)) * 2.2, 2);
    this.setView({
      position: target.clone().addScaledVector(direction, distance).toArray() as [number, number, number],
      target: target.toArray() as [number, number, number],
    });
  }

  /** Where a copy is drawn, CSS pixels from the canvas's top left: the whole canvas when the scene is one view. */
  cellRect(cell: number): Rect {
    return this.cells ? cellRect(cell, this.cells.count, this.cells.region) : { x: 0, y: 0, width: this.width, height: this.height };
  }

  /** The camera's shape is a copy's, and the dots' screen positions are within one. */
  private shape() {
    const size = this.cellRect(0);
    this.camera.aspect = Math.max(1, size.width) / Math.max(1, size.height);
    // The view's centre, where the camera's target is drawn: the canvas's
    // centre unless told otherwise, and always a copy's own in a grid.
    const centre = this.cells ? null : this.centre;
    if (centre) {
      const width = Math.max(1, this.width), height = Math.max(1, this.height);
      this.camera.setViewOffset(width, height, width / 2 - centre[0], height / 2 - centre[1], width, height);
    } else {
      this.camera.clearViewOffset();
    }
    this.camera.updateProjectionMatrix();
    this.screenFresh = false;
  }

  /** Where the camera's target is drawn, CSS pixels from the canvas's top left. */
  private centre: [number, number] | null = null;

  /**
   * Draws the camera's target at (x, y) rather than the canvas's centre,
   * so the polar's middle sits in the middle of what the panels leave in
   * view (asked 2026-10-02); turning still pivots on the target. Kept in
   * pixels from the canvas's top left across resizes; null for the centre.
   */
  setViewCentre(x: number | null, y?: number) {
    this.centre = x === null || y === undefined ? null : [x, y];
    this.shape();
  }

  /** Sizes the drawing buffer to the canvas's CSS size. */
  resize(width: number, height: number) {
    this.width = Math.max(1, width);
    this.height = Math.max(1, height);
    this.renderer.setSize(this.width, this.height, false);
    this.shape();
  }

  /**
   * Orbit, pan and zoom with the pointer (spec.md 10.1): left drag turns,
   * right drag pans, the wheel zooms. `onChange` runs after every move, so
   * the owner renders on demand rather than every frame.
   */
  private applyingView = false;

  enableControls(element: HTMLElement, onChange: () => void, onInteraction?: () => void) {
    this.controls?.dispose();
    this.controls = new OrbitControls(this.camera, element);
    this.controls.enableDamping = false;
    let interacting = false;
    this.controls.addEventListener("start", () => { interacting = true; });
    this.controls.addEventListener("end", () => { interacting = false; });
    this.controls.addEventListener("change", () => {
      this.screenFresh = false;
      onChange();
      if (interacting && !this.applyingView) onInteraction?.();
    });
  }

  /** Whether dragging turns the view; off while a lasso or box is drawn. */
  setRotateEnabled(enabled: boolean) {
    if (this.controls) this.controls.enableRotate = enabled;
  }

  /** Places the camera. */
  getView(): View {
    return { position: this.camera.position.toArray(), target: this.controls?.target.toArray() ?? [0, 0, 14] };
  }

  /** Places the camera. */
  setView(view: View) {
    this.applyingView = true;
    this.camera.position.set(...view.position);
    const target = new THREE.Vector3(...view.target);
    this.controls?.target.copy(target);
    this.camera.lookAt(target);
    this.camera.updateMatrixWorld();
    this.controls?.update();
    this.applyingView = false;
    this.screenFresh = false;
  }

  /** Places the camera on a circle round the TWS axis, looking at mid-height (the benchmark's orbit). */
  orbit(angle: number) {
    this.orbitAngle = angle;
    const r = 45;
    this.setView({ position: [r * Math.cos(angle), r * Math.sin(angle), 32], target: [0, 0, 14] });
  }

  /** Shows what belongs to copy `cell` and hides the other copies' own; -1 hides them all. */
  private showBound(cell: number) {
    for (const entry of this.bound) {
      const shown = entry.cell === cell;
      for (const object of entry.objects) object.visible = shown;
    }
  }

  private setPdfDrawCell(copy: number) {
    for (const material of [...this.pdfMaterials, ...this.pdfOutlineMaterials]) material.uniforms.drawCell!.value = copy;
    for (const child of this.pdfSelectionOutline.children) {
      ((child as THREE.LineSegments).material as THREE.ShaderMaterial).uniforms.drawCell!.value = copy;
    }
  }

  render() {
    const cells = this.cells;
    const drawCell = (this.dots?.material as THREE.ShaderMaterial | undefined)?.uniforms.drawCell;
    if (!cells) {
      if (drawCell) drawCell.value = -1;
      this.setPdfDrawCell(-1);
      this.showBound(-1);
      this.renderer.render(this.scene, this.camera);
      return;
    }
    const renderer = this.renderer;
    // The canvas behind and between the copies, then each copy in its part
    // of it. WebGL counts y from the bottom.
    renderer.setScissorTest?.(false);
    renderer.setViewport?.(0, 0, this.width, this.height);
    renderer.clear?.();
    renderer.setScissorTest?.(true);
    for (let k = 0; k < cells.count; k++) {
      const at = cellRect(k, cells.count, cells.region);
      const bottom = this.height - at.y - at.height;
      renderer.setViewport?.(at.x, bottom, at.width, at.height);
      renderer.setScissor?.(at.x, bottom, at.width, at.height);
      if (drawCell) drawCell.value = k;
      this.setPdfDrawCell(k);
      this.showBound(k);
      renderer.render(this.scene, this.camera);
    }
    renderer.setScissorTest?.(false);
    renderer.setViewport?.(0, 0, this.width, this.height);
  }

  /**
   * Highlights exactly `indices`; everything else is unselected. An index
   * that is not a dot — past the end, negative, fractional — is ignored
   * rather than written past the buffer. Returns how many were ignored.
   */
  setSelection(indices: ArrayLike<number>): number {
    const n = this.selected.length;
    this.selected.fill(0);
    let ignored = 0;
    for (let k = 0; k < indices.length; k++) {
      const i = indices[k]!;
      if (Number.isInteger(i) && i >= 0 && i < n) this.selected[i] = 1;
      else ignored++;
    }
    const attribute = this.dots?.geometry.getAttribute("selected");
    if (attribute) attribute.needsUpdate = true;
    return ignored;
  }

  /** Every dot's screen position for the current camera, cached until it moves. */
  projected(): Float32Array {
    if (!this.screenFresh) {
      this.camera.updateMatrixWorld();
      this.mvp.multiplyMatrices(this.camera.projectionMatrix, this.camera.matrixWorldInverse);
      // Within a copy: every copy is the same picture of its own dots.
      const size = this.cellRect(0);
      this.screen = project(this.positions, this.mvp.elements, size.width, size.height, this.screen);
      this.screenFresh = true;
    }
    return this.screen.subarray(0, this.dotCount * 2);
  }

  /** Whether dot `i` is drawn in copy `cell`. */
  private inCell(i: number, cell: number): boolean {
    const of = this.cellOf[i]!;
    return of === -1 || of === cell;
  }

  /**
   * The dots inside a screen-space lasso (flat x, y pairs in CSS pixels
   * from the canvas's top left), for the current camera.
   */
  lasso(polygon: ArrayLike<number>): Uint32Array {
    if (!this.cells) return lassoSelect(this.projected(), polygon);
    // In a grid the gesture belongs to the copy it began in, and selects
    // that copy's dots.
    const cell = polygon.length >= 2 ? this.cellAt(polygon[0]!, polygon[1]!) : -1;
    if (cell < 0) return new Uint32Array(0);
    const at = this.cellRect(cell);
    const local = Array.from(polygon, (value, k) => value - (k % 2 === 0 ? at.x : at.y));
    return lassoSelect(this.projected(), local).filter((dot) => this.inCell(dot, cell));
  }

  /** The dots inside a screen-space box between two corners. */
  box(x0: number, y0: number, x1: number, y1: number): Uint32Array {
    return this.lasso([x0, y0, x1, y0, x1, y1, x0, y1]);
  }

  /** The dot nearest a screen point within `radius` pixels, or -1. */
  pick(x: number, y: number, radius: number): number {
    const cell = this.cellAt(x, y);
    if (cell < 0) return -1;
    const split = this.cells !== null;
    const at = this.cellRect(cell);
    x -= at.x;
    y -= at.y;
    const screen = this.projected();
    let best = -1;
    let bestDistance = radius * radius;
    for (let i = 0; i < screen.length / 2; i++) {
      if (split && !this.inCell(i, cell)) continue;
      const dx = screen[i * 2]! - x, dy = screen[i * 2 + 1]! - y;
      const d = dx * dx + dy * dy;
      // NaN (behind the camera) fails the comparison.
      if (d <= bestDistance) { best = i; bestDistance = d; }
    }
    return best;
  }

  /**
   * The pickable surface under a screen point (CSS pixels from the canvas's
   * top left) and the grid node of it nearest the hit, or null. Only the
   * surfaces marked `pickable` are tested: the translucent sources in front
   * of the blend are seen through, so they are picked through as well. The
   * nearest of them to the camera wins.
   */
  pickSurface(x: number, y: number): SurfaceHit | null {
    if (this.pickable.length === 0) return null;
    const cell = this.cellAt(x, y);
    if (cell < 0) return null;
    const at = this.cellRect(cell);
    this.camera.updateMatrixWorld();
    this.raycaster.setFromCamera(new THREE.Vector2(((x - at.x) / at.width) * 2 - 1, 1 - ((y - at.y) / at.height) * 2), this.camera);
    // A copy's own surface is found in that copy alone.
    const here = this.pickable.filter((entry) => entry.cell === undefined || (this.cells !== null && entry.cell === cell));
    const hit = this.raycaster.intersectObjects(here.map((entry) => entry.mesh), false)[0];
    const entry = hit && here.find((candidate) => candidate.mesh === hit.object);
    if (!hit || !entry || !hit.face) return null;
    // The hit triangle's corner nearest the hit: a vertex is a grid node,
    // `j * ni + i` (`surfaceMesh`).
    const position = entry.mesh.geometry.getAttribute("position");
    const corner = new THREE.Vector3();
    let vertex = hit.face.a;
    let nearest = Infinity;
    for (const candidate of [hit.face.a, hit.face.b, hit.face.c]) {
      const distance = corner.fromBufferAttribute(position, candidate).distanceToSquared(hit.point);
      if (distance < nearest) { nearest = distance; vertex = candidate; }
    }
    return { surface: entry.surface, twaIndex: vertex % entry.ni, twsIndex: Math.floor(vertex / entry.ni) };
  }

  /** A model-space point's screen position (in the first copy of a grid), or null when it is behind the camera. */
  toScreen(x: number, y: number, z: number): [number, number] | null {
    return this.toScreenIn(0, x, y, z);
  }

  /** A model-space point's position on the canvas as copy `cell` draws it, or null when it is behind the camera. */
  toScreenIn(cell: number, x: number, y: number, z: number): [number, number] | null {
    const at = this.cellRect(cell);
    this.camera.updateMatrixWorld();
    this.mvp.multiplyMatrices(this.camera.projectionMatrix, this.camera.matrixWorldInverse);
    const out = project(new Float32Array([x, y, z]), this.mvp.elements, at.width, at.height);
    return Number.isNaN(out[0]) ? null : [at.x + out[0]!, at.y + out[1]!];
  }

  /** Frees every GPU resource: geometries, materials, controls and the renderer. */
  dispose() {
    this.controls?.dispose();
    this.controls = null;
    if (this.dots) {
      this.scene.remove(this.dots);
      this.dots.geometry.dispose();
      (this.dots.material as THREE.Material).dispose();
      this.dots = null;
    }
    disposeChildren(this.surfaces);
    disposeChildren(this.pdfVolumes);
    disposeChildren(this.pdfCellOutlines);
    disposeChildren(this.pdfSelectionOutline);
    this.pdfMaterials.length = 0;
    this.pdfOutlineMaterials.length = 0;
    this.pdfBoxes = [];
    this.pdfBoxEdges.clear();
    this.pickable = [];
    disposeChildren(this.guides);
    this.positions = new Float32Array(0);
    this.selected = new Float32Array(0);
    this.cellOf = new Float32Array(0);
    this.cells = null;
    this.renderer.dispose();
  }
}
