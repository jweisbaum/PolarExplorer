/** First launch follows the native OS preference list and remembers the match. */
import { readFile } from "node:fs/promises";
import { join } from "node:path";

import { assert } from "./harness.mjs";

export default {
  name: "initial language from OS preferences",
  env: { PE_DRIVER_LOCALES: "pt-BR:fr-CA:de-DE" },
  async run(t) {
    const d = t.driver;
    await d.waitFor('html[lang="fr"]');
    await d.waitFor('[data-feature="start:language"]');
    assert.equal(await d.text('[data-feature="start:language"]'), "fr");
    const file = join(d.automationRoot, "config", "settings.json");
    assert.equal(JSON.parse(await readFile(file, "utf8")).language, "fr");
    assert.equal((await d.invoke("app_settings", {})).language, "fr");
    await t.shot("first-launch-french");

    await d.type('[data-feature="start:language"]', "de");
    await d.waitFor('html[lang="de"]');
    // The picker relabels optimistically; wait for the save behind it.
    for (let attempt = 0; attempt < 100; attempt += 1) {
      if (JSON.parse(await readFile(file, "utf8")).language === "de") break;
      await new Promise(resolve => setTimeout(resolve, 100));
    }
    assert.equal(JSON.parse(await readFile(file, "utf8")).language, "de");
    await t.shot("chosen-german");
  },
};
