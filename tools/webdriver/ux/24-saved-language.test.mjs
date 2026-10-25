/** A saved English choice wins over a different native OS language. */
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { assert } from "./harness.mjs";

let root;

export default {
  name: "saved language takes priority over OS preferences",
  async setup() {
    root = await mkdtemp(join(tmpdir(), "pe-ux-saved-language-"));
    await mkdir(join(root, "config"));
    await writeFile(join(root, "config", "settings.json"), JSON.stringify({ language: "en" }));
    return {
      env: { PE_AUTOMATION_ROOT: root, PE_DRIVER_LOCALES: "fr-CA:de-DE" },
      teardown: () => rm(root, { recursive: true, force: true }),
    };
  },
  async run(t) {
    const d = t.driver;
    await d.waitFor('[data-feature="start:language"]');
    assert.equal((await d.invoke("app_settings", {})).language, "en");
    assert.equal(await d.text('[data-feature="start:language"]'), "en");
    assert.equal(JSON.parse(await readFile(join(root, "config", "settings.json"), "utf8")).language, "en");
    await t.shot("saved-english");
  },
};
