import { useEffect, useRef, useState } from "react";
import { msg, useT } from "../i18n";
import type { MatrixExportOptions } from "../generated/MatrixExportOptions";
import type { MatrixWaveBins } from "../generated/MatrixWaveBins";

const defaultWaves = (): MatrixWaveBins => ({
  height: { cutoffs: [1, 3, 5] }, angle: { edges: [-45, 45, 135, 225, 315] }, period: { cutoffs: [5] },
});
export const defaultMatrixExport = (): MatrixExportOptions => ({
  sog: true, sampled_pdf: true, gaussian_pdf: true, waves: defaultWaves(), sog_bins: 120,
});

const increasing = (values: number[]) => values.every((v, i) => Number.isFinite(v) && (i === 0 || v > values[i - 1]!));
const directionCenter = (lo: number, hi: number) => Math.fround((((lo + hi) / 2) % 360 + 360) % 360) % 360;
function validCutoffs(cutoffs: number[], limit: number): boolean {
  if (cutoffs.length > 63 || !increasing(cutoffs) || cutoffs.some(v => v <= 0 || v >= limit)) return false;
  const centers = cutoffs.map((v, i) => Math.fround(((cutoffs[i - 1] ?? 0) + v) / 2));
  const last = cutoffs.at(-1) ?? 0;
  centers.push(Math.fround(Math.min(limit, last + (last - (cutoffs.at(-2) ?? 0)) / 2)));
  return increasing(centers);
}
function waveError(waves: MatrixWaveBins): string | null {
  if (!validCutoffs(waves.height.cutoffs, 100) || !validCutoffs(waves.period.cutoffs, 1000)) {
    return msg("Use up to 63 increasing cutoffs above 0, below 100 m for height and 1000 s for period.");
  }
  const edges = waves.angle.edges;
  if (edges.length < 2 || edges.length > 65 || !increasing(edges) || edges[0]! < -360 || edges[0]! >= 360
    || Math.abs(edges.at(-1)! - edges[0]! - 360) > 1e-9
    || !increasing(edges.slice(1).map((v, i) => directionCenter(edges[i]!, v)).sort((a, b) => a - b))) {
    return msg("Use 2–65 increasing direction boundaries spanning 360°, starting from −360° to less than 360°.");
  }
  return null;
}
const waveCount = (waves: MatrixWaveBins) => (waves.height.cutoffs.length + 1) * (waves.angle.edges.length - 1) * (waves.period.cutoffs.length + 1);

export function matrixExportError(value: MatrixExportOptions, windCells?: number): string | null {
  if (!value.sog && !value.sampled_pdf && !value.gaussian_pdf) return msg("Choose at least one matrix data mode.");
  if (value.sampled_pdf && (!Number.isInteger(value.sog_bins) || value.sog_bins < 2 || value.sog_bins > 512)) return msg("Choose 2–512 boat-speed bins.");
  if (value.waves) {
    const error = waveError(value.waves);
    if (error) return error;
  }
  const combinations = value.waves ? waveCount(value.waves) : 1;
  const perCell = Number(value.sog) + (value.sampled_pdf ? value.sog_bins : 0) + 2 * Number(value.gaussian_pdf);
  if (combinations > 4096 || (windCells !== undefined && combinations * windCells * perCell > 16000000)) return msg("The matrix is too large. Reduce the bin counts or wind grid.");
  return null;
}

const parseValues = (text: string): number[] => text.trim() === "" ? [] : text.split(/[,;\n]/).map(v => v.trim() === "" ? NaN : Number(v.trim()));
/** Keep unfinished decimals and trailing separators editable while validation disables export. */
function CutoffInput({ values, onChange, label, feature }: {
  values: number[]; onChange: (values: number[]) => void; label: string; feature: string;
}) {
  const t = useT();
  const [text, setText] = useState(() => values.join(", "));
  const draft = useRef(text);
  useEffect(() => {
    const parsed = parseValues(draft.current);
    if (parsed.length !== values.length || parsed.some((v, i) => !Object.is(v, values[i]))) {
      draft.current = values.join(", ");
      setText(draft.current);
    }
  }, [values]);
  return <label>{t(label)}<input type="text" dir="ltr" data-feature={feature} value={text}
    onChange={event => { draft.current = event.target.value; setText(draft.current); onChange(parseValues(draft.current)); }} /></label>;
}

function cutoffRanges(cutoffs: number[], inclusive: boolean): string {
  if (cutoffs.length === 0) return "≥0";
  const ranges = cutoffs.map((v, i) => inclusive
    ? `${i === 0 ? "0" : `>${cutoffs[i - 1]}`}–${v}`
    : `${cutoffs[i - 1] ?? 0}–<${v}`);
  return [...ranges, `${inclusive ? ">" : "≥"}${cutoffs.at(-1)}`].join(" · ");
}

/** Shared by single-tab and fleet export. Default buckets need no configuration. */
export default function MatrixExportFields({ value, onChange, disabled, windCells }: {
  value: MatrixExportOptions; onChange: (value: MatrixExportOptions) => void; disabled: boolean; windCells?: number;
}) {
  const t = useT();
  const error = matrixExportError(value, windCells);
  const waves = value.waves;
  const validWaves = waves !== null && waveError(waves) === null;
  const combinations = waves ? waveCount(waves) : 1;
  const number = (v: number) => Number.isFinite(v) ? v : "";
  return <>
    <details className="matrix-export">
      <summary data-feature="matrix-export:toggle">{t("Matrix data modes")}</summary>
      <fieldset className="matrix-export-fields" disabled={disabled} aria-label={t("Matrix data modes")}>
        <div className="matrix-export-modes">
          <label><input type="checkbox" data-feature="matrix-export:sog" checked={value.sog} onChange={e => onChange({ ...value, sog: e.target.checked })} />{t("Boat speed (SOG)")}</label>
          <label><input type="checkbox" data-feature="matrix-export:histogram" checked={value.sampled_pdf} onChange={e => onChange({
            ...value, sampled_pdf: e.target.checked,
            sog_bins: Number.isInteger(value.sog_bins) && value.sog_bins >= 2 && value.sog_bins <= 512 ? value.sog_bins : 120,
          })} />{t("Histogram (sampled PDF)")}</label>
          <label><input type="checkbox" data-feature="matrix-export:gaussian" checked={value.gaussian_pdf} onChange={e => onChange({ ...value, gaussian_pdf: e.target.checked })} />{t("Gaussian PDF")}</label>
        </div>
        <p className="muted">{t("Histogram counts use source weights and blend settings; Gaussian mode stores mean and standard deviation.")}</p>
        {value.sampled_pdf && <label className="matrix-speed-bins">{t("Boat-speed bins (0–60 kn)")}
          <input type="number" dir="ltr" data-feature="matrix-export:speed-bins" min={2} max={512} step={1} value={number(value.sog_bins)} onChange={e => onChange({ ...value, sog_bins: e.target.valueAsNumber })} />
        </label>}
        <label className="matrix-wave-toggle"><input type="checkbox" data-feature="matrix-export:waves" checked={waves !== null}
          onChange={e => onChange({ ...value, waves: e.target.checked ? defaultWaves() : null })} />{t("Split by wave conditions")}</label>
        {waves && <>
          {validWaves && <dl className="matrix-wave-summary">
            <dt>{t("Wave height (m)")}</dt><dd><bdi dir="ltr">{cutoffRanges(waves.height.cutoffs, false)}</bdi></dd>
            <dt>{t("Wave angle off bow (°)")}</dt><dd><bdi dir="ltr">{waves.angle.edges.slice(1).map((v, i) => `${waves.angle.edges[i]}–<${v}`).join(" · ")}</bdi></dd>
            <dt>{t("Wave period (s)")}</dt><dd><bdi dir="ltr">{cutoffRanges(waves.period.cutoffs, true)}</bdi></dd>
          </dl>}
          <details className="matrix-wave-custom">
            <summary data-feature="matrix-export:customize">{t("Customize wave buckets (optional)")}</summary>
            <div className="matrix-wave-grid">
              <p className="muted">{t("Enter comma-separated cutoffs. Add or remove a cutoff to change the number of buckets. Leave height or period blank for one bucket.")}</p>
              <CutoffInput label={msg("Wave height cutoffs (m)")} feature="matrix-export:height-cutoffs" values={waves.height.cutoffs}
                onChange={cutoffs => onChange({ ...value, waves: { ...waves, height: { cutoffs } } })} />
              <CutoffInput label={msg("Wave period cutoffs (s)")} feature="matrix-export:period-cutoffs" values={waves.period.cutoffs}
                onChange={cutoffs => onChange({ ...value, waves: { ...waves, period: { cutoffs } } })} />
              <p className="muted">{t("Height cutoffs enter the next bucket; period cutoffs stay in the preceding bucket. Final buckets have no upper limit.")}</p>
              <CutoffInput label={msg("Wave direction boundaries (°)")} feature="matrix-export:direction-edges" values={waves.angle.edges}
                onChange={edges => onChange({ ...value, waves: { ...waves, angle: { edges } } })} />
              <p className="muted">{t("Enter increasing boundaries spanning 360°. Directions run clockwise from the bow; −45° is the same as 315°.")}</p>
              <button type="button" data-feature="matrix-export:reset" onClick={() => onChange({ ...value, waves: defaultWaves() })}>{t("Reset wave buckets")}</button>
            </div>
          </details>
          <p className="muted">{t("Each wave combination gets separate wind matrices. Track samples without complete wave data are omitted; ordinary polars apply to every combination.")}</p>
        </>}
        {(waves === null || validWaves) && <p>{t("Wave combinations: {count}", { count: combinations })}</p>}
      </fieldset>
    </details>
    {error && <p className="modal-error" role="alert">{t(error)}</p>}
  </>;
}
