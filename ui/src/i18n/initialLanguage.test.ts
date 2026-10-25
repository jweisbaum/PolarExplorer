// @vitest-environment happy-dom
import { afterEach, beforeEach, expect, it, vi } from "vitest";

beforeEach(() => {
  vi.resetModules();
  localStorage.clear();
});
afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
  localStorage.clear();
});

it("uses the first supported OS preference for the first frame", async () => {
  vi.stubGlobal("navigator", { languages: ["pt-BR", "fr-CA", "de-DE"] });
  const { language, t } = await import("./index");
  expect(language()).toBe("fr");
  expect(t("Help")).toBe("Aide");
  expect(document.documentElement.lang).toBe("fr");
});

it("keeps a saved choice, including English, ahead of OS preferences", async () => {
  vi.stubGlobal("navigator", { languages: ["fr-CA"] });
  localStorage.setItem("pe.language", "en");
  const { language } = await import("./index");
  expect(language()).toBe("en");
});

it("handles regional Arabic and sets the correct reading direction", async () => {
  vi.stubGlobal("navigator", { languages: [], language: "AR_EG.UTF-8" });
  const { language } = await import("./index");
  expect(language()).toBe("ar");
  expect(document.documentElement.dir).toBe("rtl");
});

it("falls back to English when no OS locale is supported", async () => {
  vi.stubGlobal("navigator", { languages: ["pt-BR", "sv-SE"] });
  const { language } = await import("./index");
  expect(language()).toBe("en");
});

it("detects the language even when local storage is unavailable", async () => {
  vi.stubGlobal("navigator", { languages: ["ja-JP"] });
  vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => { throw new Error("unavailable"); });
  const { language } = await import("./index");
  expect(language()).toBe("ja");
});

it("caches the authoritative backend choice even when it matches the first frame", async () => {
  vi.stubGlobal("navigator", { languages: ["fr-CA"] });
  const { language, setLanguage } = await import("./index");
  setLanguage("fr");
  expect(localStorage.getItem("pe.language")).toBe("fr");
  setLanguage("de");
  expect(language()).toBe("de");
  expect(localStorage.getItem("pe.language")).toBe("de");
});
