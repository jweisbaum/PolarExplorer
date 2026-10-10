import { useBoatApi } from "../boats/context";
import { useEffect, useState } from "react";

import { describeError } from "../errors";
import type { ProjectSummary } from "../generated/ProjectSummary";
import { useT } from "../i18n";

import { STATISTICS } from "../polar/EditPanel";
import { formatAxis, parseAxis, type AxisResult } from "./axes";

/** The output grid a new project starts with (spec.md 12.2). */
export const DEFAULT_GRID = {
  twa: [0, 30, 35, 40, 45, 52, 60, 70, 75, 80, 90, 100, 110, 120, 135, 150, 160, 170, 180],
  tws: [4, 6, 8, 10, 12, 14, 16, 20, 25, 30],
};

/** The most samples a setting may name (pe-core's `MAX_SAMPLE_SETTING`). */
const MAX_SAMPLES = 1_000_000;

/** A whole number of samples, 1 to a million; null otherwise. */
export function parseSamples(text: string): number | null {
  if (!/^\d+$/.test(text.trim())) return null;
  const value = Number(text.trim());
  return value >= 1 && value <= MAX_SAMPLES ? value : null;
}

/**
 * The Blend settings dialog (spec.md 8, 12): the output grid's TWA and TWS
 * axes, the statistic new tracks start with, the samples a track cell needs
 * and the samples at which it counts fully, smoothing, current correction
 * and Stokes drift. Nothing changes until Apply, which is one undoable
 * entry; Rust checks everything again and refuses what it would not keep.
 *
 * Its controls are tagged and registered, landing on the Blend settings
 * button (spec.md 3.6); Cancel and Apply are not, as in every dialog.
 */
export default function BlendSettingsDialog({ project, onProject, onClose }: {
  project: ProjectSummary;
  onProject: (project: ProjectSummary) => void;
  onClose: () => void;
}) {
  const api = useBoatApi();
  const t = useT();
  const blend = project.blend;
  const [twaText, setTwaText] = useState(formatAxis(blend.twa));
  const [twsText, setTwsText] = useState(formatAxis(blend.tws));
  const [polarStatistic, setPolarStatistic] = useState(blend.polar_statistic);
  const [statistic, setStatistic] = useState(blend.default_statistic);
  const [minText, setMinText] = useState(String(blend.min_samples));
  const [fullText, setFullText] = useState(String(blend.n_full));
  const [smoothing, setSmoothing] = useState(blend.smoothing);
  const asymmetric = blend.asymmetric;
  const [interpolation, setInterpolation] = useState(blend.interpolation);
  const [useCorrected, setUseCorrected] = useState(project.use_corrected);
  const [error, setError] = useState<unknown>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") { event.preventDefault(); onClose(); }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  const twa = parseAxis(twaText, "twa", asymmetric);
  const tws = parseAxis(twsText, "tws");
  const minSamples = parseSamples(minText);
  const nFull = parseSamples(fullText);
  const valid = twa.values !== undefined && tws.values !== undefined && minSamples !== null && nFull !== null;

  const apply = async () => {
    if (!valid) return;
    setBusy(true);
    setError(null);
    try {
      onProject(await api.setBlendSettings({
        twa: twa.values, tws: tws.values, min_samples: minSamples, n_full: nFull, smoothing,
        default_statistic: statistic, polar_statistic: polarStatistic, use_corrected: useCorrected, asymmetric, interpolation,
      }));
      onClose();
    } catch (failure) {
      setError(failure);
    } finally {
      setBusy(false);
    }
  };

  const axisError = (result: AxisResult) => result.error
    ? <p className="modal-error" role="alert">{t(result.error.key, result.error.params)}</p>
    : null;

  const title = t("Blend settings");
  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal settings blend-settings" role="dialog" aria-label={title} onClick={(event) => event.stopPropagation()}>
        <h2>{title}</h2>
        {error !== null && <p className="modal-error" role="alert" title={describeError(error).detail}>{describeError(error).text}</p>}

        <section>
          <h3>{t("Output grid")}</h3>
          <label className="settings-field">
            {t("Interpolation")}
            <select data-feature="blend-settings:interpolation" value={interpolation} onChange={(event) => setInterpolation(event.target.value)}>
              <option value="linear">{t("Linear")}</option>
              <option value="monotone_spline">{t("Monotone spline")}</option>
            </select>
          </label>
          <div className="settings-buttons">
            <label>{t("TWA step")}
              <select data-feature="blend-settings:twa-step" value="" onChange={(event) => {
                const step = Number(event.target.value);
                setTwaText(formatAxis(Array.from({ length: (asymmetric ? 360 : 180) / step + 1 }, (_, i) => i * step)));
              }}>
                <option value="" disabled>{t("Choose spacing")}</option>
                {[1, 2, 5, 10].map((step) => <option key={step} value={step}>{step}°</option>)}
              </select>
            </label>
            <label>{t("TWS step")}
              <select data-feature="blend-settings:tws-step" value="" onChange={(event) => {
                const step = Number(event.target.value);
                const maximum = parseAxis(twsText, "tws").values?.at(-1) ?? 30;
                setTwsText(formatAxis(Array.from({ length: Math.max(1, Math.ceil(maximum / step)) }, (_, i) => Math.min(70, (i + 1) * step))));
              }}>
                <option value="" disabled>{t("Choose spacing")}</option>
                {[1, 2, 5, 10].map((step) => <option key={step} value={step}>{t("{tws} kn", { tws: step })}</option>)}
              </select>
            </label>
          </div>
          <p className="muted">{t("Every source is read onto this grid and the blend is made on it. Values in order, separated by commas or spaces, with at most two decimals after a decimal point.")}</p>
          <label className="settings-field axis-field">
            {t("TWA, degrees")}
            <textarea data-feature="blend-settings:twa" rows={2} value={twaText}
              title={t("True wind angles, 0 to {max}", { max: asymmetric ? 360 : 180 })} onChange={(event) => setTwaText(event.target.value)} />
          </label>
          {axisError(twa)}
          <label className="settings-field axis-field">
            {t("TWS, knots")}
            <textarea data-feature="blend-settings:tws" rows={2} value={twsText}
              title={t("True wind speeds, 0 to 70")} onChange={(event) => setTwsText(event.target.value)} />
          </label>
          {axisError(tws)}
          <div className="settings-buttons">
            <button data-feature="blend-settings:default-grid"
              title={t("Put back the output grid a new project starts with")}
              onClick={() => { setTwaText(formatAxis(asymmetric ? [...DEFAULT_GRID.twa, ...DEFAULT_GRID.twa.slice(0, -1).reverse().map((a) => 360 - a)] : DEFAULT_GRID.twa)); setTwsText(formatAxis(DEFAULT_GRID.tws)); }}>
              {t("Default grid")}
            </button>
          </div>
        </section>

        <section>
          <h3>{t("Polars")}</h3>
          <label className="settings-field">
            {t("Polar blend statistic")}
            <select data-feature="blend-settings:polar-statistic" value={polarStatistic}
              onChange={(event) => setPolarStatistic(event.target.value)}>
              <option value="min">{t("Minimum")}</option>
              <option value="median">{t("Median")}</option>
              <option value="mean">{t("Mean")}</option>
              <option value="max">{t("Maximum")}</option>
              <option value="p90">{t("90th percentile")}</option>
            </select>
          </label>
          <p className="muted">{t("Combine visible polars per cell. Mean uses source weights; median and p90 interpolate sorted speeds. Track settings do not affect polar-only views.")}</p>
        </section>

        <section>
          <h3>{t("Tracks")}</h3>
          <label className="settings-field">
            {t("Statistic for new tracks")}
            <select data-feature="blend-settings:statistic" value={statistic}
              title={t("The statistic of a cell's boat speeds a newly imported track starts with. Each track keeps its own, changed in its edit panel.")}
              onChange={(event) => setStatistic(event.target.value)}>
              {STATISTICS.map((option) => <option key={option.id} value={option.id}>{t(option.label)}</option>)}
            </select>
          </label>
          <label className="settings-field">
            {t("Samples a cell needs")}
            <input type="number" min={1} step={1} data-feature="blend-settings:min-samples" value={minText}
              title={t("A track cell with fewer samples than this has no value")}
              onChange={(event) => setMinText(event.target.value)} />
          </label>
          <label className="settings-field">
            {t("Samples for full confidence")}
            <input type="number" min={1} step={1} data-feature="blend-settings:n-full" value={fullText}
              title={t("A track cell counts in the blend by its samples over this number, at most fully")}
              onChange={(event) => setFullText(event.target.value)} />
          </label>
          {(minSamples === null || nFull === null) && (
            <p className="modal-error" role="alert">{t("Sample counts are whole numbers from 1 to {max}.", { max: MAX_SAMPLES })}</p>
          )}
          <label className="settings-field">
            <input type="checkbox" data-feature="blend-settings:use-corrected" checked={useCorrected}
              title={t("Feed the polar from boat speed and wind through the water where a current was found")}
              onChange={(event) => setUseCorrected(event.target.checked)} />
            {t("Correct for current")}
          </label>
        </section>

        <section>
          <h3>{t("Blend")}</h3>
          <label className="settings-field">
            <input type="checkbox" data-feature="blend-settings:smoothing" checked={smoothing}
              title={t("Smooth the blended grid once its empty cells are filled")}
              onChange={(event) => setSmoothing(event.target.checked)} />
            {t("Smooth the blend")}
          </label>
          <p className="muted">{t("The polar result carries the combined polar weight when blended with tracks. Track cells count by weight and sample confidence. Empty cells are filled between known values; the 0° row is 0 kn.")}</p>
        </section>

        <div className="modal-actions">
          <button onClick={onClose} title={t("Close without changing anything")}>{t("Cancel")}</button>
          <span className="spacer" />
          <button className="primary" disabled={!valid || busy} title={t("Apply these settings as one change (undoable)")}
            onClick={() => void apply()}>
            {t("Apply")}
          </button>
        </div>
      </div>
    </div>
  );
}
