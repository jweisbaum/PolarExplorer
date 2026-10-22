/** Retired weather and track-scraping preferences cannot reappear after an upgrade. */
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { assert, newProject } from "./harness.mjs";
let root;
export default {
  name: "simplified weather and track library settings",
  async setup() {
    root = await mkdtemp(join(tmpdir(), "pe-ux-simple-settings-"));
    await mkdir(join(root, "config"));
    await writeFile(join(root, "old-cache"), "preserve existing files");
    await writeFile(join(root, "config", "settings.json"), JSON.stringify({
      weather_memory_mb: 16, weather_cache: { directory: join(root, "old-cache"), max_size_gb: 1 },
      network: { timeout_s: 5 }, library: { scrape_schedule: "startup", scrape_urls: "unsupported://never-fetch" },
    }));
    return { env: { PE_AUTOMATION_ROOT: root }, teardown: () => rm(root, { recursive: true, force: true }) };
  },
  async run(t) {
    const d = t.driver;
    await newProject(d, "Settings");
    const settings = await d.invoke("app_settings", {});
    for (const key of ["weather_memory_mb", "weather_cache", "network"]) assert.ok(!(key in settings));
    assert.ok(!("scrape_schedule" in settings.library));
    assert.equal((await d.invoke("library_scrape_status", {})).running, false);
    await d.click('[data-feature="shell:settings"]');
    await d.waitFor(".modal.settings");
    for (const id of ["weather-memory", "cache-directory", "cache-size", "cache-clear", "timeout",
      "library-schedule", "library-urls", "library-yb-user-key", "library-yb-device-id", "library-scrape", "library-cancel"]) {
      assert.ok(!(await d.exists(`[data-feature="settings:${id}"]`)), `${id} is removed`);
    }
    await t.shot("weather-settings");
    await d.run(`document.querySelector('[data-section="settings:library"]').scrollIntoView({block:'start'}); done(true);`);
    assert.ok(await d.exists('[data-feature="settings:library-db-download"]'));
    await t.shot("track-library-settings");
    assert.equal(await readFile(join(root, "old-cache"), "utf8"), "preserve existing files");
    await d.click('[data-feature="settings:close"]');
    await d.waitGone(".modal.settings");
  },
};
