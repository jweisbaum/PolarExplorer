/**
 * The search's registry agrees with the interface (spec.md 3.6): every
 * control tagged `data-feature` is findable, every registered feature has an
 * element to flash, every reveal step has a handler, and every feature is
 * found in every language by its own translated label.
 */
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { FEATURES, searchFeatures } from "./features";
import { TOPICS } from "./topics";
import { LANGUAGES, setLanguage, t } from "../i18n";
import sailorQueries from "./sailor-queries.json";

const ROOT = fileURLToPath(new URL("..", import.meta.url));

function source(dir: string, out: string[] = [], tsxOnly = false): string[] {
  for (const name of readdirSync(dir)) {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) { if (name !== "generated" && name !== "locales") source(path, out, tsxOnly); }
    else if ((tsxOnly ? /\.tsx$/ : /\.tsx?$/).test(name) && !/\.test\.tsx?$/.test(name)) out.push(readFileSync(path, "utf8"));
  }
  return out;
}
const text = source(ROOT).join("\n");
const jsx = source(ROOT, [], true).join("\n");
/** `data-feature="x"`, and `feature="x"` on the components that pass one through. */
const tagged = new Set([...jsx.matchAll(/(?:data-feature|\bfeature)=(?:"([^"$]+)"|\{"([^"$]+)"\})/g)].map(m => m[1] ?? m[2]!));
/** `data-feature={`stage:${…}`}`: a family of ids built from a prefix. */
for (const match of jsx.matchAll(/filterFeature\(prefix, "([^"$]+)"\)/g)) {
  tagged.add(`tracks:${match[1]}`);
  if (!["time-start", "time-end"].includes(match[1]!)) {
    tagged.add(`global-filters:${match[1]}`);
    tagged.add(`priority-filters:${match[1]}`);
  }
}
const families = [...jsx.matchAll(/data-feature=\{`([^`$]*)\$\{/g)].map(m => m[1]!);
const reveals = [...text.matchAll(/onReveal\(\s*"([^"]+)"/g)].map(m => m[1]!);

describe("feature registry", () => {
  it("has unique ids", () => {
    const ids = FEATURES.map(f => f.id);
    expect(ids.filter((id, i) => ids.indexOf(id) !== i)).toEqual([]);
  });
  it("registers every tagged control", () => {
    const ids = new Set(FEATURES.map(f => f.id));
    expect([...tagged].filter(id => !ids.has(id))).toEqual([]);
  });
  /**
   * An id with no literal tag must come from a family (`project:${…}`) *and*
   * its suffix must be written somewhere as a string literal, so a family
   * cannot hide a registry entry that no element carries.
   */
  const withoutElement = (ids: string[]) => ids.filter(id => {
    if (tagged.has(id)) return false;
    const family = families.find(p => id.startsWith(p));
    return family === undefined || !text.includes(`"${id.slice(family.length)}"`);
  });
  it("has an element for every feature", () => {
    expect(withoutElement(FEATURES.map(f => f.id))).toEqual([]);
  });
  it("does not let a family excuse an entry nothing renders", () => {
    expect(families).toContain("project:");
    expect(withoutElement(["project:zq-no-such-item", "stage:zq-no-such-stage"])).toEqual(["project:zq-no-such-item", "stage:zq-no-such-stage"]);
  });
  it("names help pages that exist", () => {
    const topics = new Set(TOPICS.map(p => p.id));
    expect(FEATURES.filter(f => !f.topic || !topics.has(f.topic)).map(f => `${f.id} → ${f.topic}`)).toEqual([]);
  });
  it("has a handler for every reveal step", () => {
    const steps = FEATURES.flatMap(f => f.reveal ?? []);
    expect(steps.filter(step => !reveals.some(r => r === step || (r.endsWith(":") && step.startsWith(r))))).toEqual([]);
  });
  it("lands dialog-only controls on a registered, tagged control that opens the dialog", () => {
    const ids = new Set(FEATURES.map(f => f.id));
    const landed = FEATURES.filter(f => f.landing !== undefined);
    expect(landed.length).toBeGreaterThan(0);
    expect(landed.filter(f => !ids.has(f.landing!) || !tagged.has(f.landing!)).map(f => f.id)).toEqual([]);
    // Each is still a real, tagged control of its own.
    expect(withoutElement(landed.map(f => f.id))).toEqual([]);
  });
  it("describes every feature", () => {
    expect(FEATURES.filter(f => !f.description || !f.keywords?.length).map(f => f.id)).toEqual([]);
  });

  for (const { id: language } of LANGUAGES) {
    it(`finds every feature by its label in ${language}`, () => {
      setLanguage(language);
      try {
        const missed = FEATURES.filter(f => !searchFeatures(t(f.label)).some(m => m.feature.id === f.id)).map(f => f.id);
        expect(missed).toEqual([]);
      } finally {
        setLanguage("en");
      }
    });
  }

  it("searches in the language on screen, ignoring case and accents", () => {
    setLanguage("en");
    expect(searchFeatures("langu").map(m => m.feature.id)).toContain("settings:language");
    setLanguage("fr");
    expect(searchFeatures("langue").map(m => m.feature.id)).toContain("settings:language");
    // « Thème », typed without its accent.
    expect(searchFeatures("theme").map(m => m.feature.id)).toContain("settings:theme");
    setLanguage("de");
    expect(searchFeatures("SPRACHE").map(m => m.feature.id)).toContain("settings:language");
    // „Größenbegrenzung“ and „Öffnen…“, typed without umlauts.
    expect(searchFeatures("offnen").map(m => m.feature.id)).toContain("project:open");
    setLanguage("en");
  });

  it("does not match English words once the interface is in another language", () => {
    setLanguage("fr");
    expect(searchFeatures("Projection").map(m => m.feature.id)).toContain("map:projection");
    expect(searchFeatures("Wave height").map(m => m.feature.id)).not.toContain("settings:wave-unit");
    setLanguage("en");
  });

  it("ranks a label match above a description match", () => {
    setLanguage("en");
    expect(searchFeatures("help")[0]?.feature.id).toMatch(/:help$/);
  });

  it("uses a help page's title, in the language on screen, to find its controls", () => {
    setLanguage("de");
    // Only the page title, „Das Projektfenster“, says this.
    expect(searchFeatures("projektfenster").map(m => m.feature.id)).toContain("shell:statusbar");
    setLanguage("en");
  });
});

/**
 * Twenty words a sailor would type in each language, each finding the
 * control meant as the first result (M17b). `npm run ux` drives the same
 * list through the application and checks the flash lands on the control
 * (or, for one inside a dialog, on the button that opens it).
 */
describe("a sailor's words", () => {
  for (const [language, queries] of Object.entries(sailorQueries as unknown as Record<string, [string, string, string][]>)) {
    it(`find the control meant first, in ${language}`, () => {
      setLanguage(language);
      try {
        const wrong = queries
          .map(([query, id]) => [query, id, searchFeatures(query)[0]?.feature.id ?? "nothing"])
          .filter(([, id, found]) => found !== id)
          .map(([query, id, found]) => `${query}: ${found}, not ${id}`);
        expect(wrong).toEqual([]);
        expect(queries.length).toBe(20);
        for (const [, id, flashed] of queries) {
          const feature = FEATURES.find(f => f.id === id)!;
          expect(feature.landing ?? feature.id, id).toBe(flashed);
        }
      } finally {
        setLanguage("en");
      }
    });
  }
});
