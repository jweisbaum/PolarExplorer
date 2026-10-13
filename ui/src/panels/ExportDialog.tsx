import { useBoatApi } from "../boats/context";
import { useEffect, useMemo, useState } from "react";

import { describeError, reportFailure } from "../errors";
import type { ExportPreview } from "../generated/ExportPreview";
import type { ExportProblemView } from "../generated/ExportProblemView";
import type { ProjectSummary } from "../generated/ProjectSummary";
import { later, setHint } from "../hint";
import { msg, useT } from "../i18n";

import { pickExportPath } from "../project/dialogs";
import { formatAxis, parseAxis } from "./axes";
import MatrixExportFields, { defaultMatrixExport, matrixExportError } from "./MatrixExportFields";

/** The formats offered (spec.md 6): each writer's layout and extension. */
export const EXPORT_FORMATS: readonly { id: string; label: string }[] = [
  { id: "whirlwind_matrix_polar", label: msg("WhirlwindMatrixPolar (.wmpolar)") },
  { id: "expedition", label: msg("Expedition (.txt)") },
  { id: "adrena", label: msg("Adrena (.pol)") },
  { id: "csv", label: msg("CSV (.csv)") },
  { id: "qtvlm", label: msg("qtVlm (.csv)") },
  { id: "opencpn", label: msg("OpenCPN (.pol)") },
  { id: "luckgrib", label: msg("LuckGrib (.tsv)") },
];

/** Why an export is refused, as a translated sentence naming what is wrong. */
export function problemText(problem: ExportProblemView, t: ReturnType<typeof useT>): string {
  const axis = (problem.axis ?? "").toUpperCase();
  switch (problem.code) {
    case "empty":
      return t("The blend has no boat speeds yet: there is nothing to export.");
    case "axis-collision":
      return t("{axis} values {first} and {second} would both be written as {written}, and the file would not read back. Change the grid.", {
        axis, first: problem.first ?? "", second: problem.second ?? "", written: problem.written ?? "",
      });
    case "out-of-range":
      return t("{axis} value {value} cannot be written in a polar file.", { axis, value: problem.first ?? "" });
    case "too-fast": {
      const [twa, tws, bsp] = problem.cell ?? [0, 0, 0];
      return t("The boat speed {bsp} kn at {twa}° and {tws} kn is faster than a polar file holds (60 kn).", { bsp, twa, tws });
    }
    default:
      return t("The polar cannot be exported.");
  }
}

/**
 * The export dialog (spec.md 12): the blend written as an Expedition,
 * Adrena or CSV polar, on the project's output grid or a custom set of
 * axes (the blend resampled onto them, never extrapolated), with a preview
 * of the grid before the native save dialog. What is written is
 * recomputed from the sources (invariant 2).
 *
 * Its controls are tagged and registered, landing on the Export button
 * (spec.md 3.6); Cancel and Save are not.
 */
export default function ExportDialog({ project, onClose }: {
  project: ProjectSummary;
  onClose: () => void;
}) {
  const api = useBoatApi();
  const t = useT();
  const [format, setFormat] = useState("whirlwind_matrix_polar");
  const [matrix, setMatrix] = useState(defaultMatrixExport);
  const matrixFormat = format === "whirlwind_matrix_polar";
  const [custom, setCustom] = useState(false);
  const [twaText, setTwaText] = useState(formatAxis(project.blend.twa));
  const [twsText, setTwsText] = useState(formatAxis(project.blend.tws));
  const [preview, setPreview] = useState<ExportPreview | null>(null);
  const [error, setError] = useState<unknown>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") { event.preventDefault(); if (!busy) onClose(); }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose, busy]);

  const twa = parseAxis(twaText, "twa", project.blend.asymmetric);
  const tws = parseAxis(twsText, "tws");
  const axes = useMemo(
    () => (custom && twa.values && tws.values ? { twa: twa.values, tws: tws.values } : null),
    // The parsed values are new arrays each render; their text is what moves.
    [custom, twaText, twsText],
  );
  const customInvalid = custom && axes === null;

  const windCells = (axes?.twa.length ?? project.blend.twa.length) * (axes?.tws.length ?? project.blend.tws.length);
  const matrixInvalid = matrixFormat && matrixExportError(matrix, windCells) !== null;

  useEffect(() => {
    if (matrixInvalid || customInvalid) { setPreview(null); return; }
    let live = true;
    setError(null);
    setPreview(null);
    const timer = window.setTimeout(() => { void api.exportPreview(format, axes, matrixFormat ? matrix : null)
      .then((next) => { if (live) setPreview(next); })
      .catch((failure) => { if (live) { setPreview(null); setError(failure); } });
    }, 200);
    return () => { live = false; window.clearTimeout(timer); };
  }, [format, axes, customInvalid, matrixInvalid, matrixFormat, matrix, project.revision]);

  const save = async () => {
    setBusy(true);
    try {
      const path = await pickExportPath(format, project.name);
      if (path === null) return;
      const written = await api.exportPolar(path, format, axes, matrixFormat ? matrix : null);
      setHint(later(msg("Exported the polar to {path}"), { path: written.path }));
      onClose();
    } catch (failure) {
      reportFailure(failure);
      setError(failure);
    } finally {
      setBusy(false);
    }
  };

  const title = t("Export the polar");
  const canSave = !busy && !matrixInvalid && !customInvalid && preview !== null && preview.problem === null;
  return (
    <div className="modal-backdrop" onClick={() => { if (!busy) onClose(); }}>
      <div className="modal settings export-dialog" role="dialog" aria-label={title} aria-busy={busy} onClick={(event) => event.stopPropagation()}>
        <h2>{title}</h2>
        <p className="modal-summary muted">{t("The blend of the visible sources, recomputed from them now.")}</p>

        <label className="export-format">
          <span>{t("Format")}</span>
          <select name="export-format" data-feature="export:format" value={format} disabled={busy}
            title={t("Write the polar in this format")} onChange={(event) => setFormat(event.target.value)}>
            {EXPORT_FORMATS.map((option) => (
              <option key={option.id} value={option.id}
                data-feature={`export:${option.id === "whirlwind_matrix_polar" ? "whirlwind-matrix-polar" : option.id}`}>{t(option.label)}</option>
            ))}
          </select>
        </label>

        {matrixFormat && <MatrixExportFields value={matrix} onChange={setMatrix} disabled={busy} windCells={windCells} />}

        <fieldset className="export-grid" disabled={busy}>
          <legend>{t("Grid")}</legend>
          <label>
            <input type="radio" name="export-grid" data-feature="export:project-grid" checked={!custom}
              title={t("Write the blend on the project's output grid (Blend settings)")} onChange={() => setCustom(false)} />
            {t("The project's output grid")}
          </label>
          <label>
            <input type="radio" name="export-grid" data-feature="export:custom-grid" checked={custom}
              title={t("Write the blend read onto other axes, without extrapolating")} onChange={() => setCustom(true)} />
            {t("Custom axes")}
          </label>
          {custom && <>
            <label className="settings-field axis-field">
              {t("TWA, degrees")}
              <textarea data-feature="export:custom-twa" rows={2} value={twaText}
                title={t("True wind angles, 0 to 180")} onChange={(event) => setTwaText(event.target.value)} />
            </label>
            {twa.error && <p className="modal-error" role="alert">{t(twa.error.key, twa.error.params)}</p>}
            <label className="settings-field axis-field">
              {t("TWS, knots")}
              <textarea data-feature="export:custom-tws" rows={2} value={twsText}
                title={t("True wind speeds, 0 to 70")} onChange={(event) => setTwsText(event.target.value)} />
            </label>
            {tws.error && <p className="modal-error" role="alert">{t(tws.error.key, tws.error.params)}</p>}
          </>}
        </fieldset>

        {error !== null && <p className="modal-error" role="alert" title={describeError(error).detail}>{describeError(error).text}</p>}
        {preview?.problem && <p className="modal-error" role="alert">{problemText(preview.problem, t)}</p>}
        {matrixFormat && <p className="muted">{t("Wind-grid preview; each wave condition is recomputed separately on export.")}</p>}
        {preview && <ExportTable preview={preview} matrixFormat={matrixFormat} />}

        <div className="modal-actions">
          <button disabled={busy} onClick={onClose} title={t("Close without exporting")}>{t("Cancel")}</button>
          <span className="spacer" />
          <button className="primary" disabled={!canSave} title={t("Choose where to save the file")}
            onClick={() => void save()}>
            {busy ? t("Exporting…") : t("Save…")}
          </button>
        </div>
      </div>
    </div>
  );
}

/** The grid that would be written: filled cells muted, empty ones blank. */
function ExportTable({ preview, matrixFormat }: { preview: ExportPreview; matrixFormat: boolean }) {
  const t = useT();
  return (
    <div className="export-preview" data-feature="export:preview"
      title={matrixFormat ? t("Wind-grid preview; each wave condition is recomputed separately on export.")
        : t("The grid as it would be written. Muted cells were filled between known values; blank cells stay empty.")}>
      <table>
        <thead>
          <tr>
            <th>{"TWA\\TWS"}</th>
            {preview.tws.map((tws) => <th key={tws}>{tws}</th>)}
          </tr>
        </thead>
        <tbody>
          {preview.twa.map((twa, i) => (
            <tr key={twa}>
              <th>{twa}</th>
              {preview.tws.map((tws, j) => {
                const value = preview.bsp[i]?.[j] ?? null;
                const filled = preview.origin?.[i]?.[j] === "filled";
                return <td key={tws} className={filled ? "filled" : undefined}>{value === null ? "" : value.toFixed(2)}</td>;
              })}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
