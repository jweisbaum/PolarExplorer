/**
 * What every UX test shares (D25): a step that photographs, the assertions,
 * and the flows more than one test walks through.
 *
 * A test is a module under `tools/webdriver/ux/` named `NN-name.test.mjs`
 * whose default export is `{ name, env?, setup?, run }`:
 *
 * - `setup()` runs before the application starts and may return
 *   `{ env, teardown }` (the tracker test starts its fixture server here).
 * - `run(t)` drives the application through `t.driver` (see `client.mjs`)
 *   and calls `await t.shot("label")` after each step worth seeing. The
 *   pictures land in `target/ux-shots/<test>/NN-label.png`.
 *
 * Every test gets its own application and its own data root, so no test
 * sees another's settings, recent list or session.
 */

import assert from "node:assert/strict";
import { mkdir, writeFile } from "node:fs/promises";
import { createServer } from "node:http";
import { join, resolve } from "node:path";

import { ROOT, safeName } from "../client.mjs";

export { assert };

/** Fail before HTTP using a missing folder in this driven run's data root. */
export async function blockWeatherDownloads(driver) {
  const settings = await driver.invoke("app_settings", {});
  const archive = settings.reanalysis.archive;
  if (!archive) return;
  await writeFile(join(driver.automationRoot, "config", "reanalysis-archive.json"), JSON.stringify({
    id: archive.id,
    location: { protocol: "file", path: join(driver.automationRoot, "missing-weather-archive") },
  }));
}

/** Where a test's pictures go. Under `target/`, which git ignores. */
export function shotsDir(test) {
  return join(ROOT, "target", "ux-shots", safeName(test));
}

/** The context a test's `run` receives. */
export function context(test, driver) {
  let n = 0;
  const shots = [];
  return {
    driver,
    root: ROOT,
    shots,
    /** Photographs the window as step `label`. Returns the file's path. */
    async shot(label) {
      n += 1;
      const file = join(shotsDir(test), `${String(n).padStart(2, "0")}-${safeName(label)}.png`);
      await driver.screenshot(file);
      shots.push(file);
      return file;
    },
    /** A repository file's absolute path. */
    path(relative) {
      return resolve(ROOT, relative);
    },
  };
}

/**
 * From the start screen, creates a project called `name` through the form and
 * waits for the project window (the 3D stage, the default).
 */
export async function newProject(driver, name = "UX test") {
  await driver.waitFor('[data-feature="new:create"]');
  await driver.type('[data-feature="new:name"] input', name);
  await driver.click('[data-feature="new:create"]');
  await driver.waitFor('[data-feature="stage:3d"][aria-selected="true"]', { timeoutMs: 30_000 });
  await driver.waitFor("canvas.view3d-canvas", { visible: true });
}

/**
 * How many pixels of a canvas are close to `hex` (`#rrggbb`), read through
 * the page's own 2D context. A WebGL canvas is redrawn first and copied, in
 * the same task, as the screenshot does.
 */
export function colourPixels(driver, selector, hex, tolerance = 40) {
  return driver.run(
    `var c = document.querySelector(arguments[0]);
     if (!c) { done(-1); return; }
     if (typeof c.__peRedraw === "function") c.__peRedraw();
     var copy = document.createElement("canvas");
     copy.width = c.width; copy.height = c.height;
     var ctx = copy.getContext("2d");
     ctx.drawImage(c, 0, 0);
     var data = ctx.getImageData(0, 0, copy.width, copy.height).data;
     var hex = arguments[1], tol = arguments[2];
     var r = parseInt(hex.slice(1, 3), 16), g = parseInt(hex.slice(3, 5), 16), b = parseInt(hex.slice(5, 7), 16);
     var n = 0;
     for (var i = 0; i < data.length; i += 4) {
       if (Math.abs(data[i] - r) <= tol && Math.abs(data[i + 1] - g) <= tol && Math.abs(data[i + 2] - b) <= tol) n++;
     }
     done(n);`,
    [selector, hex, tolerance],
  );
}

/** How many distinct colours a canvas shows (coarsely): a blank canvas has one. */
export function distinctColours(driver, selector) {
  return driver.run(
    `var c = document.querySelector(arguments[0]);
     if (!c) { done(-1); return; }
     if (typeof c.__peRedraw === "function") c.__peRedraw();
     var copy = document.createElement("canvas");
     copy.width = c.width; copy.height = c.height;
     var ctx = copy.getContext("2d");
     ctx.drawImage(c, 0, 0);
     var data = ctx.getImageData(0, 0, copy.width, copy.height).data;
     var seen = new Set();
     for (var i = 0; i < data.length; i += 16) seen.add((data[i] >> 4) + "," + (data[i + 1] >> 4) + "," + (data[i + 2] >> 4));
     done(seen.size);`,
    [selector],
  );
}

/** A CSS colour as `#rrggbb` (the swatches carry `rgb(…)` once rendered). */
export function toHex(css) {
  const m = /rgba?\((\d+),\s*(\d+),\s*(\d+)/.exec(css ?? "");
  if (!m) return css;
  return `#${[m[1], m[2], m[3]].map((v) => Number(v).toString(16).padStart(2, "0")).join("")}`;
}

/** Changes the interface language through Settings, as a person would. */
export async function setLanguageInSettings(driver, language) {
  await driver.click('[data-feature="shell:settings"]');
  await driver.waitFor('[data-feature="settings:language"]');
  await driver.type('[data-feature="settings:language"]', language);
  await driver.waitFor(`html[lang="${language}"]`, { timeoutMs: 10_000 });
}

/**
 * A stand-in for Hindsight Sync's archive API on `127.0.0.1`:
 * `GET /v1/archives/{cloud,local}/reanalysis` answer the `archives` of that
 * storage, as the real API does; a request carrying an `Origin` header is
 * refused with 403 like the real one. It starts listening on a port the
 * system picks; `close()` stops it and `listen()` starts it again on the
 * same port, so a test can learn the port before the application starts
 * (`PE_DRIVER_HINDSIGHT_PORT`) and show Hindsight Sync not running.
 */
export async function fakeHindsightSync(archives) {
  const server = createServer((req, res) => {
    const answer = (status, body) => {
      res.writeHead(status, { "content-type": "application/json" });
      res.end(JSON.stringify(body));
    };
    if (req.headers.origin !== undefined) return answer(403, { error: "local_applications_only" });
    const m = /^\/v1\/archives\/(cloud|local)\/reanalysis$/.exec(req.url ?? "");
    if (req.method !== "GET" || !m) return answer(404, { error: "not_found" });
    answer(200, { api_version: 1, archives: archives.filter((a) => a.storage === m[1]), errors: [] });
  });
  const listen = (port) => new Promise((done, fail) => {
    server.once("error", fail);
    server.listen(port, "127.0.0.1", () => {
      server.off("error", fail);
      done(server.address().port);
    });
  });
  const fake = {
    port: await listen(0),
    listen: () => listen(fake.port),
    close: () => new Promise((done) => {
      if (!server.listening) return done();
      server.closeAllConnections();
      server.close(() => done());
    }),
  };
  return fake;
}
