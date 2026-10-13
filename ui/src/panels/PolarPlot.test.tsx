// @vitest-environment happy-dom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import type { PolarPlotResult } from "../generated/PolarPlotResult";
import type { ProjectSummary } from "../generated/ProjectSummary";
import type { SourceSummary } from "../generated/SourceSummary";
import { TEST_BLEND } from "../testBlend";

const polarPlotCall = vi.fn();
const blendCellCall = vi.fn();
const dotsCall = vi.fn();
vi.mock("../ipc", async () => {
  const { emptyDots } = await import("./dotPacket");
  return {
    api: {
      waveRangeMaxima: async () => [null, null, null],
      polarPlot: (tws: number | null) => polarPlotCall(tws),
      polarPlotDots: async (tws: number | null, filtered: boolean) => (await dotsCall(tws, filtered)) ?? emptyDots(),
      blendCell: (twaIndex: number, twsIndex: number) => blendCellCall(twaIndex, twsIndex),
    },
  };
});

const { default: PolarPlot } = await import("./PolarPlot");

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
let host: HTMLDivElement;
let root: Root;

const source = (overrides: Partial<SourceSummary> = {}): SourceSummary => ({
  id: 1, kind: "polar_file", label: "A", colour: "#4e79a7", visible: true, weight: 1,
  count: 4, used: null, polar_file: null, orr: null, orc: null, track: null, edits: 0, ...overrides,
});

const project = (overrides: Partial<ProjectSummary> = {}): ProjectSummary => ({
  id: 1, name: "P", path: null, dirty: false, revision: 1, boat_name: "", boat_notes: "",
  sources: [source()], can_undo: false, can_redo: false, undo_label: null, redo_label: null, use_corrected: true, stokes_drift: false, blend: TEST_BLEND,
  ...overrides,
});

const result = (overrides: Partial<PolarPlotResult> = {}): PolarPlotResult => ({
  tws_min: 6, tws_max: 20, curves: [], blend: [], band_kn: 1, ...overrides,
});

async function render(...args: Parameters<typeof PolarPlot>) {
  await act(async () => root.render(<PolarPlot {...args[0]} />));
  // The fetch effect's `.then` runs a microtask turn after `render` returns;
  // give it one more pass so `setResult` lands before assertions run.
  await act(async () => { await Promise.resolve(); });
}

beforeEach(() => {
  blendCellCall.mockReset();
  polarPlotCall.mockReset();
  dotsCall.mockReset();
  polarPlotCall.mockResolvedValue(result());
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); });

it("shows the placeholder and no controls-affecting fetch stall when there is no visible polar source", async () => {
  await render({ project: project({ sources: [] }), variant: "panel" });
  expect(host.querySelector(".plot-placeholder")?.textContent).toBe(
    "The polar plot appears here once the project has a source.",
  );
  expect(host.querySelector("canvas")).toBeNull();
});

it("fetches the plot for the project, defaulting to \"all\" (tws null)", async () => {
  await render({ project: project(), variant: "panel" });
  expect(polarPlotCall).toHaveBeenCalledWith(null);
  expect(host.querySelector('[data-feature="plot:all"]')).not.toBeNull();
  expect(host.querySelector('[data-feature="plot:tws"]')).not.toBeNull();
});

it("refetches when the project's revision changes, but not on an unrelated rerender with the same revision", async () => {
  await render({ project: project({ revision: 1 }), variant: "panel" });
  expect(polarPlotCall).toHaveBeenCalledTimes(1);
  await render({ project: project({ revision: 1 }), variant: "panel" });
  expect(polarPlotCall).toHaveBeenCalledTimes(1);
  await render({ project: project({ revision: 2 }), variant: "panel" });
  expect(polarPlotCall).toHaveBeenCalledTimes(2);
});

it("switches off \"all\" to a specific wind speed within the fetched domain", async () => {
  await render({ project: project(), variant: "panel" });
  const all = host.querySelector<HTMLInputElement>('[data-feature="plot:all"]')!;
  expect(all.checked).toBe(true);
  await act(async () => all.click());
  const slider = host.querySelector<HTMLInputElement>('[data-feature="plot:tws"]')!;
  expect(slider.disabled).toBe(false);
  const called = polarPlotCall.mock.calls.at(-1)?.[0] as number;
  expect(called).toBeGreaterThanOrEqual(6);
  expect(called).toBeLessThanOrEqual(20);
});

it("has no full-size or close button: the plot is a stage of its own (asked 2026-10-02)", async () => {
  await render({ project: project(), variant: "stage" });
  expect(host.querySelector('[data-feature="plot:full-size"]')).toBeNull();
  expect(host.querySelector('[data-feature="plot:close"]')).toBeNull();
  expect(host.querySelector(".polar-plot-stage")).not.toBeNull();
});

it("shows a message when the plot has no point at the chosen slice", async () => {
  polarPlotCall.mockResolvedValue(result({ curves: [{ source_id: 1, label: "A", colour: "#4e79a7", tws: 10, points: [] }] }));
  await render({ project: project(), variant: "panel" });
  expect(host.querySelector(".polar-plot-empty")?.textContent).toBe("No source has data at this wind speed.");
});

it("sizes the fan and the hover hit-test to the blend too (M6 carry)", async () => {
  const { plotMaxBsp } = await import("./PolarPlot");
  const { emptyDots } = await import("./dotPacket");
  const blend = { source_id: null, label: "Blend", colour: "#ff8800", tws: 10, points: [{ twa: 90, bsp: 12 }] };
  const plotted = result({ curves: [{ source_id: 1, label: "A", colour: "#4e79a7", tws: 10, points: [{ twa: 90, bsp: 8 }] }], blend: [blend] });
  expect(plotMaxBsp(plotted, emptyDots())).toBe(12);
  expect(plotMaxBsp(result({ blend: [] }), emptyDots())).toBe(0);
});

it("counts a blend-only slice as something to draw", async () => {
  polarPlotCall.mockResolvedValue(result({
    blend: [{ source_id: null, label: "Blend", colour: "#ffffff", tws: 10, points: [{ twa: 90, bsp: 7 }] }],
  }));
  await render({ project: project(), variant: "panel" });
  expect(host.querySelector(".polar-plot-empty")).toBeNull();
});

it("shows the slice's wind speed and its dot band in the display speed unit (M17b)", async () => {
  const tracked = project({ sources: [source(), source({ id: 2, kind: "track", label: "T" })] });
  await render({ project: tracked, variant: "panel", unit: "kmh" });
  await act(async () => host.querySelector<HTMLInputElement>('[data-feature="plot:all"]')!.click());
  // The slice starts mid-domain, 13 kn: 13 × 1.852 = 24.076 km/h. The band is 1 kn = 1.852 km/h.
  const value = host.querySelector<HTMLElement>(".polar-plot-tws-value")!;
  expect(value.textContent).toBe("24.1 km/h");
  expect(value.title).toBe("Sample dots within 1.85 km/h of this wind speed (Settings)");
  await render({ project: tracked, variant: "panel", unit: "ms" });
  // 13 kn × 1852 / 3600 = 6.688 m/s.
  expect(host.querySelector(".polar-plot-tws-value")!.textContent).toBe("6.7 m/s");
  await render({ project: tracked, variant: "panel" });
  expect(host.querySelector(".polar-plot-tws-value")!.textContent).toBe("13 kn");
});

it("colours the dots by source until asked for the time of day, then shows the four bands (spec.md 9.2)", async () => {
  const track = source({ id: 2, kind: "track", label: "Race", colour: "#e15759" });
  await render({ project: project({ sources: [source(), track] }), variant: "panel" });
  const colour = host.querySelector<HTMLSelectElement>('[data-feature="plot:colour"]')!;
  expect(colour.disabled).toBe(false);
  expect(colour.value).toBe("source");
  expect(host.querySelector(".polar-plot-bands")).toBeNull();

  await act(async () => {
    colour.value = "timeOfDay";
    colour.dispatchEvent(new Event("change", { bubbles: true }));
  });
  expect([...host.querySelectorAll(".polar-plot-bands li")].map((item) => item.textContent)).toEqual([
    "Night 21:00–05:00", "Morning 05:00–12:00", "Afternoon 12:00–17:00", "Evening 17:00–21:00",
  ]);
});

it("offers no dot colouring while no track is shown", async () => {
  await render({ project: project(), variant: "panel" });
  const colour = host.querySelector<HTMLSelectElement>('[data-feature="plot:colour"]')!;
  expect(colour.disabled).toBe(true);
  expect(colour.title).toBe("No visible track has samples to colour");
});

/**
 * A 300 × 300 canvas: with the widest speed 7 kn the fan's centre is at
 * (28, 150) and one knot is 122 / 7 px, so TWA 90° at 7 kn is at (150, 150).
 */
async function moveOver(x: number, y: number) {
  const canvas = host.querySelector("canvas")!;
  canvas.getBoundingClientRect = () => ({ left: 0, top: 0, right: 300, bottom: 300, width: 300, height: 300, x: 0, y: 0, toJSON: () => ({}) });
  await act(async () => {
    canvas.dispatchEvent(new MouseEvent("mousemove", { bubbles: true, clientX: x, clientY: y }));
    await Promise.resolve();
  });
}
const blendAt = (tws: number) => result({
  blend: [{ source_id: null, label: "Blend", colour: "#e0457b", tws, points: [{ twa: 90, bsp: 7 }] }],
});

it("names the sources behind a blend cell hovered on the plot (spec.md 9.2)", async () => {
  // 90° is the output grid's TWA index 10 and 10 kn its TWS index 3.
  polarPlotCall.mockResolvedValue(blendAt(10));
  blendCellCall.mockResolvedValue({
    twa: 90, tws: 10, bsp: 7, origin: "direct", corrected: false,
    contributors: [{ source_id: 1, bsp: 7, weight: 1, share: 1 }],
  });
  await render({ project: project(), variant: "panel" });
  await moveOver(150, 150);
  expect(blendCellCall).toHaveBeenCalledWith(10, 3);
  const tip = host.querySelector(".blend-cell-tooltip")!;
  expect(tip.textContent).toContain("Blend");
  expect(tip.querySelector(".blend-cell-sources")?.textContent).toBe("A7.00 kn100%");
  // The same cell again asks Rust nothing more.
  await moveOver(151, 150);
  expect(blendCellCall).toHaveBeenCalledTimes(1);
});

it("keeps the plain tooltip for a blend point that is not an output-grid cell", async () => {
  // A slice at 11 kn lies between the grid's 10 and 12 kn: no cell to name.
  polarPlotCall.mockResolvedValue(blendAt(11));
  await render({ project: project(), variant: "panel" });
  await moveOver(150, 150);
  expect(blendCellCall).not.toHaveBeenCalled();
  expect(host.querySelector(".blend-cell-tooltip")).toBeNull();
  expect(host.querySelector(".polar-plot-tooltip")?.textContent).toBe("BlendTWA 90°, TWS 11.0 kn, BSP 7.00 kn");
});

/**
 * Two curves at 10 kn of wind: A reads 6 kn at 60°, 7 at 90°, 6.5 at 120°;
 * the blend 6.6, 7.7, 7.0. The widest speed is 7.7 kn, which sizes the fan.
 */
const measured = () => result({
  curves: [{ source_id: 1, label: "A", colour: "#4e79a7", tws: 10, points: [{ twa: 60, bsp: 6 }, { twa: 90, bsp: 7 }, { twa: 120, bsp: 6.5 }] }],
  blend: [{ source_id: null, label: "Blend", colour: "#e0457b", tws: 10, points: [{ twa: 60, bsp: 6.6 }, { twa: 90, bsp: 7.7 }, { twa: 120, bsp: 7 }] }],
});
/** Where (TWA, BSP) is drawn on the 300 × 300 canvas of `moveOver`. */
async function at(twa: number, bsp: number) {
  const { fitLayout, project: place } = await import("./plotGeometry");
  return place(twa, bsp, fitLayout(300, 300, 7.7, 28));
}
const measureButton = () => host.querySelector<HTMLButtonElement>('[data-feature="plot:measure"]')!;
const readout = () => host.querySelector<HTMLElement>(".polar-plot-measure");
const readoutRows = () => [...host.querySelectorAll(".polar-plot-measure li")].map((row) => row.textContent);
async function clickCanvas(x: number, y: number) {
  const canvas = host.querySelector("canvas")!;
  await act(async () => { canvas.dispatchEvent(new MouseEvent("click", { bubbles: true, clientX: x, clientY: y })); });
}

it("measures every curve at the pointer's wind angle against the one pointed at (spec.md 9.2)", async () => {
  polarPlotCall.mockResolvedValue(measured());
  await render({ project: project(), variant: "panel" });
  expect(measureButton().getAttribute("aria-pressed")).toBe("false");
  expect(readout()).toBeNull();

  await act(async () => measureButton().click());
  expect(measureButton().getAttribute("aria-pressed")).toBe("true");
  // On A's curve at 90°: the blend is 0.70 kn, 10.0 %, faster there.
  const onA = await at(90, 7);
  await moveOver(onA.x, onA.y);
  expect(readout()?.querySelector("h4")?.textContent).toBe("At TWA 90°");
  expect(readoutRows()).toEqual(["Blend · 10 kn7.70 kn+0.70 kn+10.0%", "A · 10 kn7.00 knpointed at"]);
  // The ordinary hover tooltip stays out of the way while measuring.
  expect(host.querySelector(".polar-plot-tooltip")).toBeNull();
  expect(blendCellCall).not.toHaveBeenCalled();

  // Half way to 120°, nearer the blend: A is 6.75 kn, the blend 7.35 kn.
  const onBlend = await at(105, 7.35);
  await moveOver(onBlend.x, onBlend.y);
  expect(readout()?.querySelector("h4")?.textContent).toBe("At TWA 105°");
  expect(readoutRows()).toEqual(["Blend · 10 kn7.35 knpointed at", "A · 10 kn6.75 kn−0.60 kn−8.2%"]);

  // Outside every curve's angles there is nothing to compare.
  const outside = await at(30, 5);
  await moveOver(outside.x, outside.y);
  expect(readoutRows()).toEqual([]);
  expect(readout()?.textContent).toContain("No curve at this angle");

  await act(async () => measureButton().click());
  expect(readout()).toBeNull();
});

it("pins a point and measures from it to the pointer; Escape lets it go", async () => {
  polarPlotCall.mockResolvedValue(measured());
  await render({ project: project(), variant: "panel" });
  await act(async () => measureButton().click());
  const a = await at(60, 6);
  await moveOver(a.x, a.y);
  expect(readout()?.querySelector(".polar-plot-measure-pin")).toBeNull();
  await clickCanvas(a.x, a.y);
  // To the blend at 90°: 7.70 − 6.00 = +1.70 kn, 7.7 / 6 = +28.3 %, 30° on.
  const b = await at(90, 7.7);
  await moveOver(b.x, b.y);
  expect(readout()?.querySelector(".polar-plot-measure-pin")?.textContent)
    .toBe("A: 6.00 kn at 60°B − A: +1.70 kn (+28.3%), 30° apart");

  await act(async () => { window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })); });
  expect(readout()?.querySelector(".polar-plot-measure-pin")).toBeNull();
  expect(readout()).not.toBeNull();
});

it("shows measurements in the display speed unit", async () => {
  polarPlotCall.mockResolvedValue(measured());
  await render({ project: project(), variant: "panel", unit: "kmh" });
  await act(async () => measureButton().click());
  const onA = await at(90, 7);
  await moveOver(onA.x, onA.y);
  // 7.7 kn = 14.26 km/h, 7 kn = 12.96 km/h, 0.7 kn = 1.30 km/h; the wind 18.5 km/h.
  expect(readoutRows()).toEqual(["Blend · 18.5 km/h14.26 km/h+1.30 km/h+10.0%", "A · 18.5 km/h12.96 km/hpointed at"]);
});

it("offers no measuring while the plot has nothing drawn", async () => {
  await render({ project: project(), variant: "panel" });
  expect(measureButton().disabled).toBe(true);
});

it("hides excluded dots behind a checkbox of their own, without asking Rust again (asked 2026-10-04)", async () => {
  const track = source({ id: 9, kind: "track" });
  const withFlags = (flags: number[]) => ({
    count: flags.length, sources: [9], points: Float32Array.from(flags.flatMap((_, k) => [90, 10, 5 + k])),
    source: new Uint32Array(flags.length), ids: Uint32Array.from(flags.flatMap((_, k) => [k, 0])), flags: Uint32Array.from(flags),
  });
  dotsCall.mockResolvedValue(withFlags([0, 1]));
  await render({ project: project({ sources: [track] }), variant: "stage" });
  const box = host.querySelector('[data-feature="plot:show-excluded"]') as HTMLInputElement;
  expect(box.checked).toBe(false);
  expect(box.disabled).toBe(false);
  const asked = dotsCall.mock.calls.length;
  await act(async () => box.click());
  expect(box.checked).toBe(true);
  expect(dotsCall.mock.calls.length).toBe(asked);
  // Nothing excluded: nothing to show.
  await act(async () => box.click());
  dotsCall.mockResolvedValue(withFlags([0, 0]));
  await render({ project: project({ sources: [track], revision: 2 }), variant: "stage" });
  expect((host.querySelector('[data-feature="plot:show-excluded"]') as HTMLInputElement).disabled).toBe(true);
});
