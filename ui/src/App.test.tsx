// @vitest-environment happy-dom
/**
 * The shell end to end, against a mocked backend: the start screen, the
 * project window's layout, the save guard on every way a project is put
 * down (quitting included), and the two acceptance criteria of plan.md M2 —
 * a language switch relabels everything with no reload, and every control is
 * found by the search in each language and flashed after it is revealed.
 */
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { AppSettings } from "./generated/AppSettings";
import type { OrcHit } from "./generated/OrcHit";
import type { ProjectSummary } from "./generated/ProjectSummary";
import type { SourceSummary } from "./generated/SourceSummary";
import { resetCompareChoices } from "./compare/compareState";
import { TEST_BLEND } from "./testBlend";

const invoke = vi.hoisted(() => vi.fn());
const events = vi.hoisted(() => new Map<string, () => void>());
const dialog = vi.hoisted(() => ({ open: vi.fn(), save: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: async (name: string, handler: () => void) => {
    events.set(name, handler);
    return () => events.delete(name);
  },
}));
vi.mock("@tauri-apps/plugin-dialog", () => dialog);

const { default: App } = await import("./App");
const { CATALOGUES, LANGUAGES, language, setLanguage, t } = await import("./i18n");
const { FEATURES } = await import("./help/features");
const { QUIT_REQUESTED } = await import("./ipc");

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

// happy-dom lays nothing out: every connected element gets a box, so
// "on screen" means "rendered", which is exactly what a reveal step changes.
HTMLElement.prototype.getBoundingClientRect = function (this: HTMLElement) {
  return this.isConnected ? new DOMRect(10, 10, 80, 20) : new DOMRect(0, 0, 0, 0);
};
HTMLElement.prototype.scrollIntoView = () => undefined;

/** An imported Adrena polar, as the source list and Polar files section show it. */
const POLAR: SourceSummary = {
  id: 7, kind: "polar_file", label: "boat.pol", colour: "#4e79a7", visible: true, weight: 1, count: 71, used: null,
  polar_file: { format: "adrena", file_name: "boat.pol", twa: [0, 42.5, 180], tws: [6, 8, 20] }, orr: null, orc: null, track: null, edits: 0,
};
const TRACK: SourceSummary = {
  id: 8, kind: "track", label: "Fastnet 2025", colour: "#f28e2b", visible: false, weight: 0.5, count: 120, used: 100,
  polar_file: null, orr: null, orc: null, track: null, edits: 0,
};
/** A visible track with its summary, as the Tracks section lists it. */
const TRACKED: SourceSummary = {
  ...TRACK, id: 10, visible: true, track: {
    origin: "file", boat_name: "Alpha", event_title: "race.geojson", start: 1_753_531_200, end: 1_753_617_600,
    samples: 120, filtered: 20, excluded: 0, used: 100, with_wind: 0, env_status: "not_fetched", env_fetched: 0, env_interval: null, no_tide: 0, max_gap_s: 10_800,
    // The track gives wind, headings and speeds, so every provided-or-derived choice is on screen to be found.
    prefer_heading: "given", prefer_speed: "given", supplied_wind: 40, supplied_heading: 120, supplied_speed: 120, downloaded_wind_only: false, environment_filters: false,
    filters: { max_awa_change: null, max_wind_speed_change: null, max_wind_direction_change: null, time_start: null, time_end: null, min_bsp: 1, max_bsp: null, max_heading_change: 30, heading_from: null, heading_to: null, cog_from: null, cog_to: null, vmg_min: null, vmg_max: null, twd_from: null, twd_to: null, exclude_tacks: false,
    tws_min: null, tws_max: null, twa_min: null, twa_max: null, hs_min: null, hs_max: null, current_min: null, current_max: null,
    wave_mode: "off", wave_sectors: [], wave_min: null, wave_max: null, wave_from: null, wave_to: null, exclude_no_tide: false, exclude_unknown_wave: false, exclude_unknown_current: false },
  },
};
/** An ORC certificate, as the source list and the ORC polars section show it. */
const ORC: SourceSummary = {
  id: 9, kind: "orc", label: "Eratosthenes", colour: "#e15759", visible: true, weight: 1, count: 70, used: null,
  polar_file: null, orr: null, orc: { sail_no: "GBR 1124", model: "Swan 112", year: 1999, certificate_year: 2023 }, track: null, edits: 0,
};
/** A search result for it. */
const HIT: OrcHit = {
  id: 4242, name: "Eratosthenes", sail_no: "GBR 1124", country: "GBR", model: "Swan 112", builder: "Nautor",
  year: 1999, certificate_year: 2023, ref_no: null, in_project: false,
  thumb: [{ tws: 6, twa: [52, 90, 150], bsp: [6.87, 7.75, 5.17] }],
};

function summary(dirty: boolean, path: string | null = null, sources: SourceSummary[] = []): ProjectSummary {
  return {
    id: 1, name: "Fastnet", path, dirty, revision: 1, boat_name: "", boat_notes: "", sources,
    can_undo: false, can_redo: false, undo_label: null, redo_label: null, use_corrected: true, stokes_drift: false, blend: TEST_BLEND,
  };
}

let settings: AppSettings;
let project: ProjectSummary | null;
let calls: [string, unknown][];
/** What `legacy_cache_notice` answers: an earlier version's chunk cache. */
let legacyNotice: { path: string; bytes: number } | null;
/** How `compare_polars` answers: the fixture, never (a request in flight), or a refusal. */
let compareAnswer: "fixture" | "pending" | "refuse";

function backend() {
  invoke.mockImplementation(async (command: string, args?: Record<string, unknown>) => {
    calls.push([command, args]);
    switch (command) {
      case "app_settings": return settings;
      case "library_metadata_available": return true;
      case "weather_cache_status": return { directory: "/cache/whirlwind-hindsight-v1", bytes: 0, max_bytes: 1e10 };
      case "clear_weather_cache": return { directory: "/cache/whirlwind-hindsight-v1", bytes: 0, max_bytes: 1e10 };
      case "set_weather_cache": settings = { ...settings, weather_cache: { directory: args?.directory as string, max_size_gb: args?.maxSizeGb as number } }; return settings;
      case "database_job_status": return { running: false, operation: "", failures: [], done: 0, total: 0, tracks: 0, skipped: 0, failed: 0, current: "", cancelled: false, error: null };
      case "app_info": return { name: "PolarExplorer", version: "0.1.0" };
      // The MCP service, off: its section shows the switch and the port.
      case "mcp_status": return { enabled: false, port: 47392, token: "", bound_port: null, bind_error: null, sessions: 0, last_tool: null, clients: ["claude_code", "codex", "claude_desktop"] };
      case "project_summary": return project;
      case "recent_projects": return [
        { path: "/boats/Fastnet.wpsproj", name: "Fastnet", exists: true },
        { path: "/gone/Old.wpsproj", name: "Old", exists: false },
      ];
      case "forget_recent_project": return [{ path: "/boats/Fastnet.wpsproj", name: "Fastnet", exists: true }];
      case "recovered_projects": return [{ id: 9, name: "Lost", original_path: null, saved_unix_s: 0 }];
      case "new_project": project = summary(true); return project;
      case "open_project":
        // Another file is another project, with its own id.
        project = { ...summary(false, args?.path as string), id: args?.path === "/other.wpsproj" ? 2 : 1 };
        return project;
      case "close_project":
        if (project?.dirty && !args?.discardUnsaved) throw { kind: "unsaved-changes", message: "unsaved" };
        project = null; return null;
      case "save_project_as": project = summary(false, args?.path as string); return project;
      case "save_project": project = summary(false, project?.path ?? null); return project;
      case "set_language": settings = { ...settings, language: args?.language as string }; return settings;
      case "set_theme": settings = { ...settings, theme: args?.theme as string }; return settings;
      case "legacy_cache_notice": return legacyNotice;
      case "remove_old_chunk_cache": return null;
      case "quit_app": return null;
      case "env_jobs": return { tracks: [], failure: null, warning: null };
      case "cancel_env_fetch": {
        // Rust stops the fetch and reports the emptied queue.
        const idle = { tracks: [], failure: null, warning: null };
        (events.get("env://progress") as ((e: { payload: unknown }) => void) | undefined)?.({ payload: idle });
        return idle;
      }
      case "polar_scene": {
        // An empty scene: the header alone (layout in polar/scenePacket.ts).
        const header = new ArrayBuffer(48);
        new DataView(header).setUint32(0, 0x44334550, true);
        new DataView(header).setUint32(4, 5, true);
        return header;
      }
      case "compare_polars": {
        if (compareAnswer === "pending") return new Promise(() => undefined);
        if (compareAnswer === "refuse") throw { kind: "bad-option", message: "Compare operand cannot be \"gone\"." };
        // The comparison the Rust packing test pins (layout in compare/comparePacket.ts).
        const bytes = readFileSync(join(dirname(fileURLToPath(import.meta.url)), "compare/fixtures/compare-v2.bin"));
        return bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength);
      }
      case "polar_plot_dots": {
        // No dots: the header alone (layout in panels/dotPacket.ts).
        const header = new ArrayBuffer(16);
        new DataView(header).setUint32(0, 0x44324550, true);
        new DataView(header).setUint32(4, 2, true);
        return header;
      }
      case "polar_edit_surface": {
        const source = project?.sources.find((s) => s.id === args?.sourceId);
        const grid = source?.polar_file ?? { twa: [45, 90], tws: [8] };
        const rows = grid.twa.map(() => grid.tws.map(() => 5));
        const flags = grid.twa.map(() => grid.tws.map(() => false));
        return {
          source_id: args?.sourceId, kind: source?.kind ?? "polar_file", twa: grid.twa, tws: grid.tws, source: rows, bsp: rows,
          edited: flags, excluded: flags, count: null, spread: null, statistic: null, min_samples: 5, edit_count: 0,
        };
      }
      case "map_tracks": {
        // No tracks: the header alone (layout in map/trackPacket.ts).
        const header = new ArrayBuffer(16);
        new DataView(header).setUint32(0, 0x544d4550, true);
        new DataView(header).setUint32(4, 1, true);
        return header;
      }
      case "import_polar_files":
        project = summary(true, project?.path ?? null, [...(project?.sources ?? []), POLAR]);
        return {
          project, imported: ["boat.pol"],
          failures: [{ file: "bad.csv", line: 2, column: 6, reason: "not-a-number", message: "bad.csv, line 2, column 6: \"x\" is not a number" }],
        };
      case "set_blend_visible":
        project = project && { ...project, blend: { ...project.blend, visible: args?.visible as boolean } };
        return project;
      case "set_blend_colour": case "set_blend_settings": return project;
      case "export_preview": {
        const axes = args?.axes as { twa: number[]; tws: number[] } | null;
        const twa = axes?.twa ?? [0, 90];
        const tws = axes?.tws ?? [10];
        return {
          format: args?.format, twa, tws, bsp: twa.map((a) => tws.map(() => (a === 0 ? 0 : 7.5))),
          origin: axes ? null : twa.map((a) => tws.map(() => (a === 0 ? "filled" : "direct"))),
          text: "TWA\\TWS\t10\n0\t0.00\n90\t7.50\n", problem: null,
        };
      }
      case "export_polar": return { path: args?.path, bytes: 31 };
      case "set_source_label": case "set_source_visible": case "set_source_colour": case "set_source_weight":
      case "move_source": case "remove_source": case "edit_polar": case "set_segment_statistic":
        return project;
      case "orc_catalogue_info": return {
        records: 18135, scraped: 0, scraped_at: null, source: "jieter/orc-data", commit: "c2ca870c6b22cc02c25afd5bac0f2d8297bf95de",
        commit_date: "2026-09-28", build_date: "2026-09-28", countries: ["GBR", "NED"], year_min: 1900, year_max: 2026,
      };
      case "orc_search": {
        const inProject = project?.sources.some((s) => s.kind === "orc") ?? false;
        return { total: 120, hits: [{ ...HIT, in_project: inProject }] };
      }
      // Both catalogues are searched at once (asked 2026-10-02); the ORR one has nothing here.
      case "orr_catalogue_info": return { records: 600, scraped_at: null };
      case "orr_search": return { total: 0, hits: [] };
      case "orc_add":
        if (!args?.allowDuplicate && project?.sources.some((s) => s.kind === "orc")) {
          throw { kind: "orc-duplicate", message: "The project already holds the certificate of Eratosthenes." };
        }
        project = {
          ...summary(true, project?.path ?? null, [...(project?.sources ?? []), { ...ORC, id: 9 + (project?.sources.length ?? 0) }]),
          revision: (project?.revision ?? 0) + 1,
        };
        return project;
      default: return null;
    }
  });
}

let host: HTMLDivElement;
let root: Root;
const settle = (ms = 30) => act(() => new Promise((resolve) => setTimeout(resolve, ms)));
async function mount() {
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
  await act(async () => root.render(<App />));
  await settle();
}
const q = <T extends Element = HTMLElement>(selector: string) => document.querySelector<T>(selector);
// What is on screen: a stage kept mounted while another shows is hidden.
const feature = (id: string) =>
  [...document.querySelectorAll<HTMLElement>(`[data-feature="${id}"]`)].find((el) => !el.closest("[hidden]")) ?? null;
const click = async (element: Element | null) => {
  expect(element).not.toBeNull();
  await act(async () => (element as HTMLElement).click());
  await settle();
};
const buttonNamed = (text: string) =>
  [...document.querySelectorAll("button")].find((b) => b.textContent?.trim() === text) ?? null;
const type = async (input: HTMLInputElement | HTMLTextAreaElement, text: string) => act(async () => {
  input.focus();
  const prototype = input instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
  Object.getOwnPropertyDescriptor(prototype, "value")!.set!.call(input, text);
  input.dispatchEvent(new Event("input", { bubbles: true }));
});

beforeEach(() => {
  localStorage.clear();
  setLanguage("en");
  calls = [];
  project = null;
  legacyNotice = null;
  compareAnswer = "fixture";
  settings = {
    library: { geojson_directory: "", metadata_directory: "", scrape_schedule: "on_demand", scrape_urls: "", yellowbrick_user_key: "", yellowbrick_device_id: "",
      database: { host: "localhost", port: 5432, name: "syrfbackendprod", user: "postgres", password: "", tls: false } },
    recent_projects: [], autosave: "recovery", language: "en", theme: "harbour",
    units: { speed: "kn", wave_height: "m", distance: "nm" },
    weather_cache: { directory: "", max_size_gb: 10 }, data_source: "open_data", weather_memory_mb: 256, network: { concurrency: 8, timeout_s: 60 },
    projection: "equirectangular", plot_tws_band_kn: 1,
    mcp: { enabled: false, port: 47392, token: "" },
    catalogues: { orc_schedule: "on_demand", orr_schedule: "on_demand" },
  };
  dialog.open.mockReset().mockResolvedValue(null);
  dialog.save.mockReset().mockResolvedValue(null);
  backend();
});
afterEach(async () => {
  resetCompareChoices();
  await act(async () => root.unmount());
  host.remove();
  document.body.innerHTML = "";
  events.clear();
  setLanguage("en");
});

describe("the start screen", () => {
  it("offers new, open, recent (missing ones greyed) and recovered work", async () => {
    await mount();
    expect(q("h1")?.textContent).toBe("PolarExplorer");
    expect(feature("start:new")).not.toBeNull();
    expect(feature("start:browse")).not.toBeNull();
    expect(feature("start:language")).not.toBeNull();
    expect(feature("start:recover")?.textContent).toContain("Lost");
    const missing = q(".recent-item.missing");
    expect(missing?.textContent).toContain("Not found");
    // A missing file is removed only after a confirmation.
    await click(missing);
    expect(q("[role=dialog]")?.textContent).toContain("/gone/Old.wpsproj");
    await click(buttonNamed("Remove"));
    expect(calls).toContainEqual(["forget_recent_project", { path: "/gone/Old.wpsproj" }]);
    expect(q(".recent-item.missing")).toBeNull();
  });

  it("creates a project and opens the project window", async () => {
    await mount();
    // The name only: the boat is named on its tab (asked 2026-10-04).
    expect(q(".new-project-form")?.querySelectorAll("input, textarea")).toHaveLength(1);
    expect(q(".new-project-form")?.textContent).not.toContain("Boat");
    await click(feature("new:create"));
    expect(calls.find(([c]) => c === "new_project")?.[1]).toEqual({
      name: "Untitled Project", boat: null, discardUnsaved: false,
    });
    for (const id of ["shell:project-menu", "shell:rename", "stage:3d", "stage:compare", "shell:search",
      "shell:help", "shell:settings", "shell:asymmetric", "nav:orc", "nav:polar-files", "nav:tracks", "panel:sources", "stage:2d",
      "shell:statusbar", "dock:left", "dock:right", "view3d:layout"]) {
      expect(feature(id), id).not.toBeNull();
    }
    expect(feature("stage:map")).toBeNull();
    expect(feature("stage:3d")?.getAttribute("aria-selected")).toBe("true");
    // The three navigation sections, in the spec's order.
    expect([...document.querySelectorAll(".left-nav h2")].map((h) => h.textContent)).toEqual(["ORC / ORR polars", "Polar files", "Tracks"]);
    expect(q(".project-name")?.textContent).toContain("•");
  });
});

describe("shortcuts on the start screen", () => {
  it("Cmd/Ctrl-N goes to the inline form; nothing is left open behind it", async () => {
    await mount();
    const { IS_MAC } = await import("./chords");
    const accel = (key: string) => act(async () => {
      window.dispatchEvent(new KeyboardEvent("keydown", { key, metaKey: IS_MAC, ctrlKey: !IS_MAC, bubbles: true }));
    });
    await accel("n");
    await settle();
    expect(document.activeElement).toBe(q('[data-feature="new:name"] input'));
    expect(q("[role=dialog]")).toBeNull();
    // The other shortcuts still work: no invisible dialog swallowed them.
    await accel(",");
    await settle();
    expect(q(".modal.settings")).not.toBeNull();
    await act(async () => window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" })));
    await settle();
    await click(feature("new:create"));
    expect(feature("shell:rename")).not.toBeNull();
    expect(q("[role=dialog]"), "no stray New Project dialog").toBeNull();
  });
});

describe("errors", () => {
  it("are shown in the interface language, with Rust's English only as the tooltip", async () => {
    settings = { ...settings, language: "fr" };
    setLanguage("fr");
    await mount();
    invoke.mockImplementationOnce(async () => {
      throw { kind: "schema-too-new", message: "file schema version 9; this build reads up to version 1" };
    });
    await click(q(".recent-item:not(.missing)"));
    const alert = q("[role=alert]");
    expect(alert?.textContent).toBe("Ce projet a été enregistré par une version plus récente de PolarExplorer. Mettez PolarExplorer à jour pour l’ouvrir.");
    expect(alert?.getAttribute("title")).toContain("file schema version 9");
    // Switching language relabels the error too.
    await act(async () => setLanguage("de"));
    expect(q("[role=alert]")?.textContent).toContain("neueren Version von PolarExplorer");
  });
});

describe("the project window", () => {
  it("keeps newer analysis revisions when an older mutation response arrives last", async () => {
    project = summary(false, "/p.wpsproj", [POLAR, ORC]);
    await mount();
    const backend = invoke.getMockImplementation()!;
    let older!: (value: ProjectSummary) => void;
    let newer!: (value: ProjectSummary) => void;
    invoke.mockImplementation((command: string, args?: Record<string, unknown>) => {
      if (command === "set_source_visible") return new Promise<ProjectSummary>((resolve) => {
        if (args?.id === POLAR.id) older = resolve; else newer = resolve;
      });
      return backend(command, args);
    });
    const boxes = () => [...document.querySelectorAll<HTMLInputElement>('[data-feature="sources:visible"]')];
    await click(boxes()[0]!);
    await click(boxes()[1]!);
    const latest = { ...project!, revision: 3, sources: [{ ...POLAR, visible: false }, { ...ORC, visible: false }] };
    await act(async () => newer(latest));
    expect(boxes().map(box => box.checked)).toEqual([false, false]);
    await act(async () => older({ ...latest, revision: 2, sources: [{ ...POLAR, visible: false }, ORC] }));
    expect(boxes().map(box => box.checked)).toEqual([false, false]);
  });

  it("folds the navigation and its sections, and remembers it for the person", async () => {
    project = summary(false, "/p.wpsproj");
    await mount();
    await click(feature("nav:tracks"));
    expect(feature("nav:tracks")?.getAttribute("aria-expanded")).toBe("false");
    await click(feature("dock:left"));
    expect(feature("nav:orc")).toBeNull();
    expect(JSON.parse(localStorage.getItem("pe.panels")!)).toMatchObject({ left: false, tracks: false });
    await click(feature("dock:left"));
    expect(feature("nav:tracks")?.getAttribute("aria-expanded")).toBe("false");
  });

  it("says once that an earlier version's chunk cache is being removed (D27)", async () => {
    project = summary(false, "/p.wpsproj");
    legacyNotice = { path: "/cache/chunks", bytes: 12_300_000_000 };
    await mount();
    await settle();
    expect(q(".statusbar")!.textContent).toContain(
      "Downloaded weather is no longer kept on disk: removing 12.3 GB an earlier version left in /cache/chunks.");
    expect(calls.filter(([c]) => c === "legacy_cache_notice")).toHaveLength(1);
    const order = calls.map(([c]) => c);
    expect(order.indexOf("remove_old_chunk_cache")).toBeGreaterThan(order.indexOf("legacy_cache_notice"));
  });

  it("switches the stage", async () => {
    project = summary(false, "/p.wpsproj", [TRACKED]);
    await mount();
    await click(feature("stage:3d"));
    expect(feature("map:projection")).toBeNull();
    expect(feature("view3d:layout")).not.toBeNull();
    expect(calls.some(([command]) => command === "polar_scene")).toBe(true);
    await click(feature("stage:compare"));
    expect(feature("view3d:layout")).toBeNull();
    expect(feature("compare:operand-a")).not.toBeNull();
    expect(calls.some(([command]) => command === "compare_polars")).toBe(true);
    await click(feature("stage:map"));
    expect(feature("map:projection")).not.toBeNull();
  });

  it("comes back to a stage as it was left, without loading it again (asked 2026-10-04)", async () => {
    project = summary(false, "/p.wpsproj", [TRACKED]);
    await mount();
    await click(feature("stage:3d"));
    const surfaces = feature("view3d:show-surfaces") as unknown as HTMLInputElement;
    expect(surfaces.checked).toBe(true);
    await click(surfaces);
    const scenes = () => calls.filter(([c]) => c === "polar_scene").length;
    const fetched = scenes();
    await click(feature("stage:2d"));
    const colour = feature("plot:colour") as unknown as HTMLSelectElement;
    await act(async () => {
      colour.value = "timeOfDay";
      colour.dispatchEvent(new Event("change", { bubbles: true }));
    });
    expect(colour.value).toBe("timeOfDay");
    for (const away of ["stage:map", "stage:compare"]) {
      await click(feature(away));
      expect(feature("view3d:layout"), away).toBeNull();
      await click(feature("stage:3d"));
      // The same view, as it was: the box still unticked, nothing fetched again.
      expect(feature("view3d:show-surfaces")).toBe(surfaces);
      expect(surfaces.checked, away).toBe(false);
      expect(scenes(), away).toBe(fetched);
      await click(feature("stage:2d"));
      expect((feature("plot:colour") as unknown as HTMLSelectElement).value, away).toBe("timeOfDay");
    }
    // An edit made while the 3D view is hidden costs it nothing until it is
    // shown again, and then it catches up once.
    await click(feature("stage:compare"));
    const changed = events.get("document://changed") as unknown as (e: { payload: unknown }) => void;
    await act(async () => changed({ payload: { project: { ...project!, revision: project!.revision + 1 }, opened: false } }));
    await settle();
    expect(scenes()).toBe(fetched);
    await click(feature("stage:3d"));
    expect(scenes()).toBe(fetched + 1);
    expect(surfaces.checked).toBe(false);
  });
});

describe("the environment fetch (spec.md 7.7)", () => {
  const progress = (payload: unknown) =>
    act(async () => (events.get("env://progress") as unknown as (e: { payload: unknown }) => void)({ payload }));

  it("shows the running fetch in the status bar, and Cancel stops it", async () => {
    project = summary(false, "/p.wpsproj");
    await mount();
    expect(feature("shell:cancel-fetch")).toBeNull();
    await progress({ tracks: [
      { source_id: 8, label: "Fastnet 2025", state: "fetching", fraction: 0.25 },
      { source_id: 9, label: "Other", state: "queued", fraction: 0 },
    ], failure: null, warning: null });
    expect(q(".statusbar")!.textContent).toContain("Fetching wind, waves and current: Fastnet 2025 25 %");
    expect(q(".statusbar")!.textContent).toContain("(1 more waiting)");
    await click(feature("shell:cancel-fetch"));
    expect(calls).toContainEqual(["cancel_env_fetch", { sourceIds: null }]);
    expect(feature("shell:cancel-fetch")).toBeNull();
  });

  it("refreshes the project when the fetch writes into it, and reports a failed fetch", async () => {
    project = summary(false, "/p.wpsproj");
    await mount();
    const before = calls.filter(([c]) => c === "project_summary").length;
    await act(async () => { events.get("env://changed")!(); events.get("env://changed")!(); });
    await settle(400);
    // Two writes close together are one refresh.
    expect(calls.filter(([c]) => c === "project_summary").length).toBe(before + 1);
    await progress({ tracks: [], failure: ["Fastnet 2025", "the archive answered 500"], warning: null });
    expect(q(".statusbar")!.textContent).toContain("The environment fetch of Fastnet 2025 stopped: the archive answered 500");
    await progress({ tracks: [], failure: null, warning: ["Fastnet 2025", "the current source cmems-nws-my-uv-geo could not be opened"] });
    expect(q(".statusbar")!.textContent).toContain("The environment fetch of Fastnet 2025 left out a current source that would not open.");
  });

  it("asks to cancel a running fetch before closing the project, and keeps both on No", async () => {
    project = summary(false, "/p.wpsproj");
    await mount();
    await progress({ tracks: [{ source_id: 8, label: "Fastnet 2025", state: "fetching", fraction: 0.5 }], failure: null, warning: null });
    await click(feature("shell:project-menu"));
    await click(feature("project:close"));
    expect(q("[role=dialog]")?.textContent).toContain("Cancel the environment fetch?");
    await click(buttonNamed("Cancel"));
    expect(calls.some(([c]) => c === "cancel_env_fetch" || c === "close_project")).toBe(false);
    await click(feature("shell:project-menu"));
    await click(feature("project:close"));
    await click(buttonNamed("Cancel the fetch"));
    await settle(150);
    expect(calls).toContainEqual(["cancel_env_fetch", { sourceIds: null }]);
    expect(calls.some(([c]) => c === "close_project")).toBe(true);
    expect(feature("start:new")).not.toBeNull();
  });
});

describe("the save guard", () => {
  it("asks before closing unsaved work, and Cancel keeps it", async () => {
    project = summary(true);
    await mount();
    await click(feature("shell:project-menu"));
    await click(feature("project:close"));
    expect(q("[role=dialog]")?.textContent).toContain("“Fastnet” has changes that have not been saved.");
    await click(buttonNamed("Cancel"));
    expect(calls.some(([c]) => c === "close_project")).toBe(false);
    expect(feature("shell:rename")).not.toBeNull();
  });

  it("Escape is Cancel", async () => {
    project = summary(true);
    await mount();
    await click(feature("shell:project-menu"));
    await click(feature("project:close"));
    await act(async () => window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" })));
    await settle();
    expect(q("[role=dialog]")).toBeNull();
    expect(calls.some(([c]) => c === "close_project")).toBe(false);
  });

  it("Don't save closes, telling Rust it was asked", async () => {
    project = summary(true);
    await mount();
    await click(feature("shell:project-menu"));
    await click(feature("project:close"));
    await click(buttonNamed("Don’t save"));
    expect(calls).toContainEqual(["close_project", { discardUnsaved: true }]);
    expect(feature("start:new")).not.toBeNull();
  });

  it("Save on a never-saved project asks where; cancelling that cancels the close", async () => {
    project = summary(true);
    await mount();
    await click(feature("shell:project-menu"));
    await click(feature("project:close"));
    await click(buttonNamed("Save"));
    expect(dialog.save).toHaveBeenCalledOnce();
    expect(calls.some(([c]) => c === "close_project")).toBe(false);
    expect(feature("shell:rename")).not.toBeNull();
  });
});

describe("quitting", () => {
  it("runs the guard when Rust reports a quit, and quits only when allowed", async () => {
    project = summary(true);
    await mount();
    const quitRequested = events.get(QUIT_REQUESTED)!;
    expect(quitRequested).toBeDefined();

    await act(async () => quitRequested());
    await settle();
    await click(buttonNamed("Cancel"));
    expect(calls.some(([c]) => c === "quit_app")).toBe(false);

    // Save, then the Save As dialog is cancelled: still no quit.
    await act(async () => quitRequested());
    await settle();
    await click(buttonNamed("Save"));
    expect(calls.some(([c]) => c === "quit_app")).toBe(false);

    await act(async () => quitRequested());
    await settle();
    await click(buttonNamed("Don’t save"));
    expect(calls).toContainEqual(["quit_app", { discardUnsaved: true }]);
  });

  it("saves first when asked to, then quits with nothing to discard", async () => {
    project = summary(true);
    dialog.save.mockResolvedValue("/boats/Fastnet.wpsproj");
    await mount();
    await act(async () => events.get(QUIT_REQUESTED)!());
    await settle();
    await click(buttonNamed("Save"));
    const order = calls.map(([c]) => c).filter((c) => c === "save_project_as" || c === "quit_app");
    expect(order).toEqual(["save_project_as", "quit_app"]);
    expect(calls).toContainEqual(["quit_app", { discardUnsaved: false }]);
  });
});

/** Every text node, tooltip and label on the page, in document order. */
function visibleStrings(): string[] {
  const out: string[] = [];
  const walk = (node: Node) => {
    if (node.nodeType === Node.TEXT_NODE) {
      const text = node.textContent?.trim();
      if (text) out.push(text);
      return;
    }
    if (node instanceof HTMLElement) {
      for (const attribute of ["title", "aria-label", "placeholder"]) {
        const value = node.getAttribute(attribute);
        if (value) out.push(value);
      }
    }
    node.childNodes.forEach(walk);
  };
  walk(document.body);
  return out;
}

describe("switching language (plan.md M2 acceptance)", () => {
  // Data, not interface: the project's name and path, the version, the
  // languages' own names, and symbols.
  const data = new Set(["Fastnet", "/boats/Fastnet.wpsproj", "v0.1.0", "PolarExplorer", "English", "Français",
    "Deutsch", "Español", "Italiano", "Nederlands", "中文", "日本語", "العربية", "/cache/chunks", "/cache/whirlwind-hindsight-v1", "…", "Old", "/gone/Old.wpsproj", "Lost", "?",
    // Country codes and the catalogue's commit, from the ORC polars section.
    "ORC", "ORR", "GBR", "NED", "c2ca870c6b22cc02c25afd5bac0f2d8297bf95de"]);
  const untranslated = (target: "fr" | "de", before: string[], after: string[]) =>
    before.filter((text, index) =>
      text === after[index] && !data.has(text) && CATALOGUES[target][text] !== text && /\p{L}{2}/u.test(text)
      && !/^(Cmd|Ctrl)\+/.test(text) && !/^\d/.test(text));
  const switchTo = async (target: string) => {
    await act(async () => {
      const picker = document.querySelector<HTMLSelectElement>(".language-picker")!;
      picker.value = target;
      picker.dispatchEvent(new Event("change", { bubbles: true }));
    });
    await settle();
  };

  for (const target of ["fr", "de"] as const) {
    it(`relabels the start screen in ${target}`, async () => {
      await mount();
      const before = visibleStrings();
      await switchTo(target);
      const after = visibleStrings();
      expect(after.length).toBe(before.length);
      expect(untranslated(target, before, after)).toEqual([]);
    });

    it(`relabels the help window in ${target}`, async () => {
      project = summary(false, "/boats/Fastnet.wpsproj");
      await mount();
      const { openHelp } = await import("./help/open");
      await act(async () => openHelp("projects"));
      const before = visibleStrings();
      await act(async () => setLanguage(target));
      await settle();
      const after = visibleStrings();
      expect(after.length).toBe(before.length);
      expect(untranslated(target, before, after)).toEqual([]);
    });

    it(`relabels every visible string and tooltip in ${target}, without a reload`, async () => {
      project = summary(true, "/boats/Fastnet.wpsproj");
      await mount();
      await click(feature("shell:project-menu"));
      await click(feature("project:open-recent"));
      await click(feature("shell:settings"));
      const before = visibleStrings();
      const page = document.body.firstElementChild;

      const picker = feature("settings:language") as HTMLSelectElement;
      await act(async () => {
        picker.value = target;
        picker.dispatchEvent(new Event("change", { bubbles: true }));
      });
      await settle();
      const after = visibleStrings();

      expect(language()).toBe(target);
      expect(document.body.firstElementChild, "the page was not reloaded").toBe(page);
      // Rust relabels the native menu as it saves the language.
      expect(calls).toContainEqual(["set_language", { language: target }]);
      expect(after.length).toBe(before.length);
      const catalogue = CATALOGUES[target];
      // Data, not interface: the project's name and path, the version, the
      // languages' own names, and symbols.
      const data = new Set(["Fastnet", "/boats/Fastnet.wpsproj", "v0.1.0", "PolarExplorer", "English", "Français",
        "Deutsch", "Español", "Italiano", "Nederlands", "中文", "日本語", "العربية", "/cache/chunks", "/cache/whirlwind-hindsight-v1", "…",
        // Country codes and the catalogue's commit, from the ORC polars section.
        "ORC", "ORR", "GBR", "NED", "c2ca870c6b22cc02c25afd5bac0f2d8297bf95de"]);
      const untranslated = before.filter((text, index) =>
        text === after[index] && !data.has(text) && catalogue[text] !== text && /\p{L}{2}/u.test(text)
        && !/^(Cmd|Ctrl)\+/.test(text));
      expect(untranslated).toEqual([]);
    });
  }
});

describe("finding every control (plan.md M2 acceptance)", () => {
  const startFeatures = FEATURES.filter((f) => /^(start|new):/.test(f.id));
  const windowFeatures = FEATURES.filter((f) => !/^(start|new):/.test(f.id));

  /** Everything folded away that can be: the search has to bring each control back. */
  function foldEverything() {
    localStorage.setItem("pe.panels", JSON.stringify({
      left: false, right: false, orc: false, polarFiles: false, tracks: false, sources: false, plot: false,
    }));
  }

  async function findAndFlash(id: string) {
    const entry = FEATURES.find((f) => f.id === id)!;
    const search = feature("shell:search") as HTMLInputElement;
    await type(search, t(entry.label));
    await settle();
    const options = [...document.querySelectorAll<HTMLLIElement>(".help-menu-popup [role=option]")];
    const match = options.find((option) =>
      option.querySelector(".help-search-label")?.textContent === t(entry.label)
      && option.querySelector(".help-search-detail")?.textContent === t(entry.description!));
    expect(match, `${id} is offered for “${t(entry.label)}”`).toBeDefined();
    document.querySelectorAll(".feature-flash").forEach((box) => box.remove());
    await act(async () => match!.click());
    await settle(entry.reveal?.length ? 250 : 80);
    const flashed = q(".feature-flash");
    expect(flashed, `${id} is flashed`).not.toBeNull();
    expect(flashed!.style.borderColor).toBe("var(--flash)");
    // A control inside a dialog that needs a choice first (the track
    // import dialog) lands on the control that opens it, as registered.
    const landing = entry.landing ?? id;
    expect(feature(landing), `${landing} is on screen`).not.toBeNull();
    expect(flashed!.style.left, `${id} flashes where ${landing} is`).not.toBe("");
  }

  // Revealing and flashing a control does not depend on the language, and
  // the sweep mounts the window once per control (about 0.5 s each): in all
  // nine languages it took the suite past CI's patience. So the sweep runs
  // in English, and every other language checks what does depend on it —
  // that the search offers each control under its translated name.
  it("finds and flashes every project-window control in en", async () => {
    settings = { ...settings, language: "en" };
    setLanguage("en");
    for (const entry of windowFeatures) {
      foldEverything();
      // Sources, so that the source list's row controls and each
      // section's Remove (and a track's filters) are on screen.
      project = summary(false, "/p.wpsproj", [POLAR, ORC, TRACKED]);
      await mount();
      // The map and 3D stages are hidden behind the Compare stage, to be
      // revealed; the Compare stage's own controls behind the 3D stage.
      await click(feature(entry.id.startsWith("compare:") ? "stage:3d" : "stage:compare"));
      // Boat tabs are hidden by the fleet comparison layout, not by a stage.
      if (entry.reveal?.includes("boats:tabs")) await click(feature("boats:split"));
      if (entry.reveal?.length) {
        expect(feature(entry.id), `${entry.id} starts hidden`).toBeNull();
      }
      await findAndFlash(entry.id);
      await act(async () => root.unmount());
      host.remove();
      document.body.innerHTML = "";
    }
    await mount();
    // One mount per control: 322 controls took 155 s on the development
    // machine at 0.3.0, and a CI runner is slower.
  }, 600_000);

  for (const { id: lang } of LANGUAGES) {
    it(`offers every project-window control under its ${lang} name`, async () => {
      settings = { ...settings, language: lang };
      setLanguage(lang);
      await mount();
      const { searchFeatures } = await import("./help/features");
      for (const entry of windowFeatures) {
        const found = searchFeatures(t(entry.label)).find((m) => m.feature.id === entry.id);
        expect(found, `${entry.id} is offered for “${t(entry.label)}”`).toBeDefined();
      }
    });

    it(`finds every start-screen control in ${lang}`, async () => {
      settings = { ...settings, language: lang };
      setLanguage(lang);
      await mount();
      const { searchFeatures } = await import("./help/features");
      const { locateFeature } = await import("./help/highlight");
      for (const entry of startFeatures) {
        expect(searchFeatures(t(entry.label)).map((m) => m.feature.id), entry.id).toContain(entry.id);
        document.querySelectorAll(".feature-flash").forEach((box) => box.remove());
        expect(await locateFeature(entry), entry.id).toBe(true);
        expect(q(".feature-flash"), entry.id).not.toBeNull();
      }
    });
  }
});

describe("polar files and the source list (plan.md M4)", () => {
  const open = async (sources: SourceSummary[]) => {
    project = summary(false, "/p.wpsproj", sources);
    await mount();
  };
  const commands = (name: string) => calls.filter(([command]) => command === name).map(([, args]) => args);

  it("imports several files at once, lists them and says which failed, and where", async () => {
    await open([]);
    dialog.open.mockResolvedValue(["/boats/boat.pol", "/boats/bad.csv"]);
    await click(feature("polar-files:import"));
    expect(dialog.open.mock.calls[0]![0]).toMatchObject({ multiple: true });
    expect(commands("import_polar_files")).toEqual([{ boatContext: 1,  paths: ["/boats/boat.pol", "/boats/bad.csv"] }]);
    const listed = q(".polar-file-list")!.textContent!;
    expect(listed).toContain("boat.pol");
    expect(listed).toContain("Adrena · TWA 0–180° (3) · TWS 6–20 kn (3)");
    const failure = q(".import-failures li")!;
    expect(failure.textContent).toBe("bad.csv, line 2, column 6: this is not a number");
    expect(failure.title).toContain("is not a number");
    // The row in the source list arrived with it.
    expect(q(".source-list")!.textContent).toContain("boat.pol");
    expect(q(".source-list")!.textContent).toContain("71 cells");
  });

  it("does nothing when the picker is cancelled", async () => {
    await open([]);
    await click(feature("polar-files:import"));
    expect(commands("import_polar_files")).toEqual([]);
  });

  it("removes a polar file from its section", async () => {
    await open([POLAR]);
    await click(feature("polar-files:remove"));
    expect(commands("remove_source")).toEqual([{ boatContext: 1,  id: 7 }]);
  });

  it("puts the blend first, then each source with its kind and count", async () => {
    await open([POLAR, TRACK]);
    const rows = [...document.querySelectorAll(".source-list > li")];
    expect(rows[0]!.textContent).toContain("Blend");
    expect(rows[1]!.textContent).toContain("71 cells");
    expect(rows[2]!.textContent).toContain("100/120 samples");
    expect(rows[2]!.classList.contains("hidden-source")).toBe(true);
    expect(rows[0]!.textContent).toContain("0 direct, 0 filled");
    expect((feature("sources:blend-settings") as HTMLButtonElement).disabled).toBe(false);
    expect((feature("sources:edit") as HTMLButtonElement).disabled).toBe(false);
    expect((feature("sources:compare") as HTMLButtonElement).disabled).toBe(false);
  });

  it("opens the Compare stage with a source as A and the blend as B (spec.md 8, 11)", async () => {
    await open([POLAR, TRACK]);
    const buttons = document.querySelectorAll<HTMLButtonElement>('[data-feature="sources:compare"]');
    await click(buttons[1]!);
    await settle();
    expect(feature("compare:operand-a")).not.toBeNull();
    expect(commands("compare_polars").at(-1)).toMatchObject({
      a: { kind: "segment", source_id: TRACK.id }, b: { kind: "blend" }, thresholdKn: 0.05,
    });
    expect(feature("compare:operand-a")!.textContent).toContain(TRACK.label);
    // Swap exchanges them; the choice is view state, sent with the next request.
    await click(feature("compare:swap"));
    await settle();
    expect(commands("compare_polars").at(-1)).toMatchObject({ a: { kind: "blend" }, b: { kind: "segment", source_id: TRACK.id } });
    // The summary and the heat map show what Rust sent.
    expect(q(".compare-summary")!.textContent).toContain("1 cells compared");
    expect(q("canvas.compare-heat-map")).not.toBeNull();
  });

  it("shows nothing of one project's comparison under another's (M15 review)", async () => {
    await open([POLAR, TRACK]);
    await click(feature("stage:compare"));
    await settle();
    expect(q(".compare-summary")!.textContent).toContain("1 cells compared");
    // The next project's comparison is still on its way when it opens.
    compareAnswer = "pending";
    const hooks = window as unknown as { __peOpen: (path: string) => Promise<string> };
    await act(async () => { await hooks.__peOpen("/other.wpsproj"); });
    await settle();
    expect(feature("stage:3d")?.getAttribute("aria-selected")).toBe("true");
    await click(feature("stage:compare"));
    expect(q(".compare-summary")!.textContent).toContain("0 cells compared");
    expect(q(".compare-legend")!.textContent).toContain("No compared cell");
  });

  it("clears the comparison and says why when Rust refuses it (M15 review)", async () => {
    await open([POLAR, TRACK]);
    await click(feature("stage:compare"));
    await settle();
    expect(q(".compare-summary")!.textContent).toContain("1 cells compared");
    compareAnswer = "refuse";
    await click(feature("compare:swap"));
    await settle();
    expect(q(".compare-failure")!.textContent).toContain("The comparison could not be made");
    expect(q(".compare-summary")!.textContent).toContain("0 cells compared");
  });

  it("lets the operand list be walked with the arrow keys (M15 review)", async () => {
    await open([POLAR, TRACK]);
    await click(feature("stage:compare"));
    await settle();
    const trigger = feature("compare:operand-b") as HTMLButtonElement;
    await act(async () => { trigger.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true })); });
    const options = [...document.querySelectorAll<HTMLButtonElement>('.compare-picker-b [role="option"]')];
    expect(options.map((o) => o.getAttribute("aria-selected"))).toEqual(["true", "false", "false"]);
    expect(document.activeElement).toBe(options[0]);
    const list = q(".compare-picker-b [role=listbox]")!;
    await act(async () => { list.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true })); });
    expect(document.activeElement).toBe(options[1]);
    await act(async () => { list.dispatchEvent(new KeyboardEvent("keydown", { key: "End", bubbles: true })); });
    expect(document.activeElement).toBe(options[2]);
    await click(options[2]!);
    await settle();
    expect(commands("compare_polars").at(-1)).toMatchObject({ b: { kind: "segment", source_id: TRACK.id } });
    expect(q(".compare-picker-b [role=listbox]")).toBeNull();
    expect(document.activeElement).toBe(feature("compare:operand-b"));
  });

  it("outlines a blend colour lost on the theme's background (M14 review)", async () => {
    // The light Paper theme, whose background a white blend is lost on.
    settings = { ...settings, theme: "paper" };
    {
      project = { ...summary(false, "/p.wpsproj", [POLAR]), blend: { ...TEST_BLEND, colour: "#ffffff" } };
      await mount();
      expect(feature("sources:blend-colour")!.classList.contains("outlined")).toBe(true);
      await act(async () => root.unmount());
      host.remove();
      project = { ...summary(false, "/p.wpsproj", [POLAR]), blend: TEST_BLEND };
      await mount();
      expect(feature("sources:blend-colour")!.classList.contains("outlined")).toBe(false);
    }
  });

  it("shows and hides the blend, and applies Blend settings as one change (spec.md 8, 12)", async () => {
    await open([POLAR]);
    const visible = feature("sources:blend-visible") as HTMLInputElement;
    await click(visible);
    expect(commands("set_blend_visible")).toEqual([{ boatContext: 1,  visible: false }]);
    expect(q(".blend-row")!.classList.contains("hidden-source")).toBe(true);

    await click(feature("sources:blend-settings"));
    const twa = feature("blend-settings:twa") as HTMLTextAreaElement;
    expect(twa.value).toBe("0, 30, 35, 40, 45, 52, 60, 70, 75, 80, 90, 100, 110, 120, 135, 150, 160, 170, 180");
    const apply = () => buttonNamed("Apply") as HTMLButtonElement;
    // Two values closer than 0.01 are refused before anything is sent (M4 carry).
    await type(twa, "0, 40, 40.004, 180");
    expect(q(".blend-settings .modal-error")!.textContent).toBe("40 and 40.004 are closer than 0.01: a polar file could not tell them apart.");
    expect(apply().disabled).toBe(true);
    await type(twa, "0, 45; 90 135 180");
    await type(feature("blend-settings:n-full") as HTMLInputElement, "12");
    const polarStatistic = feature("blend-settings:polar-statistic") as HTMLSelectElement;
    expect([...polarStatistic.options].map(option => option.value)).toEqual(["min", "median", "mean", "max", "p90"]);
    await act(async () => {
      polarStatistic.value = "max";
      polarStatistic.dispatchEvent(new Event("change", { bubbles: true }));
    });
    await click(feature("blend-settings:smoothing"));
    await click(apply());
    expect(commands("set_blend_settings")).toEqual([{ boatContext: 1,
      settings: {
        twa: [0, 45, 90, 135, 180], tws: [4, 6, 8, 10, 12, 14, 16, 20, 25, 30], min_samples: 5, n_full: 12,
        smoothing: true, default_statistic: "p90", polar_statistic: "max", use_corrected: true, asymmetric: false, interpolation: "linear",
      },
    }]);
    expect(feature("blend-settings:twa")).toBeNull();
  });

  it("previews the export on either grid and saves it through the native dialog (spec.md 12)", async () => {
    await open([POLAR]);
    await click(feature("sources:export"));
    expect(commands("export_preview")).toEqual([{ boatContext: 1,  format: "expedition", axes: null }]);
    expect(q(".export-preview td.filled")!.textContent).toBe("0.00");
    await click(feature("export:csv"));
    await click(feature("export:custom-grid"));
    await type(feature("export:custom-tws") as HTMLTextAreaElement, "8 12");
    expect(commands("export_preview").at(-1)).toMatchObject({ format: "csv", axes: { tws: [8, 12] } });
    expect(q(".export-preview thead")!.textContent).toContain("12");
    dialog.save.mockResolvedValue("/boats/blend.csv");
    await click(buttonNamed("Save…"));
    expect(dialog.save.mock.calls.at(-1)![0]).toMatchObject({ defaultPath: "Fastnet.csv" });
    expect(commands("export_polar")).toEqual([{ boatContext: 1,  path: "/boats/blend.csv", format: "csv", axes: expect.objectContaining({ tws: [8, 12] }) }]);
    expect(feature("export:preview")).toBeNull();
  });

  it("opens a source in the 3D stage to edit it, and a typed value is an edit in Rust (spec.md 10.4)", async () => {
    await open([POLAR, TRACK]);
    await click(feature("sources:edit"));
    await settle();
    expect(calls.some(([command, args]) => command === "polar_scene" && (args as { focus: number }).focus === 7)).toBe(true);
    expect(commands("polar_edit_surface")).toEqual([{ boatContext: 1,  sourceId: 7 }]);
    expect(feature("view3d:tool-drag")).not.toBeNull();
    const cell = q<HTMLInputElement>(".view3d-edit-table input")!;
    await act(async () => cell.focus());
    await type(cell, "6.25");
    await act(async () => cell.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true })));
    await settle();
    expect(commands("edit_polar")).toEqual([
      { boatContext: 1,  sourceId: 7, op: { type: "type", bsp: 6.25 }, cells: [{ twa_index: 0, tws_index: 0 }], gesture: null },
    ]);
    await click(feature("edit:done"));
    expect(q(".view3d-edit")).toBeNull();
  });

  it("edits a source through Rust: visibility, name, colour, order and removal", async () => {
    await open([POLAR, TRACK]);
    await click(feature("sources:visible"));
    expect(commands("set_source_visible")).toEqual([{ boatContext: 1,  id: 7, visible: false }]);

    await click(feature("sources:rename"));
    const input = q<HTMLInputElement>("input.source-rename")!;
    await type(input, "  Sister ship  ");
    await act(async () => input.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true })));
    await settle();
    expect(commands("set_source_label")).toEqual([{ boatContext: 1,  id: 7, label: "Sister ship" }]);

    await click(feature("sources:colour"));
    const swatches = [...document.querySelectorAll<HTMLButtonElement>(".palette-swatch")];
    expect(swatches).toHaveLength(16);
    expect(swatches[0]!.getAttribute("aria-pressed")).toBe("true");
    await click(swatches[3]!);
    expect(commands("set_source_colour")).toEqual([{ boatContext: 1,  id: 7, colour: "#76b7b2" }]);
    expect(q(".colour-popover")).toBeNull();

    const handle = feature("sources:reorder")!;
    await act(async () => handle.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true })));
    await act(async () => handle.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowUp", bubbles: true })));
    await settle();
    // Down moves the first source to 1; up from the first row goes nowhere.
    expect(commands("move_source")).toEqual([{ boatContext: 1,  id: 7, to: 1 }]);

    await click(feature("sources:remove"));
    expect(commands("remove_source")).toEqual([{ boatContext: 1,  id: 7 }]);
  });

  it("drags the weight slider as one gesture", async () => {
    await open([POLAR]);
    const slider = feature("sources:weight") as HTMLInputElement;
    await act(async () => {
      slider.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true }));
      for (const value of ["0.6", "0.4"]) {
        Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(slider, value);
        slider.dispatchEvent(new Event("input", { bubbles: true }));
      }
      slider.dispatchEvent(new PointerEvent("pointerup", { bubbles: true }));
    });
    await settle();
    const weights = commands("set_source_weight") as { id: number; weight: number; gesture: string | null }[];
    expect(weights.map((w) => w.weight)).toEqual([0.6, 0.4]);
    // Weights run from 0 to 1 (spec.md 8).
    expect(slider.min).toBe("0");
    expect(slider.max).toBe("1");
    expect(weights[0]!.gesture).not.toBeNull();
    expect(weights[1]!.gesture).toBe(weights[0]!.gesture);
  });

  it("says why a file failed in the interface language", async () => {
    settings = { ...settings, language: "de" };
    setLanguage("de");
    await open([]);
    dialog.open.mockResolvedValue(["/boats/bad.csv"]);
    await click(feature("polar-files:import"));
    expect(q(".import-failures li")!.textContent).toBe("bad.csv, Zeile 2, Spalte 6: dies ist keine Zahl");
  });
});

describe("ORC polars (plan.md M5)", () => {
  const open = async (sources: SourceSummary[]) => {
    project = summary(false, "/p.wpsproj", sources);
    await mount();
  };
  const commands = (name: string) => calls.filter(([command]) => command === name).map(([, args]) => args);
  const search = async (text: string) => {
    await type(feature("orc:search") as HTMLInputElement, text);
    await settle();
  };

  it("searches as you type, with filters, and lists results with a thumbnail", async () => {
    await open([]);
    expect(commands("orc_search")).toEqual([]);
    await search("G");
    await search("GBR 1124");
    const searches = commands("orc_search") as { query: string; filters: unknown; limit: number }[];
    expect(searches.map((s) => s.query)).toEqual(["G", "GBR 1124"]);
    expect(searches[1]!.filters).toEqual({
      year_min: null, year_max: null, country: null,
      name: "", sail_no: "", model: "", builder: "", designer: "", certificate_year: "", size_min: Array(8).fill(null), size_max: Array(8).fill(null),
    });
    const row = q(".orc-results li")!;
    // Badged with its catalogue (asked 2026-10-02).
    expect(row.querySelector(".orc-name .catalogue-badge")!.textContent).toBe("ORC");
    expect(row.querySelector(".orc-name")!.textContent).toBe("ORCEratosthenes");
    expect(row.querySelector(".orc-meta")!.textContent).toBe("GBR 1124 · Swan 112 · 1999 · Nautor");
    expect(row.querySelector("svg.orc-thumb path")!.getAttribute("d")).toMatch(/^M/);
    expect(q(".orc-count")!.textContent).toBe("Certificates 1–1 of 120");

    await click(feature("orc:fields"));
    const country = feature("orc:country") as HTMLSelectElement;
    await act(async () => {
      country.value = "NED";
      country.dispatchEvent(new Event("change", { bubbles: true }));
    });
    await type(feature("orc:year-from") as HTMLInputElement, "1990");
    await settle();
    const last = (commands("orc_search") as { filters: unknown }[]).at(-1)!;
    expect(last.filters).toMatchObject({ year_min: 1990, year_max: null, country: "NED" });
    expect(q(".orc-provenance")!.textContent).toBe("Catalogue: 18135 certificates from jieter/orc-data of 2026-09-28");
  });

  it("searches each field on its own, folded away until wanted and remembered", async () => {
    await open([]);
    // Collapsed by default: no per-field box is on screen.
    const toggle = feature("orc:fields") as HTMLButtonElement;
    expect(toggle.getAttribute("aria-expanded")).toBe("false");
    expect(toggle.textContent).toContain("Search by field");
    expect(feature("orc:field-name")).toBeNull();
    await click(toggle);
    expect(localStorage.getItem("pe.orcFields")).toBe("open");
    expect((feature("orc:fields-clear") as HTMLButtonElement).disabled).toBe(true);

    // Two fields, no text in the box: each keystroke searches with both.
    await type(feature("orc:field-designer") as HTMLInputElement, "frers");
    await settle();
    await type(feature("orc:field-sail") as HTMLInputElement, " GBR/1124 ");
    await settle();
    const searches = commands("orc_search") as { query: string; filters: Record<string, unknown> }[];
    expect(searches.map((s) => s.query)).toEqual(["", ""]);
    expect(searches.at(-1)!.filters).toEqual({
      year_min: null, year_max: null, country: null,
      name: "", sail_no: "GBR/1124", model: "", builder: "", designer: "frers", certificate_year: "", size_min: Array(8).fill(null), size_max: Array(8).fill(null),
    });
    expect(q(".orc-results li .orc-name")!.textContent).toBe("ORCEratosthenes");

    // Year built is one field however many ends are set; the count shows.
    await type(feature("orc:year-from") as HTMLInputElement, "1990");
    await type(feature("orc:year-to") as HTMLInputElement, "2000");
    await type(feature("orc:field-certificate-year") as HTMLInputElement, "202");
    await settle();
    expect(feature("orc:fields")!.textContent).toContain("Search by field (4)");
    expect((commands("orc_search") as { filters: unknown }[]).at(-1)!.filters).toMatchObject({
      year_min: 1990, year_max: 2000, certificate_year: "202",
    });

    // Folding keeps the fields, and the count stays in sight.
    await click(feature("orc:fields"));
    expect(feature("orc:field-designer")).toBeNull();
    expect(localStorage.getItem("pe.orcFields")).toBe("closed");
    expect(feature("orc:fields")!.textContent).toContain("(4)");

    // Clear empties every field and keeps the box.
    await click(feature("orc:fields"));
    await type(feature("orc:search") as HTMLInputElement, "swan");
    await settle();
    await click(feature("orc:fields-clear"));
    await settle();
    expect((feature("orc:field-designer") as HTMLInputElement).value).toBe("");
    expect((feature("orc:search") as HTMLInputElement).value).toBe("swan");
    expect(feature("orc:fields")!.textContent).not.toContain("(");
    const cleared = (commands("orc_search") as { query: string; filters: unknown }[]).at(-1)!;
    expect(cleared.query).toBe("swan");
    expect(cleared.filters).toMatchObject({ designer: "", sail_no: "", year_min: null, certificate_year: "" });

    // Remembered: a new window opens it unfolded.
    await act(async () => root.unmount());
    host.remove();
    await mount();
    expect(feature("orc:fields")!.getAttribute("aria-expanded")).toBe("true");
    expect(feature("orc:field-name")).not.toBeNull();
  });

  it("lists nothing when every field is empty or holds only spaces or punctuation", async () => {
    await open([]);
    await click(feature("orc:fields"));
    await type(feature("orc:field-model") as HTMLInputElement, "   ");
    await type(feature("orc:field-sail") as HTMLInputElement, " / ");
    await settle();
    expect(feature("orc:fields")!.textContent).not.toContain("(");
    // Every box keeps a visible label, filled or not, in reading order.
    expect([...document.querySelectorAll(".orc-field > span")].map((label) => label.textContent)).toEqual([
      "Boat name", "Sail number", "Model / type", "Builder", "Designer", "Country",
      "Built from", "Built until", "Certificate year",
    ]);
    expect((feature("orc:field-sail") as HTMLInputElement).closest("label")!.textContent).toContain("Sail number");
    expect(commands("orc_search")).toEqual([]);
    expect(q(".orc-results")).toBeNull();
  });

  it("adds a certificate, and asks before adding it a second time", async () => {
    await open([]);
    await search("eratosthenes");
    await click(feature("orc:add"));
    expect(commands("orc_add")).toEqual([{ boatContext: 1,  id: 4242, allowDuplicate: false }]);
    expect(q(".orc-added")!.textContent).toContain("Eratosthenes");
    expect(q(".orc-added")!.textContent).toContain("GBR 1124 · Swan 112 · 1999 · Certificate 2023");

    // Marked as added now; Add again asks, and Cancel adds nothing.
    expect(feature("orc:add")!.textContent).toBe("Added");
    await click(feature("orc:add"));
    expect(q("[role=dialog]")!.textContent).toContain("Eratosthenes is already in the project.");
    await click(buttonNamed("Cancel"));
    expect(commands("orc_add")).toHaveLength(1);
    await click(feature("orc:add"));
    await click(buttonNamed("Add again"));
    expect(commands("orc_add")).toEqual([{ boatContext: 1,  id: 4242, allowDuplicate: false }, { boatContext: 1,  id: 4242, allowDuplicate: true }]);
  });

  it("asks when Rust finds a duplicate the list did not know about", async () => {
    await open([]);
    await search("eratosthenes");
    expect(feature("orc:add")!.textContent).toBe("Add");
    // The project gained the certificate behind the list's back.
    project = summary(false, "/p.wpsproj", [ORC]);
    await click(feature("orc:add"));
    expect(q("[role=dialog]")!.textContent).toContain("Eratosthenes is already in the project.");
    await click(buttonNamed("Add again"));
    expect(commands("orc_add")).toEqual([{ boatContext: 1,  id: 4242, allowDuplicate: false }, { boatContext: 1,  id: 4242, allowDuplicate: true }]);
    expect(q(".statusbar .hint.error"), "the duplicate is asked about, not reported").toBeNull();
  });

  it("removes an ORC polar from its section", async () => {
    await open([ORC]);
    await click(feature("orc:remove"));
    expect(commands("remove_source")).toEqual([{ boatContext: 1,  id: 9 }]);
  });
});
