import { useBoatApi } from "../boats/context";
import { useEffect, useRef, useState } from "react";

import { reportFailure } from "../errors";
import type { CsvMappingInput } from "../generated/CsvMappingInput";
import type { TrackFileInspection } from "../generated/TrackFileInspection";
import type { TrackFileRequest } from "../generated/TrackFileRequest";
import type { TrackImportResult } from "../generated/TrackImportResult";
import { useT } from "../i18n";

import {
  assign, CSV_ROLE_NAMES, CSV_ROLES, dateRange, describeTrackFailure, REQUIRED_ROLES, roleOf, SPEED_UNITS,
  TIME_FORMATS, type CsvRole,
} from "./trackImport";

/** A re-read of a CSV waits this long after the last change to its mapping. */
const RECHECK_MS = 300;

/** One file in the dialog: what Rust found, and the boats the person keeps. */
interface Entry {
  inspection: TrackFileInspection;
  /** Boat names chosen (empty string: the unnamed boat). */
  boats: Set<string>;
}

function entryOf(inspection: TrackFileInspection, previous?: Entry): Entry {
  const names = inspection.boats.map((b) => b.name);
  // Keep what was ticked when a re-read finds the same boats; tick all new ones.
  const boats = previous
    ? new Set(names.filter((n) => previous.boats.has(n) || !previous.inspection.boats.some((b) => b.name === n)))
    : new Set(names);
  return { inspection, boats };
}

/** Whether a file will import: it read, and at least one boat is ticked. */
function ready(entry: Entry): boolean {
  return entry.inspection.failure === null && entry.inspection.boats.length > 0 && entry.boats.size > 0;
}

/**
 * The track import dialog (spec.md 7.3): for each chosen file, the boats it
 * holds with a picker when there are several, and for a CSV the
 * column-mapping step — a preview of the first rows with the guessed time,
 * latitude, longitude, heading, speed and boat columns, the time format and
 * the speed unit, each correctable, the file re-read as they change. Import
 * adds one track per ticked boat as one undoable change.
 *
 * Every configuration control carries a `data-feature` and is registered
 * (`help/features/tracks.ts`) like any other. The dialog exists only once
 * files are chosen, so those entries land on File… (`tracks:import-file`),
 * and their descriptions say a file must be chosen first. Only its answer
 * buttons (Cancel, Import) go untagged, as in every transient dialog
 * (spec.md 3.6).
 */
export default function TrackImportDialog({ inspections, onDone, onCancel }: {
  inspections: TrackFileInspection[];
  onDone: (result: TrackImportResult) => void;
  onCancel: () => void;
}) {
  const api = useBoatApi();
  const t = useT();
  const [entries, setEntries] = useState<Entry[]>(() => inspections.map((i) => entryOf(i)));
  const [busy, setBusy] = useState(false);
  const timers = useRef(new Map<string, number>());
  const requests = useRef(new Map<string, number>());

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") { event.preventDefault(); onCancel(); }
    };
    window.addEventListener("keydown", onKey);
    const pending = timers.current;
    return () => {
      window.removeEventListener("keydown", onKey);
      for (const timer of pending.values()) window.clearTimeout(timer);
    };
  }, [onCancel]);

  const update = (path: string, change: (entry: Entry) => Entry) =>
    setEntries((current) => current.map((e) => (e.inspection.path === path ? change(e) : e)));

  /** Changes a CSV's mapping at once on screen, and re-reads the file with it shortly after. */
  const remap = (path: string, mapping: CsvMappingInput) => {
    update(path, (e) => e.inspection.csv
      ? { ...e, inspection: { ...e.inspection, csv: { ...e.inspection.csv, mapping } } }
      : e);
    window.clearTimeout(timers.current.get(path));
    timers.current.set(path, window.setTimeout(() => {
      const id = (requests.current.get(path) ?? 0) + 1;
      requests.current.set(path, id);
      api.inspectCsvTrack(path, mapping)
        .then((inspection) => {
          if (requests.current.get(path) === id) update(path, (e) => entryOf(inspection, e));
        })
        .catch(reportFailure);
    }, RECHECK_MS));
  };

  const importable = entries.filter(ready);

  const run = async () => {
    setBusy(true);
    try {
      const files: TrackFileRequest[] = importable.map((e) => ({
        path: e.inspection.path,
        mapping: e.inspection.csv?.mapping ?? null,
        boats: [...e.boats],
      }));
      onDone(await api.importTrackFiles(files));
    } catch (error) {
      reportFailure(error);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="modal-backdrop" onClick={onCancel}>
      <div className="modal track-import" role="dialog" aria-label={t("Import tracks")} onClick={(e) => e.stopPropagation()}>
        <h2>{t("Import tracks")}</h2>
        <p className="modal-summary">
          {t("Each ticked boat becomes one track. Times are read as UTC. Files that cannot be read are left out.")}
        </p>
        <div className="track-import-files">
          {entries.map((entry) => (
            <FileCard key={entry.inspection.path} entry={entry}
              onMapping={(mapping) => remap(entry.inspection.path, mapping)}
              onBoats={(boats) => update(entry.inspection.path, (e) => ({ ...e, boats }))} />
          ))}
        </div>
        <div className="modal-actions">
          <button onClick={onCancel} title={t("Import nothing")}>{t("Cancel")}</button>
          <span className="spacer" />
          <button className="primary" disabled={busy || importable.length === 0}
            title={t("Add one track per ticked boat (one undo takes them all back out)")} onClick={() => void run()}>
            {importable.length === entries.length ? t("Import") : t("Import {count} of {total} files", { count: importable.length, total: entries.length })}
          </button>
        </div>
      </div>
    </div>
  );
}

function FileCard({ entry, onMapping, onBoats }: {
  entry: Entry;
  onMapping: (mapping: CsvMappingInput) => void;
  onBoats: (boats: Set<string>) => void;
}) {
  const t = useT();
  const { inspection } = entry;
  const csv = inspection.csv;
  const unnamed = t("(unnamed)");
  return (
    <section className="track-import-file">
      <h3>
        {inspection.file} <span className="muted">{inspection.kind === "csv" ? "CSV" : inspection.kind === "gpx" ? "GPX" : "GeoJSON"}</span>
      </h3>
      {csv && (
        <>
          <div className="track-import-mapping">
            {CSV_ROLES.map((role) => (
              <label key={role}>
                {t(CSV_ROLE_NAMES[role])}
                <select data-feature={`track-import:${role}`} value={csv.mapping[role] ?? ""}
                  title={REQUIRED_ROLES.includes(role) ? t("The column this is read from (required)") : t("The column this is read from (optional)")}
                  onChange={(e) => onMapping(assign(csv.mapping, role as CsvRole, e.target.value === "" ? null : Number(e.target.value)))}>
                  <option value="">{t("— none —")}</option>
                  {csv.header.map((name, column) => <option key={column} value={column}>{name || t("column {n}", { n: column + 1 })}</option>)}
                </select>
              </label>
            ))}
            <label>
              {t("Time format")}
              <select data-feature="track-import:time-format" value={csv.mapping.time_format}
                title={t("How the time column is written; times without a zone are UTC")}
                onChange={(e) => onMapping({ ...csv.mapping, time_format: e.target.value })}>
                {TIME_FORMATS.map((f) => <option key={f.id} value={f.id}>{t(f.label)}</option>)}
              </select>
            </label>
            {csv.mapping.time_format === "custom" && (
              <label>
                {t("Format")}
                <input data-feature="track-import:custom-format" value={csv.mapping.custom_format}
                  placeholder="%d/%m/%Y %H:%M:%S"
                  title={t("%Y year, %m month, %d day, %H hour, %M minute, %S second, %f fraction, %b month name, %z zone")}
                  onChange={(e) => onMapping({ ...csv.mapping, custom_format: e.target.value })} />
              </label>
            )}
            <label>
              {t("Speed unit")}
              <select data-feature="track-import:speed-unit" value={csv.mapping.speed_unit}
                title={t("The unit of the speed column; stored in knots")}
                onChange={(e) => onMapping({ ...csv.mapping, speed_unit: e.target.value })}>
                {SPEED_UNITS.map((u) => <option key={u.id} value={u.id}>{t(u.label)}</option>)}
              </select>
            </label>
            <label>
              {t("Wind speed unit")}
              <select data-feature="track-import:wind-speed-unit" value={csv.mapping.wind_speed_unit}
                onChange={(e) => onMapping({ ...csv.mapping, wind_speed_unit: e.target.value })}>
                {SPEED_UNITS.map((u) => <option key={u.id} value={u.id}>{t(u.label)}</option>)}
              </select>
            </label>
          </div>
          <div className="track-import-preview">
            <table>
              <thead>
                <tr>
                  {csv.header.map((name, column) => {
                    const role = roleOf(csv.mapping, column);
                    return (
                      <th key={column} className={role ? "mapped" : undefined}>
                        {name}
                        {role && <span className="track-import-role">{t(CSV_ROLE_NAMES[role])}</span>}
                      </th>
                    );
                  })}
                </tr>
              </thead>
              <tbody>
                {csv.rows.map((row, r) => (
                  <tr key={r}>{row.map((cell, c) => <td key={c}>{cell}</td>)}</tr>
                ))}
              </tbody>
            </table>
            <p className="muted">{t("First {shown} of {count} rows", { shown: csv.rows.length, count: csv.row_count })}</p>
          </div>
        </>
      )}
      {inspection.failure && (
        <p className="import-failures" role="alert" title={inspection.failure.message}>{describeTrackFailure(inspection.failure)}</p>
      )}
      {inspection.boats.length > 1 && (
        <fieldset className="track-import-boats" data-feature="track-import:boats">
          <legend>{t("Boats in this file")}</legend>
          {inspection.boats.map((boat) => (
            <label key={boat.name}>
              <input type="checkbox" checked={entry.boats.has(boat.name)} onChange={(e) => {
                const next = new Set(entry.boats);
                if (e.target.checked) next.add(boat.name); else next.delete(boat.name);
                onBoats(next);
              }} />
              {boat.name || unnamed}
              <span className="muted">{t("{count} positions, {dates}", { count: boat.fixes, dates: dateRange(boat.start, boat.end) })}</span>
            </label>
          ))}
        </fieldset>
      )}
      {inspection.boats.length === 1 && (
        <p className="muted">{t("{count} positions, {dates}", { count: inspection.boats[0]!.fixes, dates: dateRange(inspection.boats[0]!.start, inspection.boats[0]!.end) })}</p>
      )}
    </section>
  );
}
