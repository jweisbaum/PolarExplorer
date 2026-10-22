import { beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

import { api, IpcError, isErrorPayload } from "./ipc";

describe("isErrorPayload", () => {
  it("accepts the shape Rust actually sends", () => {
    expect(isErrorPayload({ kind: "io", message: "disk full" })).toBe(true);
  });

  it("rejects a bare string, which is how a panic arrives", () => {
    expect(isErrorPayload("something exploded")).toBe(false);
    expect(isErrorPayload(null)).toBe(false);
    expect(isErrorPayload({ kind: "io" })).toBe(false);
  });
});

describe("api", () => {
  beforeEach(() => {
    invoke.mockReset();
  });

  it("calls the Rust command by its snake_case name", async () => {
    invoke.mockResolvedValue({ name: "PolarExplorer", version: "0.1.0" });
    await expect(api.appInfo()).resolves.toEqual({ name: "PolarExplorer", version: "0.1.0" });
    expect(invoke).toHaveBeenCalledWith("app_info", undefined);
  });

  it("turns an AppError payload into an IpcError with its kind", async () => {
    invoke.mockRejectedValue({ kind: "doing", message: "Could not read x: gone" });
    const err = await api.appInfo().catch((e: unknown) => e);
    expect(err).toBeInstanceOf(IpcError);
    expect((err as IpcError).kind).toBe("doing");
    expect((err as IpcError).message).toBe("Could not read x: gone");
  });

  it("turns a bare string into an IpcError of unknown kind", async () => {
    invoke.mockRejectedValue("panicked");
    const err = await api.appInfo().catch((e: unknown) => e);
    expect(err).toBeInstanceOf(IpcError);
    expect((err as IpcError).kind).toBe("unknown");
    expect((err as IpcError).message).toBe("panicked");
  });

  it("passes the unsaved-changes answer to every call that drops a project", async () => {
    invoke.mockResolvedValue(null);
    await api.newProject("Fastnet", { name: "Boat", notes: "" }, true);
    expect(invoke).toHaveBeenLastCalledWith("new_project", {
      name: "Fastnet",
      boat: { name: "Boat", notes: "" },
      discardUnsaved: true,
    });
    await api.openProject("/a.wpsproj");
    expect(invoke).toHaveBeenLastCalledWith("open_project", {
      path: "/a.wpsproj",
      discardUnsaved: false,
    });
    await api.closeProject(true);
    expect(invoke).toHaveBeenLastCalledWith("close_project", { discardUnsaved: true });
    await api.openRecovered(7);
    expect(invoke).toHaveBeenLastCalledWith("open_recovered", { id: 7, discardUnsaved: false });
  });

  it("names the history and recent-list commands as Rust does", async () => {
    invoke.mockResolvedValue([]);
    await api.undo();
    expect(invoke).toHaveBeenLastCalledWith("undo", undefined);
    await api.renameProject("New");
    expect(invoke).toHaveBeenLastCalledWith("rename_project", { name: "New" });
    await api.clearRecent();
    expect(invoke).toHaveBeenLastCalledWith("clear_recent", undefined);
    await api.saveProjectAs("/b");
    expect(invoke).toHaveBeenLastCalledWith("save_project_as", { path: "/b" });
  });

  it("names the source, settings and quit commands as Rust does", async () => {
    invoke.mockResolvedValue(null);
    await api.setSourceColour(3, "#aabbcc");
    expect(invoke).toHaveBeenLastCalledWith("set_source_colour", { id: 3, colour: "#aabbcc" });
    await api.setSourceVisible(3, false);
    expect(invoke).toHaveBeenLastCalledWith("set_source_visible", { id: 3, visible: false });
    await api.setSourceWeight(3, 1.5, "drag");
    expect(invoke).toHaveBeenLastCalledWith("set_source_weight", { id: 3, weight: 1.5, gesture: "drag" });
    await api.setSourceWeight(3, 1);
    expect(invoke).toHaveBeenLastCalledWith("set_source_weight", { id: 3, weight: 1, gesture: null });
    await api.setSourceLabel(3, "B");
    expect(invoke).toHaveBeenLastCalledWith("set_source_label", { id: 3, label: "B" });
    await api.moveSource(3, 0);
    expect(invoke).toHaveBeenLastCalledWith("move_source", { id: 3, to: 0 });
    await api.removeSource(3);
    expect(invoke).toHaveBeenLastCalledWith("remove_source", { id: 3 });
    await api.setLanguage("fr");
    expect(invoke).toHaveBeenLastCalledWith("set_language", { language: "fr" });
    await api.legacyCacheNotice();
    expect(invoke).toHaveBeenLastCalledWith("legacy_cache_notice", undefined);
    await api.removeOldChunkCache();
    expect(invoke).toHaveBeenLastCalledWith("remove_old_chunk_cache", undefined);
    await api.setProjection("orthographic");
    expect(invoke).toHaveBeenLastCalledWith("set_projection", { projection: "orthographic" });
    await api.quitApp(true);
    expect(invoke).toHaveBeenLastCalledWith("quit_app", { discardUnsaved: true });
  });

  it("names the polar plot commands and passes null for \"all\"", async () => {
    invoke.mockResolvedValue(null);
    await api.polarPlot(12);
    expect(invoke).toHaveBeenLastCalledWith("polar_plot", { tws: 12 });
    await api.polarPlot(null);
    expect(invoke).toHaveBeenLastCalledWith("polar_plot", { tws: null });
    const empty = new Uint8Array(16);
    new DataView(empty.buffer).setUint32(0, 0x44324550, true);
    new DataView(empty.buffer).setUint32(4, 2, true);
    invoke.mockResolvedValue(empty.buffer);
    expect((await api.polarPlotDots(null, true)).count).toBe(0);
    expect(invoke).toHaveBeenLastCalledWith("polar_plot_dots", { tws: null, showFiltered: true });
  });

  it("names the edit commands' arguments", async () => {
    invoke.mockResolvedValue(null);
    await api.polarEditSurface(3);
    expect(invoke).toHaveBeenLastCalledWith("polar_edit_surface", { sourceId: 3 });
    const cells = [{ twa_index: 1, tws_index: 2 }];
    await api.editPolar(3, { type: "drag", bsp: 6.5 }, cells, "g1");
    expect(invoke).toHaveBeenLastCalledWith("edit_polar", { sourceId: 3, op: { type: "drag", bsp: 6.5 }, cells, gesture: "g1" });
    await api.editPolar(3, { type: "reset_all" }, []);
    expect(invoke).toHaveBeenLastCalledWith("edit_polar", { sourceId: 3, op: { type: "reset_all" }, cells: [], gesture: null });
    await api.setSegmentStatistic(4, "median");
    expect(invoke).toHaveBeenLastCalledWith("set_segment_statistic", { sourceId: 4, statistic: "median" });
  });

  it("unpacks the 3D scene from raw bytes and names the exclusion command's arguments", async () => {
    const header = new Uint8Array(48);
    new DataView(header.buffer).setUint32(0, 0x44334550, true);
    new DataView(header.buffer).setUint32(4, 5, true);
    new DataView(header.buffer).setUint32(32, 77, true);
    invoke.mockResolvedValue(header.buffer);
    const held = await api.polarScene();
    expect(held.nodes.count).toBe(0);
    expect(invoke).toHaveBeenLastCalledWith("polar_scene", { focus: null, samplesKey: null });
    await api.polarScene(5, held);
    expect(invoke).toHaveBeenLastCalledWith("polar_scene", { focus: 5, samplesKey: 77 });
    invoke.mockResolvedValue([...header]);
    expect((await api.polarScene()).sources).toEqual([]);
    invoke.mockResolvedValue([1, 2, 3]);
    await expect(api.polarScene()).rejects.toThrow(/no header/);
    invoke.mockResolvedValue(null);
    const node = { source_id: 1, twa_index: 2, tws_index: 0 };
    await api.setExcluded([node], [], true);
    expect(invoke).toHaveBeenLastCalledWith("set_excluded", { nodes: [node], samples: [], excluded: true });
  });

  it("hands the basemap over as an ArrayBuffer either way it arrives", async () => {
    invoke.mockResolvedValue([1, 2, 3]);
    const bytes = await api.basemap();
    expect(bytes).toBeInstanceOf(ArrayBuffer);
    expect(new Uint8Array(bytes)).toEqual(new Uint8Array([1, 2, 3]));
  });
});
