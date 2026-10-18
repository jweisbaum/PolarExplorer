import { readFile } from "node:fs/promises";
import { join } from "node:path";
import { assert, newProject } from "./harness.mjs";

const f = id => `[data-feature="${id}"]`;

export default {
  name: "fixed wave sectors and explicit bucket export",
  async run(t) {
    const d = t.driver;
    await newProject(d, "Matrix buckets");
    await d.queueDialog([t.path("crates/pe-polar/tests/golden/expedition.txt")]);
    await d.click(f("polar-files:import"));
    await d.waitFor(".polar-file-list li", { text: "expedition" });
    await d.click(f("sources:export"));
    await d.type(f("export:format"), "whirlwind_matrix_polar");
    await d.click(f("matrix-export:toggle"));
    await d.click(f("matrix-export:histogram"));
    await d.click(f("matrix-export:gaussian"));
    await d.click(f("matrix-export:customize"));
    assert.deepEqual(await d.run(`done(Array.from(document.querySelectorAll('[data-feature="matrix-export:directions"] option'), o => o.value))`), ["2", "4", "8", "20"]);
    await d.type(f("matrix-export:directions"), "2");
    await d.waitFor(".matrix-wave-summary", { text: "-90–<90" });
    await t.shot("bow-and-stern");
    await d.type(f("matrix-export:directions"), "8");
    await d.waitFor(".matrix-wave-summary", { text: "-22.5–<22.5" });
    await d.type(f("matrix-export:directions"), "20");
    await d.type(f("matrix-export:height-cutoffs"), "0.75, 2.25");
    await d.type(f("matrix-export:period-cutoffs"), "4.5, 9");
    await d.waitFor(".matrix-export-fields", { text: "Wave combinations: 180" });
    await t.shot("twenty-directions-custom-cutoffs");
    const path = join(d.automationRoot, "buckets.wmpolar");
    await d.queueDialog(path);
    await d.waitFor(".export-dialog .modal-actions .primary:not(:disabled)");
    await d.click(".export-dialog .modal-actions .primary");
    await d.waitGone(".export-dialog", { timeoutMs: 30000 });
    const bytes = await readFile(path);
    assert.equal(bytes.readUInt16LE(8), 1);
    assert.equal(bytes.readUInt32LE(20), 3);
    assert.equal(bytes.readUInt32LE(24), 20);
    assert.equal(bytes.readUInt32LE(28), 3);
    const cutoffs = 44 + 4 * (bytes.readUInt32LE(12) + bytes.readUInt32LE(16));
    assert.deepEqual([0, 4, 8, 12].map(i => bytes.readFloatLE(cutoffs + i)), [0.75, 2.25, 4.5, 9]);
    await d.queueDialog([path]);
    await d.click(f("polar-files:import"));
    await d.waitFor(".polar-file-list li", { text: "buckets.wmpolar" });
    await d.click(f("sources:export"));
    await d.type(f("export:format"), "whirlwind_matrix_polar");
    await d.click(f("matrix-export:toggle"));
    await d.click(f("matrix-export:waves"));
    await d.waitFor(".matrix-export-fields", { text: "Wave combinations: 1" });
    assert.equal(await d.exists(f("matrix-export:directions")), false);
    await t.shot("waves-disabled");
  },
};
