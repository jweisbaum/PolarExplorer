import { useBoatApi } from "../boats/context";
import { useEffect, useMemo, useRef, useState, type KeyboardEvent } from "react";

import { reportFailure } from "../errors";
import { later, reportError } from "../hint";
import type { EditOp } from "../generated/EditOp";
import type { EditSurface } from "../generated/EditSurface";
import type { PolarCell } from "../generated/PolarCell";
import type { ProjectSummary } from "../generated/ProjectSummary";
import type { SpeedUnit } from "../generated/SpeedUnit";
import { msg, useT } from "../i18n";

import { SPEED_FACTOR, SPEED_SYMBOL } from "./view3d";

/** A cell as the 3D view keys it: TWA index | TWS index << 16. */
export function cellCode(i: number, j: number): number {
  return i | (j << 16);
}

/** The segment statistics offered (spec.md 12.1), the default first. */
export const STATISTICS: readonly { id: string; label: string }[] = [
  { id: "min", label: msg("Minimum") },
  { id: "p90", label: msg("90th percentile") },
  { id: "p75", label: msg("75th percentile") },
  { id: "median", label: msg("Median") },
  { id: "mean", label: msg("Mean") },
  { id: "max", label: msg("Maximum") },
];

/** The fastest boat speed a cell takes, knots. */
export const MAX_BSP_KN = 60;

/** A speed stored in knots as the table shows it: two decimals in the display unit. */
export function showSpeed(knots: number, unit: SpeedUnit): string {
  return (knots * SPEED_FACTOR[unit]).toFixed(2);
}

/**
 * A value typed in the display unit, in knots (storage stays knots; the unit
 * is converted only here and in `showSpeed`, M17a); null for an empty box
 * (reset the cell); undefined for something that is not a speed from 0 to
 * 60 kn. The knots are not rounded, so the cell shows back exactly what was
 * typed at the table's two decimals.
 */
export function parseSpeed(text: string, unit: SpeedUnit = "kn"): number | null | undefined {
  const trimmed = text.trim().replace(",", ".");
  if (trimmed === "") return null;
  const value = Number(trimmed);
  // The limit as the table shows it (30.87 m/s), so the largest value it
  // names is accepted; it is stored as 60 kn.
  if (!Number.isFinite(value) || value < 0 || value > Number(showSpeed(MAX_BSP_KN, unit))) return undefined;
  const knots = unit === "kn" ? value : value / SPEED_FACTOR[unit];
  return Math.min(knots, MAX_BSP_KN);
}

/** A wind speed column's heading in the display unit: whole knots as they are, other units to 0.1. */
function showTws(knots: number, unit: SpeedUnit): string {
  if (unit === "kn") return String(knots);
  return String(Math.round(knots * SPEED_FACTOR[unit] * 10) / 10);
}

/**
 * Editing one source (spec.md 10.4), beside the 3D view: the table of its
 * editable surface (TWA rows × TWS columns, knots), where typing a value is
 * an edit and every edited cell is marked; the tools that act on the
 * selected cells — scale by a percentage, smooth, reset — and Reset all
 * edits; for a track, the statistic of its polar segment (spec.md 12.1).
 * Every action is one undoable change computed in Rust; the table refetches
 * the surface on every revision.
 */
export default function EditPanel({ project, sourceId, unit = "kn", selected, hideOthers, onHideOthers, onSelectCells, onProject, onDone }: {
  project: ProjectSummary;
  sourceId: number;
  /** The display speed unit (Settings); values are stored in knots whatever it is. */
  unit?: SpeedUnit;
  /** The selected cells (see `cellCode`). */
  selected: ReadonlySet<number>;
  hideOthers: boolean;
  onHideOthers: (hide: boolean) => void;
  /** Selects cells; `add` keeps the ones already selected. */
  onSelectCells: (codes: number[], add: boolean) => void;
  onProject: (summary: ProjectSummary) => void;
  onDone: () => void;
}) {
  const api = useBoatApi();
  const t = useT();
  const [surface, setSurface] = useState<EditSurface | null>(null);
  const [drafts, setDrafts] = useState<Record<number, string>>({});
  const [percent, setPercent] = useState("5");
  const request = useRef(0);
  const source = sourceId === 0 ? { label: t("Blend"), colour: project.blend.colour } : project.sources.find((s) => s.id === sourceId) ?? null;

  useEffect(() => {
    const id = ++request.current;
    void api.polarEditSurface(sourceId)
      .then((next) => { if (request.current === id) setSurface(next); })
      .catch((error: unknown) => { if (request.current === id) reportFailure(error); });
  }, [sourceId, project.id, project.revision]);

  const run = (change: Promise<ProjectSummary>) => change.then(onProject).catch(reportFailure);
  const cells: PolarCell[] = useMemo(
    () => [...selected].map((code) => ({ twa_index: code & 0xffff, tws_index: code >>> 16 })),
    [selected],
  );
  const apply = (op: EditOp) => void run(api.editPolar(sourceId, op, cells));
  const scale = Number(percent.replace(",", "."));
  const scaleValid = Number.isFinite(scale) && scale > -100 && scale <= 1000 && scale !== 0;

  const commit = (i: number, j: number) => {
    const code = cellCode(i, j);
    const text = drafts[code];
    if (text === undefined || !surface) return;
    setDrafts((current) => {
      const next = { ...current };
      delete next[code];
      return next;
    });
    const value = parseSpeed(text, unit);
    const held = surface.bsp[i]?.[j] ?? null;
    if (value === undefined) {
      reportError(later(msg("{text} is not a boat speed from 0 to {max} {unit}"), {
        text, max: Number(showSpeed(MAX_BSP_KN, unit)), unit: symbol,
      }), null);
      return;
    }
    // An empty box resets an edited cell; on a cell without an edit it changes nothing.
    if (value === null && !surface.edited[i]?.[j]) return;
    // What the cell already shows is no edit, whatever the knots behind it.
    if (value !== null && held !== null && showSpeed(value, unit) === showSpeed(held, unit)) return;
    void run(api.editPolar(sourceId, { type: "type", bsp: value }, [{ twa_index: i, tws_index: j }]));
  };

  const onKey = (event: KeyboardEvent<HTMLInputElement>, i: number, j: number) => {
    if (event.key === "Enter") {
      event.currentTarget.blur();
    } else if (event.key === "Escape") {
      event.stopPropagation();
      setDrafts((current) => {
        const next = { ...current };
        delete next[cellCode(i, j)];
        return next;
      });
      event.currentTarget.blur();
    }
  };

  const symbol = SPEED_SYMBOL[unit];
  const isTrack = surface?.kind === "track";
  const cellTip = (i: number, j: number): string => {
    if (!surface) return "";
    const imported = surface.source[i]?.[j];
    const base = imported === null || imported === undefined
      ? (isTrack ? t("No segment value here") : t("No value in the source here"))
      : (isTrack
        ? t("Segment value {bsp} {unit}", { bsp: showSpeed(imported, unit), unit: symbol })
        : t("Source value {bsp} {unit}", { bsp: showSpeed(imported, unit), unit: symbol }));
    const parts = [base];
    if (isTrack && surface.count) {
      const count = surface.count[i]?.[j] ?? 0;
      const spread = surface.spread?.[i]?.[j];
      parts.push(spread === null || spread === undefined
        ? t("{count} samples", { count })
        : t("{count} samples, spread ±{spread} {unit}", { count, spread: showSpeed(spread, unit), unit: symbol }));
    }
    if (surface.edited[i]?.[j]) parts.push(t("Edited"));
    if (surface.excluded[i]?.[j]) parts.push(t("Excluded from the blend"));
    return parts.join(" · ");
  };

  return (
    <section className="view3d-edit" aria-label={t("Edit polar")}>
      <header className="view3d-edit-head">
        <span className="swatch" style={{ background: source?.colour }} />
        <strong>{t("Editing {label}", { label: source?.label ?? "" })}</strong>
        <span className="spacer" />
        <button className="small" data-feature="edit:done" title={t("Leave edit mode (the edits stay)")} onClick={onDone}>
          {t("Done")}
        </button>
      </header>
      <div className="view3d-edit-row">
        {sourceId !== 0 && <label title={t("Hide the other sources instead of fading them")}>
          <input type="checkbox" data-feature="edit:hide-others" checked={hideOthers}
            onChange={(event) => onHideOthers(event.target.checked)} />
          {t("Hide other sources")}
        </label>}
        {isTrack && (
          <label className="view3d-edit-statistic">
            {t("Statistic")}
            <select data-feature="edit:statistic" value={surface?.statistic ?? "p90"}
              title={t("How a cell of the track's polar segment sums up its samples' boat speeds (undoable)")}
              onChange={(event) => void run(api.setSegmentStatistic(sourceId, event.target.value))}>
              {STATISTICS.map((s) => <option key={s.id} value={s.id}>{t(s.label)}</option>)}
            </select>
          </label>
        )}
      </div>
      {isTrack && surface && (
        <p className="muted view3d-edit-note">
          {t("A cell needs {count} samples to have a value.", { count: surface.min_samples })}
        </p>
      )}
      <div className="view3d-edit-row">
        <input className="view3d-edit-percent" data-feature="edit:scale-percent" value={percent} inputMode="decimal"
          aria-label={t("Scale by (%)")} title={t("The percentage Scale applies: 5 is 5 % faster, -5 is 5 % slower")}
          onChange={(event) => setPercent(event.target.value)} />
        <span>%</span>
        <button className="small" data-feature="edit:scale" disabled={cells.length === 0 || !scaleValid}
          title={t("Scale the selected cells by the percentage (undoable)")}
          onClick={() => apply({ type: "scale", percent: scale })}>
          {t("Scale")}
        </button>
        <button className="small" data-feature="edit:smooth" disabled={cells.length === 0}
          title={t("Smooth the selected cells over their neighbours on the grid (undoable)")}
          onClick={() => apply({ type: "smooth" })}>
          {t("Smooth")}
        </button>
        <button className="small" data-feature="edit:reset" disabled={cells.length === 0}
          title={t("Put the selected cells back to the source's values (undoable)")}
          onClick={() => apply({ type: "reset" })}>
          {t("Reset")}
        </button>
        <button className="small" data-feature="edit:reset-all" disabled={(surface?.edit_count ?? 0) === 0}
          title={t("Clear every edit of this source (undoable)")}
          onClick={() => void run(api.editPolar(sourceId, { type: "reset_all" }, []))}>
          {t("Reset all edits")}
        </button>
      </div>
      <p className="muted view3d-edit-note">
        {cells.length === 0
          ? t("Select cells in the table or nodes in the view; drag a node with the Drag tool.")
          : t("{count} cells selected", { count: cells.length })}
        {" · "}
        {t("{count} edits", { count: surface?.edit_count ?? 0 })}
      </p>
      {surface && (
        <div className="view3d-edit-table-wrap" data-feature="edit:table">
          <table className="view3d-edit-table">
            <thead>
              <tr>
                <th title={t("TWA (°) down, TWS ({unit}) across; boat speeds in {unit}", { unit: symbol })}>TWA \ TWS</th>
                {surface.tws.map((tws) => <th key={tws}>{showTws(tws, unit)}</th>)}
              </tr>
            </thead>
            <tbody>
              {surface.twa.map((twa, i) => (
                <tr key={twa}>
                  <th>{twa}°</th>
                  {surface.tws.map((tws, j) => {
                    const code = cellCode(i, j);
                    const value = surface.bsp[i]?.[j] ?? null;
                    const classes = ["view3d-edit-cell"];
                    if (surface.edited[i]?.[j]) classes.push("edited");
                    if (surface.excluded[i]?.[j]) classes.push("excluded");
                    if (selected.has(code)) classes.push("selected");
                    return (
                      <td key={tws} className={classes.join(" ")}>
                        <input value={drafts[code] ?? (value === null ? "" : showSpeed(value, unit))} placeholder="–"
                          inputMode="decimal" title={cellTip(i, j)}
                          aria-label={t("BSP at {twa}° and {tws} {unit}", { twa, tws: showTws(tws, unit), unit: symbol })}
                          onFocus={(event) => { if (!selected.has(code) || selected.size > 1) onSelectCells([code], false); event.currentTarget.select(); }}
                          onMouseDown={(event) => { if (event.shiftKey) { event.preventDefault(); onSelectCells([code], true); } }}
                          onChange={(event) => setDrafts((current) => ({ ...current, [code]: event.target.value }))}
                          onBlur={() => commit(i, j)}
                          onKeyDown={(event) => onKey(event, i, j)} />
                      </td>
                    );
                  })}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </section>
  );
}
