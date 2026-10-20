/**
 * The Settings dialog (spec.md 3.4). Copied in structure from VectorEffects:
 * one section per group, each change saved at once through Rust, which
 * validates it and answers with the settings as saved — so what is shown is
 * always what is on disk.
 */

import { useCallback, useEffect, useRef, useState } from "react";

import type { AppSettings } from "../generated/AppSettings";
import type { AutosaveMode } from "../generated/AutosaveMode";
import type { Units } from "../generated/Units";
import { onReveal } from "../help/highlight";
import { useT } from "../i18n";
import LanguagePicker from "../i18n/LanguagePicker";
import { describeError } from "../errors";
import { api } from "../ipc";
import ThemePicker from "./ThemePicker";
import LibrarySettings from "./LibrarySettings";
import IntegerField from "./IntegerField";
import WeatherCache from "./WeatherCache";
import McpSection from "./McpSection";
import OrcScraper from "./OrcScraper";
import OrrScraper from "./OrrScraper";

/** The dot bands offered, knots either side of the plot's wind speed (spec.md 9.2). */
const PLOT_BANDS = [0.25, 0.5, 1, 1.5, 2, 3, 5];

export default function SettingsDialog({ settings, onSettings, onClose }: {
  settings: AppSettings;
  onSettings: (settings: AppSettings) => void;
  onClose: () => void;
}) {
  const t = useT();
  /** The last failure, described at render so a language switch relabels it. */
  const [error, setError] = useState<unknown>(null);

  const report = (err: unknown) => setError(err ?? new Error("unknown"));
  const save = (change: Promise<AppSettings>) => {
    setError(null);
    change.then(onSettings).catch(report);
  };

  /**
   * The track library edits a draft (its folders are typed a character at a
   * time, and its YellowBrick fields go in pairs), so closing saves that
   * draft first; a draft Rust refuses keeps the dialog open with the reason.
   */
  const flushLibrary = useRef<(() => Promise<void>) | null>(null);
  const flushCache = useRef<(() => Promise<void>) | null>(null);
  const close = useCallback(() => {
    setError(null);
    (flushCache.current?.() ?? Promise.resolve()).then(() => flushLibrary.current?.()).then(onClose, report);
  }, [onClose]);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        close();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [close]);

  /**
   * The search's second reveal step (spec.md 3.6): `settings:` opened the
   * dialog, and `settings:<section>` brings that section into view. Each is
   * written out, not looped over, so the registry's test can read them.
   */
  useEffect(() => {
    const show = (step: string) => {
      document.querySelector(`[data-section="${step}"]`)?.scrollIntoView?.({ block: "start" });
    };
    const offs = [
      onReveal("settings:appearance", show),
      onReveal("settings:units", show),
      onReveal("settings:autosave", show),
      onReveal("settings:weather", show),
      onReveal("settings:cache", show),
      onReveal("settings:network", show),
      onReveal("settings:mcp", show),
      onReveal("settings:orc", show),
      onReveal("settings:orr", show),
      onReveal("settings:library", show),
    ];
    return () => { for (const off of offs) off(); };
  }, []);

  const units = (change: Partial<Units>) => save(api.setUnits({ ...settings.units, ...change }));

  return (
    <div className="modal-backdrop" onClick={close}>
      <div className="modal settings" role="dialog" aria-label={t("Settings")} onClick={(event) => event.stopPropagation()}>
        <h2>{t("Settings")}</h2>
        {error !== null && <p className="modal-error" role="alert" title={describeError(error).detail}>{describeError(error).text}</p>}

        <section data-section="settings:appearance">
          <h3>{t("Appearance")}</h3>
          <ThemePicker value={settings.theme} onChoose={(id) => save(api.setTheme(id))} />
          <label className="settings-field">
            {t("Language")}
            <LanguagePicker feature="settings:language" onSettings={onSettings} />
          </label>
          <p className="muted">{t("Applies throughout the app, across all projects.")}</p>
        </section>

        <section data-section="settings:units">
          <h3>{t("Units")}</h3>
          <label className="settings-field">
            {t("Boat and wind speed")}
            <select data-feature="settings:speed-unit" value={settings.units.speed}
              title={t("The unit speeds are shown in. Polars are stored in knots.")}
              onChange={(e) => units({ speed: e.target.value as Units["speed"] })}>
              <option value="kn">{t("kn — knots")}</option>
              <option value="ms">{t("m/s — metres per second")}</option>
              <option value="kmh">{t("km/h — kilometres per hour")}</option>
            </select>
          </label>
          <label className="settings-field">
            {t("Wave height")}
            <select data-feature="settings:wave-unit" value={settings.units.wave_height}
              title={t("The unit wave heights are shown in")}
              onChange={(e) => units({ wave_height: e.target.value as Units["wave_height"] })}>
              <option value="m">{t("m — metres")}</option>
              <option value="ft">{t("ft — feet")}</option>
            </select>
          </label>
          <label className="settings-field">
            {t("Distance")}
            <select data-feature="settings:distance-unit" value={settings.units.distance}
              title={t("The unit distances are shown in")}
              onChange={(e) => units({ distance: e.target.value as Units["distance"] })}>
              <option value="nm">{t("nm — nautical miles")}</option>
              <option value="km">{t("km — kilometres")}</option>
            </select>
          </label>
          <label className="settings-field">
            {t("Polar plot dot band")}
            <select data-feature="settings:plot-band" value={String(settings.plot_tws_band_kn)}
              title={t("How far from the polar plot's wind speed a track sample may be and still be drawn")}
              onChange={(e) => save(api.setPlotBand(Number(e.target.value)))}>
              {PLOT_BANDS.map((band) => (
                <option key={band} value={String(band)}>{t("{band} kn either side", { band })}</option>
              ))}
            </select>
          </label>
        </section>

        <section data-section="settings:autosave">
          <h3>{t("Autosave")}</h3>
          <label className="settings-field">
            {t("Unsaved work")}
            <select data-feature="settings:autosave" value={settings.autosave}
              title={t("Every minute, or every fifty edits, whichever comes first")}
              onChange={(e) => save(api.setAutosaveMode(e.target.value as AutosaveMode))}>
              <option value="recovery">{t("Keep a recovery copy, offered back after a crash")}</option>
              <option value="save">{t("Save into the project file itself")}</option>
              <option value="off">{t("Leave it until you save")}</option>
            </select>
          </label>
        </section>

        <section data-section="settings:weather">
          <h3>{t("Downloaded weather")}</h3>
          <p className="muted">
            {t("A fetch downloads only the parts of the archives that hold a track's positions, and the project keeps only the wind, waves and current at each position: kilobytes per track.")}
          </p>
          <label className="settings-field">
            {t("Keep downloaded weather in memory for this session (MB)")}
            <IntegerField data-feature="settings:weather-memory" aria-label={t("Keep downloaded weather in memory for this session (MB)")}
              title={t("Other boats of the same race reuse it instead of downloading it again. It is forgotten when PolarExplorer quits (16–4096 MB).")}
              value={settings.weather_memory_mb} min={16} max={4096}
              onCommit={(value) => save(api.setWeatherMemory(value))} />
          </label>
        </section>

        <section data-section="settings:cache">
          <h3>{t("Whirlwind cache")}</h3>
          <WeatherCache settings={settings} onSettings={onSettings} flush={flushCache} />
        </section>

        <section data-section="settings:network">
          <h3>{t("Network")}</h3>
          <p className="muted">{t("Whirlwind downloads up to 64 chunks at once.")}</p>
          <label className="settings-field">
            {t("Request timeout (s)")}
            <IntegerField data-feature="settings:timeout" aria-label={t("Request timeout (s)")}
              title={t("How long one download may take before it is abandoned (5–600 s)")}
              value={settings.network.timeout_s} min={5} max={600}
              onCommit={(value) => save(api.setNetwork({ ...settings.network, timeout_s: value }))} />
          </label>
        </section>

        <McpSection onError={setError} />

        <LibrarySettings settings={settings} onSettings={onSettings} flush={flushLibrary} />
        <OrcScraper settings={settings} onSettings={onSettings} />
        <OrrScraper settings={settings} onSettings={onSettings} />
        <div className="modal-actions">
          <button data-feature="settings:close" onClick={close} title={t("Close the settings (Esc)")}>{t("Close")}</button>
        </div>
      </div>
    </div>
  );
}
