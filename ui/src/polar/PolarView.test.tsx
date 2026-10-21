// @vitest-environment happy-dom
/**
 * The 3D stage against a stand-in scene (happy-dom has no WebGL) and a
 * mocked backend: it fetches the packed scene on every revision, selects by
 * click, excludes through Rust, disposes the scene on unmount, and still
 * shows its controls where WebGL is missing.
 */
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import type { ProjectSummary } from "../generated/ProjectSummary";
import { BLEND_SOURCE, FLAG_EXCLUDED, FLAG_FILTERED, type ScenePacket } from "./scenePacket";
import { TEST_BLEND } from "../testBlend";

const scenes = vi.hoisted(() => ({
  made: [] as FakeScene[], fail: false, pick: -1,
  /** The copy the pointer is over, in a split view. */
  cell: 0,
  surface: null as { surface: number; twaIndex: number; twsIndex: number } | null,
  /** What the fake scene's `projected()` answers: (x, y) per drawn dot. */
  projected: [] as number[],
}));

interface FakeScene {
  setView: ReturnType<typeof vi.fn>;
  setData: ReturnType<typeof vi.fn>;
  setSelection: ReturnType<typeof vi.fn>;
  setCells: ReturnType<typeof vi.fn>;
  dispose: ReturnType<typeof vi.fn>;
}

vi.mock("./scene3d", async (original) => {
  const actual = await original<typeof import("./scene3d")>();
  class PolarScene {
    camera = { fov: 40 };
    setData = vi.fn();
    setSelection = vi.fn(() => 0);
    dispose = vi.fn();
    constructor() {
      if (scenes.fail) throw new Error("no WebGL");
      scenes.made.push(this);
    }
    setBackground() {}
    enableControls() {}
    setRotateEnabled() {}
    resize() {}
    setGuides() {}
    setPdfHighlight = vi.fn();
    pickPdfCell() { return null; }
    setView = vi.fn();
    getView() { return { position: [0, 0, 40] as [number, number, number], target: [0, 0, 0] as [number, number, number] }; }
    setCells = vi.fn();
    setViewCentre() {}
    render() {}
    toScreen() { return [0, 0]; }
    // A row of copies, each 100 × 100.
    cellAt() { return scenes.cell; }
    cellRect(cell: number) { return { x: cell * 100, y: 0, width: 100, height: 100 }; }
    toScreenIn(cell: number) { return [cell * 100 + 50, 50]; }
    /** Every dot at (30 + 20 i, 40 + 10 i). */
    projected() { return Float32Array.from(scenes.projected); }
    pick() { return scenes.pick; }
    pickSurface() { return scenes.surface; }
    lasso() { return new Uint32Array(0); }
    box() { return new Uint32Array(0); }
  }
  return { ...actual, PolarScene };
});

const api = vi.hoisted(() => ({
  setWaveRanges: vi.fn(), blendCell: vi.fn(), blendCellSplit: vi.fn(), polarScene: vi.fn(), polarSceneSplit: vi.fn(), setExcluded: vi.fn(), editPolar: vi.fn(), polarEditSurface: vi.fn(), setSegmentStatistic: vi.fn(),
}));
vi.mock("../ipc", () => ({ api }));

const { default: PolarView, draggedSpeed } = await import("./PolarView");
const { createFleetSync, FleetSyncProvider } = await import("../boats/synchronization");
const { editSource } = await import("./editFocus");
const selection = await import("../selection");

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
let host: HTMLDivElement;
let root: Root;

function packet(excluded = false): ScenePacket {
  return {
    timeOrigin: 0,
    samplesKey: 0,
    sources: [{ id: 10, colour: "#ff0000", kind: "orc" }],
    nodes: {
      count: 2, points: Float32Array.from([52, 6, 6, 90, 12, 8]), source: Uint32Array.from([0, 0]),
      cell: Uint32Array.from([0, 1 | (1 << 16)]), flags: Uint32Array.from([0, excluded ? FLAG_EXCLUDED : 0]),
    },
    samples: {
      wavePeriod: new Float32Array(0), waveAngle: new Float32Array(0), waveWindAngle: new Float32Array(0), waveBearing: new Float32Array(0),
      count: 0, points: new Float32Array(0), source: new Uint32Array(0), ids: new Uint32Array(0),
      hs: new Float32Array(0), current: new Float32Array(0), time: new Float32Array(0), flags: new Uint32Array(0),
    },
    surfaces: [{ source: 0, twa: Float32Array.from([52, 90]), tws: Float32Array.from([6, 12]), bsp: Float32Array.from([6, 7, 7, 8]) }],
  };
}

const project = (revision: number): ProjectSummary => ({
  id: 1, name: "P", path: null, dirty: false, revision, boat_name: "", boat_notes: "",
  sources: [{ id: 10, kind: "orc", label: "Farr 40", colour: "#ff0000", visible: true, weight: 1, count: 2, used: null, polar_file: null, orr: null, orc: null, track: null, edits: 0 }],
  can_undo: false, can_redo: false, undo_label: null, redo_label: null, use_corrected: true, stokes_drift: false, blend: TEST_BLEND,
});

const onProject = vi.fn();

it("shows details for the picked visible dot and clears hover when dragging or leaving", async () => {
  api.polarScene.mockResolvedValue(withSamples());
  await render();
  const canvas = q("canvas")!;
  const hover = async (index: number) => {
    scenes.pick = index;
    await act(async () => {
      canvas.dispatchEvent(new PointerEvent("pointermove", { bubbles: true, clientX: 60, clientY: 70, buttons: 0 }));
      await new Promise((r) => setTimeout(r, 30));
    });
  };
  await hover(0);
  expect(q('[role="tooltip"]')?.textContent).toContain("Farr 40");
  expect(q('[role="tooltip"]')?.textContent).toContain("52 °");
  await hover(2); // visible local #2 maps to the first track sample
  expect(q('[role="tooltip"]')?.textContent).toContain("14.0 kn");
  expect(q('[role="tooltip"]')?.textContent).toContain("Wave period");
  expect(q('[role="tooltip"]')?.textContent).toContain("1970-01-01");
  await act(async () => { canvas.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, button: 0 })); });
  expect(q('[role="tooltip"]')).toBeNull();
  await act(async () => { canvas.dispatchEvent(new PointerEvent("pointerup", { bubbles: true, button: 0 })); });
  await hover(0);
  expect(q('[role="tooltip"]')).not.toBeNull();
  await act(async () => { canvas.dispatchEvent(new PointerEvent("pointerout", { bubbles: true })); });
  expect(q('[role="tooltip"]')).toBeNull();
});

it("names the sources behind the blend cell under the pointer, and prefers a dot to the surface (spec.md 10.1)", async () => {
  // The blend is the scene's second surface, drawn (spline mode) on finer
  // axes than the output grid: its node at 89° in 12 kn belongs to the
  // output cell at 90° (index 10) and 12 kn (index 4).
  const base = packet();
  api.polarScene.mockResolvedValue({
    ...base,
    surfaces: [...base.surfaces,
      { source: BLEND_SOURCE, twa: Float32Array.from([88, 89]), tws: Float32Array.from([11, 12]), bsp: Float32Array.from([7, 7.4, 7.2, 7.5]) }],
  });
  api.blendCell.mockResolvedValue({
    twa: 90, tws: 12, bsp: 7.524, origin: "direct", corrected: false,
    contributors: [{ source_id: 10, bsp: 8, weight: 0.5, share: 1 }],
  });
  await render();
  const canvas = q("canvas")!;
  const move = async () => {
    await act(async () => {
      canvas.dispatchEvent(new PointerEvent("pointermove", { bubbles: true, clientX: 60, clientY: 70, buttons: 0 }));
      await new Promise((r) => setTimeout(r, 30));
    });
  };
  scenes.surface = { surface: 1, twaIndex: 1, twsIndex: 1 };
  await move();
  expect(api.blendCell).toHaveBeenCalledWith(10, 4);
  const tip = q(".blend-cell-tooltip")!;
  expect(tip.textContent).toContain("Blend");
  expect(tip.textContent).toContain("7.52 kn");
  expect(tip.textContent).toContain("Farr 40");
  expect(tip.textContent).toContain("100%");

  // Staying on the same cell asks Rust nothing more.
  await move();
  expect(api.blendCell).toHaveBeenCalledTimes(1);

  // A dot under the pointer wins over the surface behind it.
  scenes.pick = 0;
  await move();
  expect(q(".blend-cell-tooltip")).toBeNull();
  expect(q('[role="tooltip"]')?.textContent).toContain("Farr 40");

  // Off both, nothing is shown.
  scenes.pick = -1;
  scenes.surface = null;
  await move();
  expect(q('[role="tooltip"]')).toBeNull();
});

async function render(revision = 1, id = 1) {
  await act(async () => root.render(<PolarView project={{ ...project(revision), id }} settings={null} onProject={onProject} />));
  await act(async () => { await Promise.resolve(); });
}

const q = (selector: string) => host.querySelector<HTMLElement>(selector);
const feature = (id: string) => q(`[data-feature="${id}"]`) as HTMLButtonElement | HTMLSelectElement | null;

async function clickCanvas(shift = false) {
  const canvas = q("canvas")!;
  await act(async () => {
    canvas.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, button: 0, clientX: 50, clientY: 50, shiftKey: shift }));
    canvas.dispatchEvent(new PointerEvent("pointerup", { bubbles: true, button: 0, clientX: 50, clientY: 50, shiftKey: shift }));
  });
}

/** The same scene with two track samples (ids 5 and 2^32 + 6), the second filtered out. */
function withSamples(): ScenePacket {
  const base = packet();
  return {
    ...base,
    sources: [...base.sources, { id: 30, colour: "#0000ff", kind: "track" }],
    samples: {
      wavePeriod: new Float32Array(0), waveAngle: new Float32Array(0), waveWindAngle: new Float32Array(0), waveBearing: new Float32Array(0),
      count: 2, points: Float32Array.from([120, 14, 9, 150, 16, 10]), source: Uint32Array.from([1, 1]),
      ids: Uint32Array.from([5, 0, 6, 1]), hs: Float32Array.from([Number.NaN, Number.NaN]),
      current: Float32Array.from([Number.NaN, Number.NaN]), time: Float32Array.from([0, 600]),
      flags: Uint32Array.from([0, FLAG_FILTERED]),
    },
  };
}

beforeEach(() => {
  selection.resetSelection();
  editSource(null);
  api.setWaveRanges.mockReset().mockResolvedValue(project(3));
  // No copy has a blend of its own unless a test gives it one.
  api.polarSceneSplit.mockReset().mockResolvedValue({ count: 4, surfaces: [] });
  api.blendCellSplit.mockReset();
  api.editPolar.mockReset().mockResolvedValue(project(3));
  api.polarEditSurface.mockReset().mockResolvedValue({
    source_id: 10, kind: "orc", twa: [52, 90], tws: [6, 12], source: [[6, 7], [7, 8]], bsp: [[6, 7], [7, 8]],
    edited: [[false, false], [false, false]], excluded: [[false, false], [false, false]], count: null, spread: null,
    statistic: null, min_samples: 5, edit_count: 0,
  });
  scenes.made = [];
  scenes.fail = false;
  scenes.pick = -1;
  scenes.cell = 0;
  scenes.surface = null;
  api.blendCell.mockReset();
  api.polarScene.mockReset().mockResolvedValue(packet());
  api.setExcluded.mockReset().mockResolvedValue(project(2));
  onProject.mockReset();
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); });

it("fetches the scene on mount and again on every revision, and draws its dots", async () => {
  await render(1);
  expect(api.polarScene).toHaveBeenCalledTimes(1);
  const scene = scenes.made[0]!;
  const last = scene.setData.mock.calls.at(-1)![0];
  expect(last.samples.length).toBe(6);
  expect(last.surfaces).toHaveLength(1);
  await render(2);
  expect(api.polarScene).toHaveBeenCalledTimes(2);
});

it("starts empty and populated boats with the zero-degree axis at the top", async () => {
  await render();
  const calls = scenes.made[0]!.setView.mock.calls;
  expect(calls.length).toBeGreaterThanOrEqual(2);
  for (const [view] of calls) {
    // The polar's 0° ray is +y. Looking from +z with a slight -y offset
    // places +y straight up and +x (90°) to the right, without camera roll.
    expect(view.position[0]).toBeCloseTo(view.target[0], 8);
    expect(view.position[1]).toBeLessThan(view.target[1]);
    expect(view.position[2]).toBeGreaterThan(view.target[2]);
    expect(Math.abs(view.position[1]-view.target[1]) / (view.position[2]-view.target[2])).toBeCloseTo(0.001, 6);
  }
});

it("selects the clicked dot, summarises it and excludes it through Rust", async () => {
  await render();
  expect(q(".view3d-selection h3")!.textContent).toBe("Nothing selected");
  expect((feature("view3d:exclude") as HTMLButtonElement).disabled).toBe(true);
  scenes.pick = 1;
  await clickCanvas();
  expect(q(".view3d-selection h3")!.textContent).toBe("1 selected");
  expect(q(".view3d-selection")!.textContent).toContain("Mean TWA 90°, TWS 12.0 kn, BSP 8.0 kn");
  expect(q(".view3d-breakdown")!.textContent).toBe("Farr 40: 1");
  expect(scenes.made[0]!.setSelection.mock.calls.at(-1)![0]).toEqual([1]);
  expect((feature("view3d:include") as HTMLButtonElement).disabled).toBe(true);
  expect((feature("view3d:show-on-map") as HTMLButtonElement).disabled).toBe(true);

  await act(async () => (feature("view3d:exclude") as HTMLButtonElement).click());
  expect(api.setExcluded).toHaveBeenCalledWith([{ source_id: 10, twa_index: 1, tws_index: 1 }], [], true);
  expect(onProject).toHaveBeenCalledWith(project(2));

  // The refetched scene has the node excluded, and an excluded point is
  // hidden (asked 2026-10-04): it leaves the selection with the view.
  api.polarScene.mockResolvedValue(packet(true));
  await render(2);
  expect(q(".view3d-selection h3")!.textContent).toBe("Nothing selected");
  const showExcluded = feature("view3d:show-excluded") as unknown as HTMLInputElement;
  expect(showExcluded.checked).toBe(false);
  expect(showExcluded.disabled).toBe(false);
  // Shown again, it can be picked and included.
  await act(async () => showExcluded.click());
  expect(scenes.made[0]!.setData.mock.calls.at(-1)![0].samples.length / 3).toBe(packet(true).nodes.count + packet(true).samples.count);
  await clickCanvas();
  expect(q(".view3d-selection h3")!.textContent).toBe("1 selected");
  expect((feature("view3d:include") as HTMLButtonElement).disabled).toBe(false);
  expect((feature("view3d:exclude") as HTMLButtonElement).disabled).toBe(true);
});

it("adds with Shift, clears on an empty click and on Escape", async () => {
  await render();
  scenes.pick = 0;
  await clickCanvas();
  scenes.pick = 1;
  await clickCanvas(true);
  expect(q(".view3d-selection h3")!.textContent).toBe("2 selected");
  await act(async () => q(".view3d")!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })));
  expect(q(".view3d-selection h3")!.textContent).toBe("Nothing selected");
  await clickCanvas();
  scenes.pick = -1;
  await clickCanvas();
  expect(q(".view3d-selection h3")!.textContent).toBe("Nothing selected");
});

it("offers colour modes only when samples have the data", async () => {
  await render();
  const options = [...(feature("view3d:colour") as HTMLSelectElement).options];
  expect(options.map((o) => [o.value, o.disabled])).toEqual([["source", false], ["hs", true], ["wavePeriod", true], ["waveAngle", true], ["waveWindAngle", true], ["current", true], ["time", true], ["timeOfDay", true]]);
  expect((feature("view3d:show-filtered") as unknown as HTMLInputElement).disabled).toBe(true);
});

it("disposes the scene on unmount", async () => {
  await render();
  const scene = scenes.made[0]!;
  await act(async () => root.unmount());
  expect(scene.dispose).toHaveBeenCalledTimes(1);
  root = createRoot(host);
});

it("says so where WebGL is missing, and keeps its controls", async () => {
  scenes.fail = true;
  await render();
  expect(q(".view3d-unavailable")!.textContent).toBe("The 3D view needs WebGL, which this system does not offer.");
  expect(feature("view3d:layout")).not.toBeNull();
  expect(feature("view3d:exclude")).not.toBeNull();
});

it("shares its selected samples with the map, and shows them there", async () => {
  api.polarScene.mockResolvedValue(withSamples());
  const focused = vi.fn();
  const off = selection.onFocusMap(focused);
  await render();
  // Dot 2 is the first sample (the two nodes come first).
  scenes.pick = 2;
  await clickCanvas();
  expect([...selection.getSampleSelection().ids]).toEqual([5]);
  const show = feature("view3d:show-on-map") as HTMLButtonElement;
  expect(show.disabled).toBe(false);
  await act(async () => show.click());
  expect(focused).toHaveBeenCalledWith({ kind: "selection" });
  off();
});

it("selects what a box on the map selected, but counts and excludes only drawn dots", async () => {
  api.polarScene.mockResolvedValue(withSamples());
  await render();
  await act(async () => selection.selectSamples([5, 2 ** 32 + 6], "map"));
  // The second sample is filtered out and hidden: it is not counted.
  expect(q(".view3d-selection h3")!.textContent).toBe("1 selected");
  expect(q(".view3d-breakdown")!.textContent).toBe("Unknown source: 1");
  await act(async () => (feature("view3d:exclude") as HTMLButtonElement).click());
  expect(api.setExcluded).toHaveBeenCalledWith([], [5], true);
});

it("turns a drag into a boat speed along the BSP axis on screen, snapped with Shift", () => {
  // One knot is 30 px to the right: 45 px right is 1.5 kn more.
  expect(draggedSpeed(6, [30, 0], 45, 10, false)).toBe(7.5);
  // Seen end-on, the axis falls back to 20 px per knot upward.
  expect(draggedSpeed(6, [0, 1], 0, -21, false)).toBe(7.05);
  expect(draggedSpeed(6, [0, 1], 0, -21.5, true)).toBe(7.1);
  expect(draggedSpeed(6, [0, -20], 0, 400, false)).toBe(0);
  expect(draggedSpeed(6, [0, -20], 0, -4000, false)).toBe(60);
});

it("edits the focused source: opaque, with its table, and a drag is one gesture sent to Rust", async () => {
  editSource(10);
  await render();
  expect(api.polarScene).toHaveBeenLastCalledWith(10, null);
  const scene = scenes.made[0]!;
  expect(scene.setData.mock.calls.at(-1)![0].surfaces[0].opaque).toBe(true);
  expect(api.polarEditSurface).toHaveBeenCalledWith(10);
  expect(q(".view3d-edit")!.textContent).toContain("Editing Farr 40");

  await act(async () => (feature("view3d:tool-drag") as HTMLButtonElement).click());
  scenes.pick = 1;
  const canvas = q("canvas")!;
  await act(async () => {
    canvas.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, button: 0, clientX: 50, clientY: 50 }));
    canvas.dispatchEvent(new PointerEvent("pointermove", { bubbles: true, button: 0, clientX: 50, clientY: 10 }));
  });
  await act(async () => { await Promise.resolve(); });
  await act(async () => {
    canvas.dispatchEvent(new PointerEvent("pointermove", { bubbles: true, button: 0, clientX: 50, clientY: 31, shiftKey: true }));
    canvas.dispatchEvent(new PointerEvent("pointerup", { bubbles: true, button: 0, clientX: 50, clientY: 31 }));
  });
  await act(async () => { await Promise.resolve(); });
  const sent = api.editPolar.mock.calls;
  expect(sent.map((call) => call[1])).toEqual([{ type: "drag", bsp: 10 }, { type: "drag", bsp: 8.95 }]);
  expect(sent.every((call) => call[0] === 10 && call[3] === sent[0]![3])).toBe(true);
  expect(sent[0]![2]).toEqual([{ twa_index: 1, tws_index: 1 }]);
  // Selecting the node selected its cell in the table.
  expect(q(".view3d-edit-cell.selected")).not.toBeNull();

  await act(async () => (feature("edit:done") as HTMLButtonElement).click());
  expect(q(".view3d-edit")).toBeNull();
  expect(feature("view3d:tool-drag")).toBeNull();
});

it("takes only the flags when the scene held names its samples", async () => {
  await render(1);
  const first = api.polarScene.mock.results[0]!.value as Promise<ScenePacket>;
  await render(2);
  expect(api.polarScene).toHaveBeenLastCalledWith(null, await first);
});

it("never sends another project's samples key: a new project fetches the whole scene", async () => {
  await render(1, 1);
  await render(2, 1);
  expect(api.polarScene.mock.calls[1]![1]).not.toBeNull();
  await render(1, 2);
  expect(api.polarScene).toHaveBeenLastCalledWith(null, null);
});

it("shows UTC endpoints for time and enables the three wave colour dimensions", async () => {
  const data = withSamples();
  data.timeOrigin = Date.UTC(2026, 8, 30, 12, 0, 0) / 1000;
  data.samples.wavePeriod = Float32Array.from([6, 9]);
  data.samples.waveAngle = Float32Array.from([20, 150]);
  data.samples.waveWindAngle = Float32Array.from([30, 60]);
  api.polarScene.mockResolvedValue(data);
  await render();
  const select = feature("view3d:colour") as HTMLSelectElement;
  for (const value of ["wavePeriod", "waveAngle", "waveWindAngle"]) {
    expect([...select.options].find((o) => o.value === value)?.disabled).toBe(false);
  }
  await act(async () => { select.value = "time"; select.dispatchEvent(new Event("change", { bubbles: true })); });
  expect(q(".view3d-ramp")?.textContent).toBe("2026-09-30 12:00:00 UTC2026-09-30 12:10:00 UTC");

  // By time of day there is no ramp: the four bands and their hours instead.
  await act(async () => { select.value = "timeOfDay"; select.dispatchEvent(new Event("change", { bubbles: true })); });
  expect(q(".view3d-ramp")).toBeNull();
  expect([...host.querySelectorAll(".view3d-bands li")].map((item) => item.textContent)).toEqual([
    "Night 21:00–05:00", "Morning 05:00–12:00", "Afternoon 12:00–17:00", "Evening 17:00–21:00",
  ]);
});


it("previews wave limits immediately and sends them to the native blend", async () => {
  const data = withSamples();
  data.samples.hs = Float32Array.from([0.7, 1.2]);
  data.samples.waveAngle = Float32Array.from([30, 120]);
  data.samples.wavePeriod = Float32Array.from([7, 9]);
  api.polarScene.mockResolvedValue(data);
  await render();
  const scene = scenes.made[0]!;
  const last = () => scene.setData.mock.calls.at(-1)![0];
  const originalSurfaces = last().surfaces;
  const slide = async (id: string, value: number) => {
    const input = feature(id) as unknown as HTMLInputElement;
    await act(async () => {
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(input, String(value));
      input.dispatchEvent(new Event("input", { bubbles: true }));
    });
  };
  expect(last().samples.length / 3).toBe(3); // Two nodes, one unfiltered sample.
  await slide("view3d:wave-height-min", 0.8);
  expect(last().samples.length / 3).toBe(2);
  expect(q(".wave-range-footer")!.textContent).toContain("0 samples shown");
  await slide("view3d:wave-height-min", 0.7);
  expect(last().samples.length / 3).toBe(3);
  await slide("view3d:wave-angle-max", 20);
  expect(last().samples.length / 3).toBe(2);
  await slide("view3d:wave-angle-min", 50); // Lower handle stops at upper.
  expect((feature("view3d:wave-angle-min") as unknown as HTMLInputElement).value).toBe("20");
  await act(async () => feature("view3d:wave-ranges-reset")!.click());
  expect(last().samples.length / 3).toBe(3);
  await slide("view3d:wave-period-max", 6);
  expect(last().samples.length / 3).toBe(2);
  expect(last().surfaces).toEqual(originalSurfaces);
  expect(api.polarScene).toHaveBeenCalledTimes(1);
  expect(api.setWaveRanges).toHaveBeenLastCalledWith({ hs: { min: null, max: null }, waveAngle: { min: null, max: null }, wavePeriod: { min: null, max: 6 } });
  expect(onProject).toHaveBeenCalled();
  expect(api.setExcluded).not.toHaveBeenCalled();
  await render(1, 2);
  expect(last().samples.length / 3).toBe(3); // A new project resets view limits.
});

it("offers disabled wave sliders when the scene has no wave measurements", async () => {
  await render();
  for (const metric of ["height", "angle", "period"]) {
    expect((feature(`view3d:wave-${metric}-min`)!.closest("fieldset") as HTMLFieldSetElement).disabled).toBe(true);
  }
  expect(feature("view3d:wave-ranges-reset")!.disabled).toBe(true);
});

/** Two polar nodes and four track samples: waves from 10°, 100° and 350° off the bow, and one with no waves. */
function withWaves(): ScenePacket {
  const base = packet();
  return {
    ...base,
    sources: [...base.sources, { id: 30, colour: "#0000ff", kind: "track" }],
    samples: {
      count: 4, points: Float32Array.from([90, 12, 7, 91, 12.2, 6.5, 60, 8, 5, 89, 11.8, 7.4]), source: Uint32Array.from([1, 1, 1, 1]),
      ids: Uint32Array.from([5, 0, 6, 0, 7, 0, 8, 0]), hs: Float32Array.from([1, 1.5, Number.NaN, 2]),
      current: new Float32Array(4).fill(Number.NaN), time: Float32Array.from([0, 60, 120, 180]),
      wavePeriod: Float32Array.from([7, 8, Number.NaN, 9]), waveAngle: Float32Array.from([10, 100, Number.NaN, 10]),
      waveWindAngle: Float32Array.from([20, 30, Number.NaN, 40]), waveBearing: Float32Array.from([10, 100, Number.NaN, 350]),
      flags: Uint32Array.from([0, 0, 0, 0]),
    },
  };
}

const lastCells = () => scenes.made[0]!.setCells.mock.calls.at(-1)![0] as { of: Int16Array; count: number } | null;
const setValue = async (id: string, value: string, event: "input" | "change") => {
  const input = feature(id) as unknown as HTMLInputElement;
  await act(async () => {
    const proto = input instanceof HTMLSelectElement ? HTMLSelectElement.prototype : HTMLInputElement.prototype;
    Object.getOwnPropertyDescriptor(proto, "value")!.set!.call(input, value);
    input.dispatchEvent(new Event(event, { bubbles: true }));
  });
};

it("offers Split Wave Angle beside the tools in single view only, off until asked for", async () => {
  api.polarScene.mockResolvedValue(withWaves());
  await render();
  const toggle = feature("view3d:wave-split")!;
  expect(toggle.textContent).toBe("Split Wave Angle");
  expect(toggle.getAttribute("aria-pressed")).toBe("false");
  // After the Box tool, in the toolbar.
  const order = [...q(".view3d-toolbar")!.querySelectorAll("[data-feature]")].map((el) => el.getAttribute("data-feature"));
  expect(order.indexOf("view3d:wave-split")).toBe(order.indexOf("view3d:tool-box") + 1);
  expect(feature("view3d:wave-split-count")).toBeNull();
  expect(feature("view3d:wave-split-sense")).toBeNull();
  expect(q(".wave-split-cell")).toBeNull();
  expect(lastCells()).toBeNull();
  // A pane of split or four-way view does not offer it.
  await act(async () => root.render(<PolarView project={project(1)} settings={null} onProject={onProject} comparison />));
  expect(feature("view3d:wave-split")).toBeNull();
});

it("splits the view into one copy per wave direction, each with its own samples and an arrow (spec.md 10.5)", async () => {
  api.polarScene.mockResolvedValue(withWaves());
  await render();
  await act(async () => feature("view3d:wave-split")!.click());
  expect(feature("view3d:wave-split")!.getAttribute("aria-pressed")).toBe("true");
  // The slider's stops are the six counts; it starts at eight directions.
  const slider = feature("view3d:wave-split-count") as unknown as HTMLInputElement;
  expect([slider.type, slider.min, slider.max, slider.step, slider.value]).toEqual(["range", "0", "5", "1", "1"]);
  expect(q(".wave-split-count")!.textContent).toBe("8 directions");
  // Nodes in every copy; waves from 10° and 350° off the bow in the bow's copy,
  // 100° in the starboard beam's (the third of eight); no waves, no copy.
  expect(lastCells()!.count).toBe(8);
  expect([...lastCells()!.of]).toEqual([-1, -1, 0, 2, -2, 0]);
  // One frame per copy, each saying its direction, how many samples it holds, and pointing its arrow.
  const cells = [...host.querySelectorAll<HTMLElement>(".wave-split-cell")];
  expect(cells).toHaveLength(8);
  expect(cells.map((cell) => cell.querySelector(".wave-split-name")!.textContent)).toEqual(
    ["From 0°", "From 45°", "From 90°", "From 135°", "From 180°", "From 225°", "From 270°", "From 315°"]);
  expect(cells.map((cell) => cell.querySelector(".wave-split-total")!.textContent)).toEqual(
    ["2 samples", "0 samples", "1 samples", "0 samples", "0 samples", "0 samples", "0 samples", "0 samples"]);
  expect(cells.map((cell) => cell.querySelector<SVGElement>(".wave-split-arrow")!.dataset.direction)).toEqual(
    ["0", "45", "90", "135", "180", "225", "270", "315"]);
  expect(cells[2]!.querySelector(".wave-split-arrow")!.getAttribute("data-sense")).toBe("from");
  // Each copy's share of the circle, an arc round the boat: 45° of it, centred on its direction.
  const arc = cells[2]!.querySelector(".wave-split-arc")!;
  expect([arc.getAttribute("data-from"), arc.getAttribute("data-to")]).toEqual(["67.5", "112.5"]);
  // The sample with no wave direction is in no copy, and the controls say so.
  expect(q(".wave-split-none")!.textContent).toBe("1 without a wave direction");

  // Four directions: bow, starboard beam, stern, port beam.
  await setValue("view3d:wave-split-count", "0", "input");
  expect(q(".wave-split-count")!.textContent).toBe("4 directions");
  expect(lastCells()!.count).toBe(4);
  expect([...lastCells()!.of]).toEqual([-1, -1, 0, 1, -2, 0]);
  expect(host.querySelectorAll(".wave-split-cell")).toHaveLength(4);
  expect(q(".wave-split-arc")!.getAttribute("data-from")).toBe("315");
  // Thirty-six, the last stop.
  await setValue("view3d:wave-split-count", "5", "input");
  expect(lastCells()!.count).toBe(36);
  expect(host.querySelectorAll(".wave-split-cell")).toHaveLength(36);

  // To: the copies are where the waves go. From 10° off the bow they go to 190°.
  await setValue("view3d:wave-split-count", "0", "input");
  await setValue("view3d:wave-split-sense", "to", "change");
  expect([...lastCells()!.of]).toEqual([-1, -1, 2, 3, -2, 2]);
  expect(q(".wave-split-cell .wave-split-name")!.textContent).toBe("To 0°");
  expect(q(".wave-split-arrow")!.getAttribute("data-sense")).toBe("to");

  // Off again: one view.
  await act(async () => feature("view3d:wave-split")!.click());
  expect(lastCells()).toBeNull();
  expect(q(".wave-split-cell")).toBeNull();
  expect(feature("view3d:wave-split-count")).toBeNull();
});

it("keeps the wave range filters in force in every copy", async () => {
  api.polarScene.mockResolvedValue(withWaves());
  await render();
  await act(async () => feature("view3d:wave-split")!.click());
  await setValue("view3d:wave-split-count", "0", "input");
  expect([...lastCells()!.of]).toEqual([-1, -1, 0, 1, -2, 0]);
  // Waves of 1.2 m and more only: the 1 m sample leaves its copy, and the one with no height is not drawn at all.
  await setValue("view3d:wave-height-min", "1.2", "input");
  expect([...lastCells()!.of]).toEqual([-1, -1, 1, 0]);
  expect([...host.querySelectorAll(".wave-split-total")].map((el) => el.textContent)).toEqual(["1 samples", "1 samples", "0 samples", "0 samples"]);
  // Every sample still drawn has a direction: nothing to say.
  expect(q(".wave-split-none")).toBeNull();
});

it("shows, for a dot hovered in one copy, the dot at the same wind in the other copies", async () => {
  api.polarScene.mockResolvedValue(withWaves());
  await render();
  await act(async () => feature("view3d:wave-split")!.click());
  await setValue("view3d:wave-split-count", "0", "input");
  // Drawn dots: nodes 0 and 1, then samples: 2 (copy 0), 3 (copy 1), 4 (none), 5 (copy 0).
  scenes.cell = 0;
  scenes.pick = 2;
  await act(async () => {
    q("canvas")!.dispatchEvent(new PointerEvent("pointermove", { bubbles: true, clientX: 60, clientY: 70, buttons: 0 }));
    await new Promise((r) => setTimeout(r, 30));
  });
  // The full tooltip where the pointer is, and a mark with its speed in the starboard copy, whose sample is 1° and 0.2 kn away.
  expect(q('[role="tooltip"]')?.textContent).toContain("12.0 kn");
  const marks = [...host.querySelectorAll<HTMLElement>(".wave-split-mark")];
  expect(marks).toHaveLength(1);
  expect(marks[0]!.dataset.cell).toBe("1");
  expect(marks[0]!.textContent).toBe("6.5 kn");
  expect(marks[0]!.style.left).toBe("150px");
  // Leaving clears them.
  await act(async () => { q("canvas")!.dispatchEvent(new PointerEvent("pointerout", { bubbles: true })); });
  expect(q(".wave-split-mark")).toBeNull();
});

it("draws each copy's own blend, made from its direction's samples, and reads a hovered cell from it", async () => {
  const base = withWaves();
  api.polarScene.mockResolvedValue({
    ...base,
    surfaces: [...base.surfaces,
      { source: BLEND_SOURCE, twa: Float32Array.from([88, 89]), tws: Float32Array.from([11, 12]), bsp: Float32Array.from([7, 7.4, 7.2, 7.5]) }],
  });
  const grid = { twa: Float32Array.from([88, 89]), tws: Float32Array.from([11, 12]) };
  api.polarSceneSplit.mockResolvedValue({ count: 4, surfaces: [
    { cell: 0, ...grid, bsp: Float32Array.from([6, 6.4, 6.2, 6.5]) },
    { cell: 1, ...grid, bsp: Float32Array.from([7, 7.4, 7.2, 7.9]) },
  ] });
  api.blendCellSplit.mockResolvedValue({
    twa: 90, tws: 12, bsp: 7.9, origin: "direct", corrected: false,
    contributors: [{ source_id: 30, bsp: 7.9, weight: 0.2, share: 1 }],
  });
  await render();
  const last = () => scenes.made[0]!.setData.mock.calls.at(-1)![0] as { surfaces: { cell?: number; opaque?: boolean; pickable?: boolean; color: string; grid: { bsp: Float32Array } }[] };
  expect(last().surfaces).toHaveLength(2);
  await act(async () => feature("view3d:wave-split")!.click());
  await setValue("view3d:wave-split-count", "0", "input");
  await act(async () => { await new Promise((r) => setTimeout(r, 0)); });
  expect(api.polarSceneSplit).toHaveBeenLastCalledWith({ count: 4, sense: "from" });
  // The whole blend's surface gives way to one per copy, opaque and in the blend's colour.
  const split = last().surfaces;
  expect(split.map((s) => s.cell)).toEqual([undefined, 0, 1]);
  expect(split[1]!.opaque && split[1]!.pickable).toBe(true);
  expect(split[1]!.color).toBe(split[2]!.color);
  expect(split[2]!.grid.bsp[3]).toBeCloseTo(7.9, 5);
  // To asks again for the other sense.
  await setValue("view3d:wave-split-sense", "to", "change");
  await act(async () => { await new Promise((r) => setTimeout(r, 0)); });
  expect(api.polarSceneSplit).toHaveBeenLastCalledWith({ count: 4, sense: "to" });
  // Hovering copy 1's blend reads that copy's cell, and marks the other copy's own speed there.
  scenes.cell = 1;
  scenes.surface = { surface: 2, twaIndex: 1, twsIndex: 1 };
  await act(async () => {
    q("canvas")!.dispatchEvent(new PointerEvent("pointermove", { bubbles: true, clientX: 160, clientY: 70, buttons: 0 }));
    await new Promise((r) => setTimeout(r, 30));
  });
  expect(api.blendCellSplit).toHaveBeenCalledWith(10, 4, { count: 4, sense: "to" }, 1);
  expect(api.blendCell).not.toHaveBeenCalled();
  expect(q(".blend-cell-tooltip")!.textContent).toContain("7.90 kn");
  const marks = [...host.querySelectorAll<HTMLElement>(".wave-split-mark")];
  expect(marks.map((mark) => [mark.dataset.cell, mark.textContent])).toEqual([["0", "6.5 kn"]]);
  // Off: the whole blend again.
  scenes.surface = null;
  await act(async () => feature("view3d:wave-split")!.click());
  expect(last().surfaces.map((s) => s.cell)).toEqual([undefined, undefined]);
});

it("puts a linked pane's tooltip beside the dot that matches, not in the corner (asked 2026-10-02)", async () => {
  // This pane holds the dots of `withSamples`; another pane of the split
  // view, sharing the sync, hovers a point at 90° in 12 kn: the second node here.
  api.polarScene.mockResolvedValue(withSamples());
  // Drawn dots: node 0, node 1 (excluded, still drawn), sample 0, sample 1 — the fake scene places each.
  scenes.projected = [30, 40, 50, 50, 70, 60, 90, 70];
  const sync = createFleetSync();
  await act(async () => root.render(
    <FleetSyncProvider enabled sync={sync}>
      <PolarView project={{ ...project(1), id: 1 }} settings={null} onProject={onProject} />
    </FleetSyncProvider>,
  ));
  await act(async () => { sync.publish({ kind: "hover", boat: 2, point: { twa: 90, tws: 12 } }); });
  const tip = q('[role="tooltip"]') as HTMLElement;
  expect(tip.textContent).toContain("Farr 40");
  // Beside its dot (the second drawn), as the pane's own hover is: 12 px right and down.
  expect(tip.style.left).toBe("62px");
  expect(tip.style.top).toBe("62px");
  // Without a match, nothing is pointed at.
  await act(async () => { sync.publish({ kind: "hover", boat: 2, point: { twa: 150, tws: 30 } }); });
  expect(q('[role="tooltip"]')?.textContent).toContain("No point at matching wind conditions");
});
