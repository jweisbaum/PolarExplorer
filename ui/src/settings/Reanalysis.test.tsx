// @vitest-environment happy-dom
/**
 * The Reanalysis section of Settings (design 2026-10-19, "Settings →
 * Reanalysis"): its three states, the archive list's rows and groups, and
 * what each control asks of Rust. The backend is mocked at `invoke`, as in
 * App.test.tsx, so every assertion on a call is on the command and the
 * arguments that cross IPC.
 */
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { AppSettings } from "../generated/AppSettings";
import type { RememberedArchive } from "../generated/RememberedArchive";
import type { ScanResult } from "../generated/ScanResult";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => undefined) }));

const { default: Reanalysis } = await import("./Reanalysis");

const CLOUD: RememberedArchive = {
  id: "c1", name: "ERA5 global", storage: "cloud", uri: "s3://bucket/era5", source_uri: null, mode: "global",
  start: "1990-01-01T00:00:00Z", stop: "2025-12-31T23:00:00Z", layout_version: 3, chunk_degrees: 10, chunk_hours: 24,
  products: ["wind10m", "waves"], place: "bucket.s3.eu-west-1.amazonaws.com",
};
const LOCAL: RememberedArchive = {
  id: "l1", name: "Atlantic copy", storage: "local", uri: "file:///data/atl", source_uri: "s3://bucket/era5", mode: "region",
  start: "2020-01-01T00:00:00Z", stop: null, layout_version: 3, chunk_degrees: 5, chunk_hours: 12,
  products: ["wind10m"], place: "/data/atl",
};
const GONE: RememberedArchive = { ...CLOUD, id: "old", name: "Retired archive", place: "old.example.com" };

const LISTING: ScanResult = {
  detected: true, port: 8765,
  archives: [CLOUD, LOCAL],
  unavailable: [
    { id: "u1", name: "Locked bucket", storage: "cloud", reason: "No credentials for this bucket" },
    { id: "u2", name: null, storage: "local", reason: "Folder missing" },
  ],
};
const ABSENT: ScanResult = { detected: false, port: 8765, archives: [], unavailable: [] };

let settings: AppSettings;
let probe: ScanResult;
let scan: () => Promise<ScanResult>;
let root: Root;
let host: HTMLDivElement;

const reanalysis = (port: number, connected_once: boolean, archive: RememberedArchive | null) =>
  ({ ...settings, reanalysis: { port, connected_once, archive } });

function backend() {
  invoke.mockImplementation(async (command: string, args?: Record<string, unknown>) => {
    switch (command) {
      case "reanalysis_probe": return probe;
      case "reanalysis_scan": return scan();
      case "set_reanalysis_port":
        settings = reanalysis(args?.port as number, settings.reanalysis.connected_once, settings.reanalysis.archive);
        return settings;
      case "set_reanalysis_archive": {
        const chosen = probe.archives.find(a => a.id === args?.id) ?? null;
        settings = reanalysis(settings.reanalysis.port, true, chosen);
        return settings;
      }
      case "open_hindsight_sync_site": return null;
      default: throw new Error(`unexpected command ${command}`);
    }
  });
}

const settle = () => act(async () => { for (let i = 0; i < 5; i += 1) await Promise.resolve(); });
const q = <T extends Element = HTMLElement>(selector: string) => host.querySelector<T>(selector);
const feature = <T extends Element = HTMLElement>(id: string) => q<T>(`[data-feature="${id}"]`);
const commands = () => invoke.mock.calls.map(([command]) => command as string);

/** Renders the section the way the dialog does, following the settings it is handed back. */
async function mount() {
  const onSettings = vi.fn((next: AppSettings) => { settings = next; render(); });
  const onError = vi.fn();
  const render = () => root.render(<Reanalysis settings={settings} onSettings={onSettings} onError={onError} pushed={null} />);
  await act(async () => { render(); });
  await settle();
  return { onSettings, onError };
}

beforeEach(() => {
  settings = { reanalysis: { port: 8765, connected_once: false, archive: null } } as AppSettings;
  probe = ABSENT;
  scan = async () => probe;
  invoke.mockReset();
  backend();
  host = document.createElement("div");
  document.body.appendChild(host);
  root = createRoot(host);
});
afterEach(() => { act(() => root.unmount()); host.remove(); });

describe("the Reanalysis section", () => {
  it("never connected and not detected shows the warning with the logo, the sentence and the link", async () => {
    await mount();
    expect(commands()).toEqual(["reanalysis_probe"]);
    const warning = q(".reanalysis-warning")!;
    expect(warning).not.toBeNull();
    const logo = warning.querySelector("img")!;
    expect(logo.getAttribute("alt")).toBe("Hindsight Sync logo");
    expect(logo.getAttribute("src")).toMatch(/hindsight-sync\.png/);
    expect(warning.textContent).toContain("PolarExplorer needs Hindsight Sync to download the reanalysis.");
    expect(feature("settings:reanalysis-link")!.textContent).toBe("hindsight-sync.whirlwindai.com");
    expect(feature("settings:reanalysis-scan")!.textContent).toBe("Scan");
    expect(feature<HTMLInputElement>("settings:reanalysis-port")!.value).toBe("8765");
    expect(feature("settings:reanalysis-archive")).toBeNull();
  });

  it("connected before and not detected shows the quiet line and keeps the remembered archive", async () => {
    settings = reanalysis(8765, true, CLOUD);
    const { onSettings } = await mount();
    expect(q(".reanalysis-warning")).toBeNull();
    expect(feature("settings:reanalysis-link")).toBeNull();
    expect(host.textContent).toContain("Hindsight Sync is not running. The remembered archive is still used for fetches.");
    expect(feature("settings:reanalysis-scan")).not.toBeNull();
    expect(feature("settings:reanalysis-port")).not.toBeNull();
    // Nothing asks Rust to forget the archive: fetches still read it.
    expect(commands()).toEqual(["reanalysis_probe"]);
    expect(onSettings).not.toHaveBeenCalled();

    act(() => root.unmount());
    root = createRoot(host);
    settings = reanalysis(8765, true, null);
    await mount();
    expect(host.textContent).toContain("Hindsight Sync is not running. No archive is chosen.");
  });

  it("detected lists cloud then local, each row name — place — chunking — products, unavailable rows disabled with their reason", async () => {
    probe = LISTING;
    await mount();
    expect(q(".reanalysis-warning")).toBeNull();
    const select = feature<HTMLSelectElement>("settings:reanalysis-archive")!;
    expect(select.value).toBe("");
    expect(select.options[0]!.textContent).toBe("Choose an archive…");
    const groups = [...select.querySelectorAll("optgroup")];
    expect(groups.map(g => g.getAttribute("label"))).toEqual(["Cloud", "This computer"]);
    const rows = (group: Element) => [...group.querySelectorAll("option")].map(o =>
      [o.textContent, o.disabled, o.getAttribute("title")]);
    expect(rows(groups[0]!)).toEqual([
      ["ERA5 global — bucket.s3.eu-west-1.amazonaws.com — 10° × 24 h — wind10m, waves", false, null],
      ["Locked bucket", true, "No credentials for this bucket"],
    ]);
    expect(rows(groups[1]!)).toEqual([
      ["Atlantic copy — /data/atl — 5° × 12 h — wind10m", false, null],
      ["u2", true, "Folder missing"],
    ]);
    expect(host.textContent).toContain("Connected to Hindsight Sync on port 8765");
  });

  it("choosing an archive calls set_reanalysis_archive with its id and the muted line shows its range", async () => {
    probe = LISTING;
    await mount();
    const select = feature<HTMLSelectElement>("settings:reanalysis-archive")!;
    await act(async () => {
      select.value = "c1";
      select.dispatchEvent(new Event("change", { bubbles: true }));
    });
    await settle();
    expect(invoke).toHaveBeenCalledWith("set_reanalysis_archive", { id: "c1" });
    expect(feature<HTMLSelectElement>("settings:reanalysis-archive")!.value).toBe("c1");
    expect(host.textContent).toContain("1990-01-01 to 2025-12-31");
    // The prompt goes once something is chosen.
    expect(host.textContent).not.toContain("Choose an archive…");

    // A growing archive has no end.
    await act(async () => {
      select.value = "l1";
      select.dispatchEvent(new Event("change", { bubbles: true }));
    });
    await settle();
    expect(invoke).toHaveBeenCalledWith("set_reanalysis_archive", { id: "l1" });
    expect(host.textContent).toContain("from 2020-01-01, ongoing");
  });

  it("a remembered archive missing from the list is appended as (no longer listed) and stays selected", async () => {
    probe = LISTING;
    settings = reanalysis(8765, true, GONE);
    await mount();
    const select = feature<HTMLSelectElement>("settings:reanalysis-archive")!;
    expect(select.value).toBe("old");
    const cloud = select.querySelector("optgroup")!;
    const last = cloud.querySelector("option:last-child")!;
    expect(last.textContent).toBe("Retired archive — old.example.com — 10° × 24 h — wind10m, waves (no longer listed)");
    expect((last as HTMLOptionElement).selected).toBe(true);
    expect(commands()).not.toContain("set_reanalysis_archive");
  });

  it("an empty listing shows the no-archives line", async () => {
    probe = { detected: true, port: 8765, archives: [], unavailable: [] };
    settings = reanalysis(8765, true, null);
    await mount();
    expect(host.textContent).toContain("Hindsight Sync lists no reanalysis archives.");
    expect(feature("settings:reanalysis-archive")).toBeNull();
    expect(feature("settings:reanalysis-scan")).not.toBeNull();
    expect(feature("settings:reanalysis-port")).not.toBeNull();
  });

  it("scan disables the button while it runs and the port field saves on change", async () => {
    let finish: (result: ScanResult) => void = () => undefined;
    scan = () => new Promise(resolve => { finish = resolve; });
    await mount();
    const button = feature<HTMLButtonElement>("settings:reanalysis-scan")!;
    await act(async () => { button.click(); });
    expect(invoke).toHaveBeenCalledWith("reanalysis_scan", undefined);
    expect(feature<HTMLButtonElement>("settings:reanalysis-scan")!.disabled).toBe(true);
    expect(feature("settings:reanalysis-scan")!.textContent).toBe("Scanning…");
    await act(async () => { finish(LISTING); });
    await settle();
    expect(feature<HTMLButtonElement>("settings:reanalysis-scan")!.disabled).toBe(false);
    expect(feature("settings:reanalysis-scan")!.textContent).toBe("Scan");
    // The scan's answer replaces what the section shows.
    expect(feature("settings:reanalysis-archive")).not.toBeNull();

    invoke.mockClear();
    const port = feature<HTMLInputElement>("settings:reanalysis-port")!;
    const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!;
    await act(async () => {
      setter.call(port, "9000");
      port.dispatchEvent(new Event("input", { bubbles: true }));
    });
    await act(async () => { port.dispatchEvent(new Event("blur", { bubbles: false })); port.dispatchEvent(new FocusEvent("focusout", { bubbles: true })); });
    await settle();
    expect(commands()).toEqual(["set_reanalysis_port", "reanalysis_probe"]);
    expect(invoke).toHaveBeenCalledWith("set_reanalysis_port", { port: 9000 });
    expect(feature<HTMLInputElement>("settings:reanalysis-port")!.value).toBe("9000");
  });

  it("the link opens the site through ipc, never through window.open", async () => {
    const open = vi.spyOn(window, "open").mockImplementation(() => null);
    await mount();
    const link = feature("settings:reanalysis-link")!;
    expect(link.getAttribute("target")).toBeNull();
    await act(async () => { link.click(); });
    await settle();
    expect(invoke).toHaveBeenCalledWith("open_hindsight_sync_site", undefined);
    expect(open).not.toHaveBeenCalled();
    open.mockRestore();
  });
});
