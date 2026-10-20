/**
 * Settings → Reanalysis end to end (design 2026-10-19), against a fake
 * Hindsight Sync served here, never the person's own:
 *
 * - a first run with nothing on the saved port shows the warning: the
 *   logo beside the sentence and the link to Hindsight Sync's site;
 * - the fake's port typed into Port is probed and its archives are listed,
 *   cloud and this computer; choosing the local one saves its id (and no
 *   secret) in settings.json and its location beside it;
 * - with the fake gone, Scan finds nothing and the quiet line replaces the
 *   list, without the logo, since Hindsight Sync was found once;
 * - a fetch from the chosen folder, which does not exist, fails naming it.
 *
 * The driven app asks Hindsight Sync only on `PE_DRIVER_HINDSIGHT_PORT`,
 * the fake's port, learnt in `setup` and closed until the warning is shot;
 * the person's own Hindsight Sync is never reached.
 */
import { access, mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { assert, fakeHindsightSync, newProject } from "./harness.mjs";

const f = (id) => `[data-feature="settings:${id}"]`;
const SECTION = '[data-section="settings:reanalysis"]';

let root = null;
let fake = null;

/** Polls `read` until `ok` holds of its value, for at most `ms`. */
async function until(read, ok, ms = 10_000) {
  let value;
  for (const end = Date.now() + ms; Date.now() < end;) {
    value = await read();
    if (ok(value)) return value;
    await new Promise((r) => setTimeout(r, 100));
  }
  return value;
}

/** The two archives the fake lists, shaped as Hindsight Sync's API gives them. */
function archives(localFolder) {
  const common = {
    kind: "reanalysis", mode: "global", start_datetime: "2000-01-01T00:00:00Z",
    products: ["wind10m", "waves"], parameters: [], layout_version: 3,
  };
  return [
    {
      ...common, id: "cloud-ux", name: "UX cloud", storage: "cloud",
      uri: "s3://ux-bucket/hindsight", stop_datetime: "2025-12-31T23:00:00Z",
      chunk_degrees: 5, chunk_hours: 72,
      location: {
        protocol: "s3", provider: "other", bucket: "ux-bucket", prefix: "hindsight",
        endpoint_url: "http://127.0.0.1:9", region: "us-east-1", force_path_style: true,
        credentials: { access_key_id: "UXKEY", secret_access_key: "UXSECRET", session_token: null },
        credential_error: null,
      },
    },
    {
      ...common, id: "local-ux", name: "UX copy", storage: "local",
      uri: localFolder, source_uri: "s3://ux-bucket/hindsight", stop_datetime: null,
      chunk_degrees: 10, chunk_hours: 24,
      location: { protocol: "file", path: localFolder },
    },
  ];
}

export default {
  name: "reanalysis archive from Hindsight Sync",
  async setup() {
    root = await mkdtemp(join(tmpdir(), "pe-ux-reanalysis-"));
    await mkdir(join(root, "config"), { recursive: true });
    // A first run on a closed port; the fake's port is learnt now and closed until wanted.
    await writeFile(join(root, "config", "settings.json"),
      JSON.stringify({ reanalysis: { port: 9, connected_once: false, archive: null } }));
    fake = await fakeHindsightSync(archives(join(root, "hindsight-local-copy")));
    await fake.close();
    return {
      env: { PE_AUTOMATION_ROOT: root, PE_DRIVER_HINDSIGHT_PORT: String(fake.port) },
      teardown: async () => {
        await fake.close();
        await rm(root, { recursive: true, force: true });
      },
    };
  },
  async run(t) {
    const d = t.driver;
    const settingsFile = join(root, "config", "settings.json");
    const locationFile = join(root, "config", "reanalysis-archive.json");
    const localFolder = join(root, "hindsight-local-copy");
    const listed = archives(localFolder);
    await newProject(d, "Reanalysis");

    // 1. The first-run warning.
    await d.click('[data-feature="shell:settings"]');
    await d.waitFor(f("reanalysis-link"));
    await d.run(`document.querySelector(arguments[0]).scrollIntoView({ block: "center" }); done(true);`, [SECTION]);
    assert.ok(await d.exists(`${SECTION} img[alt="Hindsight Sync logo"]`), "the logo is shown");
    assert.ok((await d.text(SECTION)).includes("PolarExplorer needs Hindsight Sync to download the reanalysis."));
    assert.equal(await d.text(f("reanalysis-link")), "hindsight-sync.whirlwindai.com");
    await t.shot("warning");

    // 2. Hindsight Sync starts; its port typed in is probed and listed.
    await fake.listen();
    try {
      await d.type(f("reanalysis-port"), String(fake.port));
      await d.key("Enter", f("reanalysis-port"));
      await d.click(f("reanalysis-scan"));
      await d.waitFor(f("reanalysis-archive"), { timeoutMs: 15_000 });
      const options = `${f("reanalysis-archive")} option`;
      const count = await until(() => d.count(options), (n) => n === 1 + listed.length);
      assert.equal(count, 1 + listed.length, "the placeholder and every listed archive");
      const groups = await d.run(`done([...document.querySelectorAll(arguments[0])].map(g => g.label));`, [`${f("reanalysis-archive")} optgroup`]);
      assert.deepEqual(groups, ["Cloud", "This computer"]);
      assert.ok(!(await d.exists(`${SECTION} img[alt="Hindsight Sync logo"]`)), "no warning once detected");
      await d.run(`document.querySelector(arguments[0]).scrollIntoView({ block: "center" }); done(true);`, [SECTION]);
      await t.shot("detected");

      // Choose the local copy.
      await d.type(f("reanalysis-archive"), "local-ux");
      const saved = await until(
        async () => JSON.parse(await readFile(settingsFile, "utf8")),
        (s) => s.reanalysis?.archive?.id === "local-ux",
      );
      assert.equal(saved.reanalysis.archive.id, "local-ux");
      assert.equal(saved.reanalysis.connected_once, true);
      assert.equal(saved.reanalysis.port, fake.port);
      const text = await readFile(settingsFile, "utf8");
      assert.ok(!text.includes("secret_access_key") && !text.includes("UXSECRET"), "no secret in settings.json");
      await access(locationFile);
      await d.waitFor(`${SECTION} p.muted`, { text: "from 2000-01-01, ongoing · global" });
      assert.ok((await d.text(`${SECTION} p.muted`)).includes(`Connected to Hindsight Sync on port ${fake.port}`));
    } finally {
      await fake.close();
    }

    // 3. Hindsight Sync gone: Scan finds nothing, and says so quietly.
    await d.click(f("reanalysis-scan"));
    await d.waitFor(SECTION, { text: "Hindsight Sync is not running. The remembered archive is still used for fetches.", timeoutMs: 15_000 });
    assert.ok(!(await d.exists(`${SECTION} img[alt="Hindsight Sync logo"]`)), "no logo once connected before");
    assert.ok(!(await d.exists(f("reanalysis-link"))));
    await d.run(`document.querySelector(arguments[0]).scrollIntoView({ block: "center" }); done(true);`, [SECTION]);
    await t.shot("not-running");
    await d.click(f("close"));
    await d.waitGone(".modal.settings");

    // 4. A fetch from the remembered folder, which is not there, names it.
    await d.queueDialog([t.path("tools/webdriver/fixtures/supplied-wind.csv")]);
    await d.click('[data-feature="tracks:import-file"]');
    await d.waitFor(".track-import", { visible: true });
    await d.click(".track-import .modal-actions button.primary");
    await d.waitGone(".track-import");
    await d.waitFor('[data-feature="tracks:fetch-weather"]', { timeoutMs: 30_000 });
    await d.click('[data-feature="tracks:fetch-weather"]');
    await d.waitFor(".statusbar", { text: localFolder, timeoutMs: 30_000 });
  },
};
