// @vitest-environment happy-dom
/**
 * The track library section edits a draft, so closing Settings saves it
 * (asked 2026-10-06: a changed GeoJSON folder was lost on closing). An
 * unchanged draft saves nothing, and a refused one keeps its reason.
 */
import { act, createRef } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { AppSettings } from "../generated/AppSettings";
import type { LibrarySettings as Preferences } from "../generated/LibrarySettings";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const held = vi.hoisted(() => ({
  setLibrarySettings: vi.fn(async (library: unknown) => ({ library }) as unknown as AppSettings),
}));

vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => undefined) }));
vi.mock("../ipc", () => ({
  api: {
    setLibrarySettings: held.setLibrarySettings,
    libraryScrapeStatus: vi.fn(async () => ({ running: false, manual: false, done: 0, total: 0, tracks: 0, skipped: 0, held: 0, failed: 0, current: "", cancelled: false, error: null, failures: [] })),
    metadataDownloadStatus: vi.fn(async () => ({ running: false, done: 0, total: 0, current: "", tracks: 0, kept: 0, cancelled: false, error: null })),
  },
  LIBRARY_SCRAPE: "library://scrape",
  LIBRARY_METADATA: "library://metadata",
  IpcError: class extends Error {},
}));

import LibrarySettings from "./LibrarySettings";

const library: Preferences = {
  geojson_directory: "/old", metadata_directory: "",

  database: { host: "localhost", port: 5432, name: "syrfbackendprod", user: "postgres", password: "", tls: false },
};
let root: Root;
let host: HTMLDivElement;
const settle = () => act(async () => { await Promise.resolve(); await Promise.resolve(); });

/** Types into a controlled input the way a person does. */
function type(feature: string, value: string) {
  const input = host.querySelector<HTMLInputElement>(`[data-feature="${feature}"]`)!;
  const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!;
  act(() => { setter.call(input, value); input.dispatchEvent(new Event("input", { bubbles: true })); });
}

async function mount() {
  const flush = createRef<(() => Promise<void>) | null>() as { current: (() => Promise<void>) | null };
  flush.current = null;
  const onSettings = vi.fn();
  await act(async () => { root.render(<LibrarySettings settings={{ library } as AppSettings} onSettings={onSettings} flush={flush} />); });
  await settle();
  return { flush, onSettings };
}

beforeEach(() => {
  held.setLibrarySettings.mockClear();
  host = document.createElement("div");
  document.body.appendChild(host);
  root = createRoot(host);
});
afterEach(() => { act(() => root.unmount()); host.remove(); });

describe("closing Settings", () => {
  it("saves a changed GeoJSON folder", async () => {
    const { flush, onSettings } = await mount();
    type("settings:library-geojson", "/new/tracks");
    await act(async () => { await flush.current!(); });
    expect(held.setLibrarySettings).toHaveBeenCalledWith({ ...library, geojson_directory: "/new/tracks" });
    expect(onSettings).toHaveBeenCalled();
  });

  it("saves nothing when nothing changed", async () => {
    const { flush } = await mount();
    await act(async () => { await flush.current!(); });
    expect(held.setLibrarySettings).not.toHaveBeenCalled();
  });

  it("passes on a refusal, so the dialog stays open", async () => {
    const { flush } = await mount();
    held.setLibrarySettings.mockRejectedValueOnce(new Error("Choose an absolute directory path"));
    type("settings:library-geojson", "relative");
    await expect(flush.current!()).rejects.toThrow("absolute");
  });
});
