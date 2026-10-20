import { readFile } from "node:fs/promises";
import { join } from "node:path";
import { assert, newProject, blockWeatherDownloads } from "./harness.mjs";

export default {
  name: "weather data sources",
  async run(t) {
    const d = t.driver;
    await blockWeatherDownloads(d);
    await newProject(d, "Data sources");
    await d.click('[data-feature="shell:settings"]');
    await d.waitFor('.modal.settings');
    const selector = '[data-feature="settings:data-source"]';
    assert.equal(await d.run(`done(document.querySelector('${selector}').value)`), "whirlwind");
    assert.deepEqual(await d.run(`done(Array.from(document.querySelector('${selector}').options, o => o.text))`),
      ["Whirlwind (Fast) S3", "Whirlwind (Fast) R2", "Whirlwind (Fast) Tigris"]);
    assert.ok((await d.text(".modal.settings")).includes("Whirlwind downloads up to 64 chunks at once."));
    assert.equal(await d.exists('[data-feature="settings:concurrency"]'), false);
    for (const source of ["whirlwind_r2", "whirlwind_tigris"]) {
      await d.type(selector, source);
      assert.ok((await d.text(".modal.settings")).includes("Whirlwind downloads up to 64 chunks at once."));
      assert.equal(await d.exists('[data-feature="settings:concurrency"]'), false);
      await d.click('[data-feature="settings:close"]');
      await d.waitGone('.modal.settings');
      const saved = JSON.parse(await readFile(join(d.automationRoot, "config", "settings.json"), "utf8"));
      assert.equal(saved.data_source, source);
      await d.click('[data-feature="shell:settings"]');
      await d.waitFor(selector);
      assert.equal(await d.run(`done(document.querySelector('${selector}').value)`), source);
      await d.run(`document.querySelector('${selector}').scrollIntoView({block:'center'}); done(true)`);
      await t.shot(`${source}-selected`);
    }
    await d.type(selector, "whirlwind");
    await d.click('[data-feature="settings:close"]');
    await d.waitGone('.modal.settings');
    const saved = JSON.parse(await readFile(join(d.automationRoot, "config", "settings.json"), "utf8"));
    assert.equal(saved.data_source, "whirlwind");
    await d.click('[data-feature="shell:settings"]');
    await d.waitFor(selector);
    assert.equal(await d.run(`done(document.querySelector('${selector}').value)`), "whirlwind");
    await d.run(`document.querySelector('${selector}').scrollIntoView({block:'center'}); done(true)`);
    assert.ok((await d.text(".modal.settings")).includes("Whirlwind downloads up to 64 chunks at once."));
    assert.equal(await d.exists('[data-feature="settings:concurrency"]'), false);
    await t.shot("whirlwind-selected");
    await d.click('[data-feature="settings:close"]');
    await d.waitGone('.modal.settings');
    await d.queueDialog([t.path("tools/webdriver/fixtures/supplied-wind.csv")]);
    await d.click('[data-feature="tracks:import-file"]');
    await d.waitFor('.track-import');
    await d.click('.track-import .modal-actions button.primary');
    await d.waitGone('.track-import');
    await d.waitFor('[data-feature="tracks:fetch-weather"]');
    await d.waitGone('.busy-spinner.on');
    for (let attempt = 0; attempt < 2; attempt++) {
      await d.click('[data-feature="tracks:fetch-weather"]');
      await d.waitFor('.statusbar', { text: "Whirlwind cache path is not a regular directory" });
      await d.waitGone('.env-fetch');
      await d.waitGone('.busy-spinner.on');
      const hint = await d.run(`done(document.querySelector('.statusbar .hint')?.title ?? '')`);
      assert.ok(hint.includes("Whirlwind cache path is not a regular directory"), hint);
      assert.equal((await d.invoke("env_jobs", {})).tracks.length, 0);
    }
    await t.shot("direct-fetch-failure-stops-spinner");
    await d.click('[data-feature="shell:settings"]');
    await d.waitFor(selector);
    await d.type(selector, "whirlwind_r2");
    await d.click('[data-feature="settings:close"]');
    await d.waitGone('.modal.settings');
    const restored = JSON.parse(await readFile(join(d.automationRoot, "config", "settings.json"), "utf8"));
    assert.equal(restored.data_source, "whirlwind_r2");
    assert.equal(await d.exists('.env-fetch'), false);
    await d.waitFor('[data-feature="tracks:fetch-weather"]:not(:disabled)');
    await t.shot("source-switched-without-estimate");
  },
};
