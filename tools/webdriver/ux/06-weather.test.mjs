/**
 * Small weather downloads, kept per sample only (M14e, D27), through the
 * real dev build and with no network:
 *
 * - an earlier version's on-disk chunk cache (seeded in the run's data
 *   root) is announced on the status line and removed, and nothing else;
 * - a five-day track starts weather immediately with no estimate dialog;
 *   a deliberately invalid cache path fails before HTTP and leave retry available;
 * - Settings has "Keep downloaded weather in memory for this session (MB)"
 *   in place of the chunk cache's folder and size, saved to settings.json
 *   without the old `chunk_cache` group.
 */
import { mkdir, mkdtemp, readFile, rm, stat, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { assert, newProject, blockWeatherDownloads } from "./harness.mjs";

/** 2025-07-26T12:00Z, the Fastnet start. */
const START = 1_753_531_200;
const OLD_CHUNK = "cache/chunks/wb2-era5-1h/10m_u_component_of_wind/539724.0.0";

/** A boat sailing from the Solent to the Fastnet and back, a fix every 10 minutes for five days. */
function race() {
  const features = [];
  for (let k = 0; k <= 720; k += 1) {
    const f = k / 720;
    const out = f < 0.5 ? f * 2 : 2 - f * 2;
    const lon = -1.3 - 8.2 * out;
    const lat = 50.7 + 0.8 * out + 0.2 * Math.sin(k / 30);
    features.push({
      type: "Feature",
      geometry: { type: "Point", coordinates: [lon, lat] },
      properties: { time: START + 600 * k, boat: "Ux Boat" },
    });
  }
  return JSON.stringify({ type: "FeatureCollection", features });
}

let root = null;

export default {
  name: "weather download and memory",
  async setup() {
    root = await mkdtemp(join(tmpdir(), "pe-ux-weather-"));
    // As an earlier version left the data root: a chunk in its cache and
    // its chunk-cache settings.
    await mkdir(join(root, OLD_CHUNK, ".."), { recursive: true });
    await writeFile(join(root, OLD_CHUNK), Buffer.alloc(3_300_000));
    await mkdir(join(root, "config"), { recursive: true });
    // A remembered reanalysis archive, so the fetch gets as far as the
    // blocked download; its address answers nothing.
    const archive = {
      id: "ux", name: "UX archive", storage: "cloud", uri: "s3://whirlwind-hindsight/hindsight-routing",
      source_uri: null, mode: "global", start: "2000-01-01T00:00:00Z", stop: null, layout_version: 3,
      chunk_degrees: 5, chunk_hours: 72, products: ["wind10m"], place: "127.0.0.1",
    };
    await writeFile(join(root, "config", "settings.json"),
      JSON.stringify({ chunk_cache: { location: "", size_limit_gb: 20 }, reanalysis: { port: 9, connected_once: true, archive } }));
    await writeFile(join(root, "config", "reanalysis-archive.json"), JSON.stringify({
      id: "ux",
      location: { protocol: "s3", bucket: "whirlwind-hindsight", prefix: "hindsight-routing",
        endpoint_url: "http://127.0.0.1:9", region: "us-east-1", force_path_style: true, credentials: null },
    }));
    await writeFile(join(root, "race.geojson"), race());
    return {
      env: { PE_AUTOMATION_ROOT: root },
      teardown: () => rm(root, { recursive: true, force: true }),
    };
  },
  async run(t) {
    const d = t.driver;
    await blockWeatherDownloads(d);
    await newProject(d, "Weather");
    // The old cache is announced once and removed; the data root stays.
    await d.waitFor(".statusbar", { text: "no longer kept on disk" });
    const notice = await d.text(".statusbar");
    assert.ok(notice.includes("3 MB"), notice);
    await t.shot("old-cache-removed");
    let gone = false;
    for (let i = 0; i < 50 && !gone; i += 1) {
      gone = await stat(join(root, "cache", "chunks")).then(() => false, () => true);
      if (!gone) await new Promise((r) => setTimeout(r, 100));
    }
    assert.ok(gone, "no file is left under the old chunk cache");
    assert.ok((await stat(join(root, "cache"))).isDirectory(), "only chunks/ went");

    // A five-day track starts directly; cache validation fails before HTTP.
    await d.queueDialog([join(root, "race.geojson")]);
    await d.click('[data-feature="tracks:import-file"]');
    await d.waitFor(".modal-actions button.primary");
    await d.click(".modal-actions button.primary");
    await d.waitFor('[data-feature="tracks:fetch-weather"]', { timeoutMs: 30_000 });
    await d.click('[data-feature="tracks:fetch-weather"]');
    assert.equal(await d.exists('[role="dialog"]'), false);
    await d.waitFor('.statusbar', { text: "Whirlwind cache path is not a regular directory" });
    assert.ok((await d.text('.track-list')).includes("Weather: not fetched"));
    await d.waitGone('.busy-spinner.on');
    await d.waitFor('[data-feature="tracks:fetch-weather"]:not(:disabled)');
    const jobs = await d.invoke('env_jobs', {});
    assert.equal(jobs.tracks.length, 0);
    assert.ok(jobs.failure?.[1]?.includes("Whirlwind cache path is not a regular directory"), JSON.stringify(jobs));
    await t.shot("weather-starts-without-estimate");

    // Settings: the memory for this session, where the chunk cache was.
    await d.click('[data-feature="shell:settings"]');
    await d.waitFor(".modal.settings");
    assert.ok(!(await d.exists('[data-feature="settings:cache-limit"]')), "no chunk cache setting");
    await d.run(`document.querySelector('[data-section="settings:weather"]').scrollIntoView({ block: "start" }); done(true);`);
    await d.waitFor('[data-feature="settings:weather-memory"]', { visible: true });
    assert.equal(await d.run(`done(document.querySelector('[data-feature="settings:weather-memory"]').value);`), "256");
    await d.type('[data-feature="settings:weather-memory"]', "512");
    await d.key("Enter", '[data-feature="settings:weather-memory"]');
    let saved = null;
    for (let i = 0; i < 50 && saved?.weather_memory_mb !== 512; i += 1) {
      saved = JSON.parse(await readFile(join(root, "config", "settings.json"), "utf8"));
      if (saved.weather_memory_mb !== 512) await new Promise((r) => setTimeout(r, 100));
    }
    assert.equal(saved.weather_memory_mb, 512);
    assert.ok(!("chunk_cache" in saved), "the old group is not written back");
    await t.shot("settings-weather-memory");
  },
};
