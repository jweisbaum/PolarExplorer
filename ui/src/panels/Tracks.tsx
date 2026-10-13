import { useBoatApi } from "../boats/context";
import { useCallback, useEffect, useRef, useState } from "react";

import { useLiveEdit } from "./useLiveEdit";
import { reportFailure } from "../errors";
import type { ProjectSummary } from "../generated/ProjectSummary";
import type { SourceSummary } from "../generated/SourceSummary";
import type { TrackFileInspection } from "../generated/TrackFileInspection";
import type { TrackFilters } from "../generated/TrackFilters";
import type { TrackImportFailure } from "../generated/TrackImportFailure";
import type { TrackImportLine } from "../generated/TrackImportLine";
import type { TrackSummary } from "../generated/TrackSummary";
import { useBoatReveal } from "../boats/context";
import type { Units } from "../generated/Units";
import { DEFAULT_UNITS, shownValue, speedUnit, storedValue, waveUnit } from "./filterUnits";
import { later, setHint } from "../hint";
import { msg, useT } from "../i18n";

import { envJobOf, useEnvJobs } from "../jobs";
import { pickTrackFiles } from "../project/dialogs";
import { useBoatSelection } from "../selection";
import BoatTrackSearch from "./BoatTrackSearch";
import TrackImportDialog from "./TrackImportDialog";
import TrackerImportDialog from "./TrackerImportDialog";
import type { TrackerId } from "./trackerImport";
import type { TrackImportResult } from "../generated/TrackImportResult";
import { dateRange, describeImportLine, describeTrackFailure, envStatusText, fromLocalInput, toLocalInput } from "./trackImport";

/**
 * The Tracks section of the left navigation (spec.md 7.1): the tracker
 * buttons (YellowBrick, Geovoile and Blue Water Tracks all open the shared
 * tracker dialog, spec.md 7.2), File… for GPX, GeoJSON and CSV, the result of
 * the last import, and every track with its colour, boat, event, dates,
 * samples used, weather status and actions. Each track unfolds to its
 * sample filters and heading and speed derivation (spec.md 7.4, 7.6),
 * every change one undo.
 *
 * Importing never fetches weather (D24): a track's Fetch weather…, or
 * Fetch weather for selected tracks… over the ticked tracks, starts the
 * background fetch immediately.
 */
export default function Tracks({ project, onProject, units = DEFAULT_UNITS, metadataDirectory = "" }: {
  project: ProjectSummary;
  onProject: (project: ProjectSummary) => void;
  /** The display units (Settings): the filters are shown and typed in them. */
  units?: Units;
  metadataDirectory?: string;
}) {
  const api = useBoatApi();
  const onReveal = useBoatReveal();
  const t = useT();
  const [inspections, setInspections] = useState<TrackFileInspection[] | null>(null);
  const [tracker, setTracker] = useState<TrackerId | null>(null);
  const [imported, setImported] = useState<TrackImportLine[]>([]);
  const [failures, setFailures] = useState<TrackImportFailure[]>([]);
  const [open, setOpen] = useState<number | null>(null);
  // The tracks ticked for Fetch weather for selected tracks…
  const [selected, setSelected] = useState<ReadonlySet<number>>(new Set());
  const starting = useRef(false);
  const [isStarting, setIsStarting] = useState(false);
  const tracks = project.sources.filter((s) => s.track !== null);
  // A ticked track already queued or fetching is left out: asking again
  // would only queue it twice (M17a).
  const jobs = useEnvJobs();
  const selectedIds = tracks.filter((s) => selected.has(s.id) && s.track!.env_status !== "ready" && envJobOf(jobs, s.id) === undefined).map((s) => s.id);
  const clearSelection = useCallback(() => setSelected(new Set()), []);
  const fetchWeather = async (ids: number[], restart: boolean, fromSelection = false) => {
    if (starting.current || ids.length === 0) return;
    starting.current = true;
    setIsStarting(true);
    try {
      await api.startEnvFetch(ids, restart);
      if (fromSelection) clearSelection();
    } catch (error) {
      reportFailure(error);
    } finally {
      starting.current = false;
      setIsStarting(false);
    }
  };
  const select = (id: number, on: boolean) => setSelected((old) => {
    const next = new Set(old);
    if (on) next.add(id); else next.delete(id);
    return next;
  });
  // The search's `track:details` step unfolds the first track's filters.
  const first = useRef<number | null>(null);
  first.current = tracks[0]?.id ?? null;
  useEffect(() => onReveal("track:details", () => { if (first.current !== null) setOpen(first.current); }), []);

  const chooseFiles = async () => {
    try {
      const paths = await pickTrackFiles();
      if (paths.length === 0) return;
      setInspections(await api.inspectTrackFiles(paths));
    } catch (error) {
      reportFailure(error);
    }
  };

  /**
   * After any import, file or tracker: the summary and a hint. No weather
   * is fetched (D24); the user asks for it per track.
   */
  const afterImport = (result: TrackImportResult) => {
    onProject(result.project);
    setImported(result.imported);
    setFailures(result.failures);
    const count = result.imported.length;
    if (count > 0) {
      setHint(count === 1
        ? later(msg("Imported 1 track. Fetch its weather from the track list when you want it."))
        : later(msg("Imported {count} tracks. Fetch their weather from the track list when you want it."), { count }));
    }
  };

  const remove = (id: number) => { api.removeSource(id).then(onProject).catch(reportFailure); };

  return (
    <>
      <div className="section-actions">
        <button data-feature="tracks:yellowbrick" onClick={() => setTracker("yellowbrick")}
          title={t("Import boats from a YellowBrick race: paste its link or race key")}>
          {t("YellowBrick…")}
        </button>
        <button data-feature="tracks:geovoile" onClick={() => setTracker("geovoile")}
          title={t("Import boats from a Geovoile race: paste its viewer address")}>
          {t("Geovoile…")}
        </button>
        <button data-feature="tracks:bluewater" onClick={() => setTracker("bluewater")}
          title={t("Import boats from a Blue Water Tracks race: paste its race link")}>
          {t("Blue Water…")}
        </button>
        <button data-feature="tracks:import-file" onClick={() => void chooseFiles()}
          title={t("Import GPX, GeoJSON and CSV tracks; several files at once")}>
          {t("File…")}
        </button>
      </div>
      <BoatTrackSearch key={project.id} onImport={afterImport} metadataDirectory={metadataDirectory} />
      {tracks.length > 0 && (
        <div className="section-actions">
          <button data-feature="tracks:select-all" disabled={tracks.every(source => source.track!.env_status === "ready" || selected.has(source.id))}
            title={t("Select all imported tracks for weather download")}
            onClick={() => setSelected(new Set(tracks.filter(source => source.track!.env_status !== "ready").map(source => source.id)))}>
            {t("Select all")}
          </button>
          <button data-feature="tracks:fetch-weather-selected" disabled={isStarting || selectedIds.length === 0}
            title={selectedIds.length === 0
              ? t("Tick tracks in the list first, then fetch their wind, waves and current together")
              : t("Fetch the wind, waves and current the ticked tracks' samples do not have yet")}
            onClick={() => void fetchWeather(selectedIds, false, true)}>
            {selectedIds.length === 0
              ? t("Fetch weather for selected tracks…")
              : t("Fetch weather for {count} selected tracks…", { count: selectedIds.length })}
          </button>
        </div>
      )}
      {tracks.length > 0 && <EnvOptions project={project} onProject={onProject} />}
      {imported.length > 0 && (
        <ul className="track-import-summary" aria-label={t("Last import")}>
          {imported.map((line, k) => <li key={k}>{describeImportLine(line)}</li>)}
        </ul>
      )}
      {failures.length > 0 && (
        <ul className="import-failures" role="alert" aria-label={t("Files or boats that were not imported")}>
          {failures.map((failure, k) => <li key={k} title={failure.message}>{describeTrackFailure(failure)}</li>)}
        </ul>
      )}
      {tracks.length === 0
        ? <p className="muted placeholder">{t("No tracks in this project yet.")}</p>
        : <ul className="track-list">
          {tracks.map((source) => (
            <TrackItem key={source.id} source={source} track={source.track!} open={open === source.id}
              selected={selected.has(source.id)} onSelect={(on) => select(source.id, on)}
              onToggle={() => setOpen(open === source.id ? null : source.id)}
              onRemove={() => remove(source.id)} onProject={onProject} units={units}
              starting={isStarting}
              onRefetch={() => void fetchWeather([source.id], source.track!.env_status === "ready")}
              />
          ))}
        </ul>}
      {inspections !== null && (
        <TrackImportDialog inspections={inspections} onCancel={() => setInspections(null)}
          onDone={(result) => {
            setInspections(null);
            afterImport(result);
          }} />
      )}
      {tracker !== null && (
        <TrackerImportDialog tracker={tracker} onCancel={() => setTracker(null)}
          onDone={(result) => {
            setTracker(null);
            afterImport(result);
          }} />
      )}
    </>
  );
}

/**
 * The project's current setting (spec.md 7.5): whether the polar is fed
 * from water-relative values where a current was found. One undo.
 */
function EnvOptions({ project, onProject }: { project: ProjectSummary; onProject: (project: ProjectSummary) => void }) {
  const api = useBoatApi();
  const t = useT();
  return (
    <div className="track-env-options">
      <label title={t("Take the current out of boat speed and wind where a current was found, so the polar is through the water")}>
        <input type="checkbox" data-feature="tracks:use-corrected" checked={project.use_corrected}
          onChange={(e) => { api.setUseCorrected(e.target.checked).then(onProject).catch(reportFailure); }} />
        {t("Correct for current")}
      </label>
    </div>
  );
}

function TrackItem({ source, track, open, selected, starting, onSelect, onToggle, onRemove, onProject, onRefetch, units }: {
  source: SourceSummary;
  units: Units;
  track: TrackSummary;
  open: boolean;
  selected: boolean;
  starting: boolean;
  onSelect: (on: boolean) => void;
  onToggle: () => void;
  onRemove: () => void;
  onProject: (project: ProjectSummary) => void;
  onRefetch: () => void;
}) {
  const { focusMap } = useBoatSelection();
  const api = useBoatApi();
  const t = useT();
  const job = envJobOf(useEnvJobs(), source.id);
  return (
    <li className="track-item">
      <div className="track-row">
        <input type="checkbox" data-feature="tracks:select" checked={selected}
          title={t("Tick to fetch this track's weather with the other ticked tracks")}
          aria-label={t("Select {track}", { track: source.label })} onChange={(e) => onSelect(e.target.checked)} />
        <span className="swatch" style={{ backgroundColor: source.colour }} aria-hidden="true" />
        <span className="polar-file-text">
          <span className="polar-file-label" title={track.event_title}>{source.label}</span>
          <span className="muted polar-file-meta">
            {track.event_title} · {dateRange(track.start, track.end)}
          </span>
          <span className="muted polar-file-meta">
            {t("{used} of {count} samples used", { used: source.used ?? 0, count: track.samples })}
          </span>
          <span className="muted polar-file-meta track-weather-status">
            {t("Weather: {status}", { status: envStatusText(track, job) })}
          </span>
        </span>
        {job
          ? <button className="small" data-feature="tracks:cancel-fetch" title={t("Stop this track's fetch; samples already fetched are kept")}
            onClick={() => { api.cancelEnvFetch([source.id]).catch(reportFailure); }}>{t("Cancel fetch")}</button>
          : <button className="small" data-feature="tracks:fetch-weather" onClick={onRefetch} disabled={starting}
            title={track.env_status === "ready"
              ? t("Fetch this track's wind, waves and current again, every sample")
              : t("Fetch the wind, waves and current this track's samples do not have yet")}>
            {t("Fetch weather…")}
          </button>}
        <button className="icon-button" data-feature="tracks:show-on-map" disabled={!source.visible}
          title={source.visible ? t("Show this track on the map") : t("Hidden tracks are not on the map")}
          aria-label={t("Show on map")} onClick={() => focusMap({ kind: "track", sourceId: source.id })}>
          ⌖
        </button>
        <button className="icon-button" data-feature="tracks:filters" aria-expanded={open}
          title={t("Sample filters, and which heading, speed and wind the track uses")} aria-label={t("Filters")} onClick={onToggle}>
          {open ? "▾" : "▸"}
        </button>
        <button className="icon-button" data-feature="tracks:remove" onClick={onRemove}
          title={t("Remove this track from the project (undoable)")} aria-label={t("Remove")}>
          ✕
        </button>
      </div>
      {open && (
        <div className="track-details">
          <TrackFiltersEditor id={source.id} track={track} onProject={onProject} units={units} />
        </div>
      )}
    </li>
  );
}

/** The wave sectors off the bow (spec.md 7.6). */
const WAVE_SECTORS: [string, string][] = [
  ["head", msg("Head (under 30°)")],
  ["bow", msg("Bow (30–60°)")],
  ["beam", msg("Beam (60–120°)")],
  ["quarter", msg("Quarter (120–150°)")],
  ["following", msg("Following (from 150°)")],
];

/**
 * A number box that applies valid edits after a short typing pause. Blur flushes. `value` is stored (knots,
 * metres, degrees); `factor` converts it to what the box shows and back, so
 * a speed or a wave height is typed in the display unit. Empty means no bound.
 */
export function NumberField({ feature, label, title, value, min, max, step, factor = 1, onCommit }: {
  feature: string; label: string; title: string; value: number | null;
  min: number; max: number; step: number; factor?: number; onCommit: (value: number | null) => void;
}) {
  const [text, setText] = useState(shownValue(value, factor));
  const [shown, setShown] = useState<[number | null, number]>([value, factor]);
  if (shown[0] !== value || shown[1] !== factor) {
    setShown([value, factor]);
    setText(shownValue(value, factor));
  }
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const callback = useRef(onCommit);
  callback.current = onCommit;
  useEffect(() => () => { if (timer.current) clearTimeout(timer.current); }, []);
  const commit = () => {
    if (timer.current) clearTimeout(timer.current);
    const next = storedValue(text, value, factor);
    if (next === undefined) { setText(shownValue(value, factor)); return; }
    if (next !== value) onCommit(next);
  };
  return (
    <label className="track-field">
      {label}
      <input type="number" data-feature={feature} value={text} min={min} max={Number((max * factor).toFixed(3))} step={step} title={title}
        onChange={(e) => {
          const input = e.target.value;
          setText(input);
          if (timer.current) clearTimeout(timer.current);
          const next = storedValue(input, value, factor);
          if (!e.target.validity.badInput && next !== undefined && next !== value &&
              (next === null || (next >= min / factor && next <= max))) {
            timer.current = setTimeout(() => callback.current(next), 120);
          }
        }} onBlur={commit}
        onKeyDown={(e) => { if (e.key === "Enter") e.currentTarget.blur(); }} />
    </label>
  );
}

/**
 * A UTC date and time that applies when complete. An unreadable entry
 * goes back to the stored value on blur.
 */
function TimeField({ feature, label, title, value, onCommit }: {
  feature: string; label: string; title: string; value: number | null; onCommit: (value: number | null) => void;
}) {
  const [text, setText] = useState(toLocalInput(value));
  const [shown, setShown] = useState(value);
  if (shown !== value) {
    setShown(value);
    setText(toLocalInput(value));
  }
  const commit = () => {
    const next = fromLocalInput(text);
    if (next === null && text.trim() !== "") { setText(toLocalInput(value)); return; }
    if (next !== value) onCommit(next);
  };
  return (
    <label className="track-field track-time-field">
      {label}
      <input type="datetime-local" step={1} data-feature={feature} value={text} title={title}
        onChange={(e) => {
          const text = e.target.value;
          setText(text);
          const next = fromLocalInput(text);
          if (next !== value && (next !== null || text === "")) onCommit(next);
        }} onBlur={commit}
        onKeyDown={(e) => { if (e.key === "Enter") e.currentTarget.blur(); }} />
    </label>
  );
}

function TrackFiltersEditor({ id, track, onProject, units }: {
  id: number; track: TrackSummary; onProject: (project: ProjectSummary) => void; units: Units;
}) {
  const api = useBoatApi();
  const prefer = (heading: Prefer, speed: Prefer) => {
    api.setTrackDerivation(id, track.max_gap_s, heading, speed).then(onProject).catch(reportFailure);
  };
  // A quantity's choice between what the track gave and what is derived is
  // offered only where the track gave it (asked 2026-10-02); otherwise it
  // is derived without a word.
  const sources: ValueSources = {
    heading: track.supplied_heading > 0 ? { value: track.prefer_heading as Prefer, choose: (v) => prefer(v, track.prefer_speed as Prefer) } : null,
    speed: track.supplied_speed > 0 ? { value: track.prefer_speed as Prefer, choose: (v) => prefer(track.prefer_heading as Prefer, v) } : null,
    wind: track.supplied_wind > 0 ? {
      value: track.downloaded_wind_only ? "derived" : "given",
      choose: (v) => { void api.setTrackWind(id, v === "derived").then(onProject).catch(reportFailure); },
    } : null,
  };
  return <SampleFiltersEditor filters={track.filters} track={track} units={units} sources={sources}
    onChange={(filters) => api.setTrackFilters(id, filters).then(onProject)} />;
}

type Prefer = "given" | "derived";

/** For each quantity a track may give: what it uses now, and how to choose; null where the track gives none. */
export interface ValueSources {
  heading: { value: Prefer; choose: (value: Prefer) => void } | null;
  speed: { value: Prefer; choose: (value: Prefer) => void } | null;
  wind: { value: Prefer; choose: (value: Prefer) => void } | null;
}

/** The provided-or-derived choice of one quantity, beside its bounds. */
function SourceChoice({ feature, label, choice, derived }: {
  feature: string; label: string; choice: NonNullable<ValueSources[keyof ValueSources]>; derived: string;
}) {
  const t = useT();
  return (
    <label className="track-field track-choice-field">
      {label}
      <select data-feature={feature} value={choice.value} onChange={(e) => choice.choose(e.target.value as Prefer)}>
        <option value="given">{t("Provided by the track")}</option>
        <option value="derived">{derived}</option>
      </select>
    </label>
  );
}

/** Stable feature families shared by individual, global and priority filters. */
function filterFeature(prefix: string, name: string): string { return `${prefix}:${name}`; }

export function SampleFiltersEditor({ filters, onChange, units, track, prefix = "tracks", sources = null }: {
  filters: TrackFilters; onChange: (filters: TrackFilters) => Promise<unknown>; units: Units;
  track?: TrackSummary; prefix?: "tracks" | "global-filters" | "priority-filters";
  /** The track's provided-or-derived choices, for its own filters only. */
  sources?: ValueSources | null;
}) {
  const t = useT();
  const speed = speedUnit(units);
  const wave = waveUnit(units);
  const [f, update] = useLiveEdit(filters, onChange);
  const set = (change: Partial<TrackFilters>) => { void update((previous) => ({ ...previous, ...change })); };
  const needsWind = t("These filters read the environment, which this track does not have yet; with one set, samples without it are left out.");
  /**
   * A compass sector's two bounds. A sector needs both, so a bound typed
   * alone is held here, not sent (Rust refuses half a sector), until its
   * partner is typed; clearing either clears both.
   */
  const [lone, setLone] = useState<Partial<Record<keyof TrackFilters, number>>>({});
  const compass = (from: keyof TrackFilters, to: keyof TrackFilters, features: [string, string], fromLabel: string, title: string) => {
    const shown = (key: keyof TrackFilters) => lone[key] ?? (f[key] as number | null);
    const commit = (key: keyof TrackFilters, other: keyof TrackFilters) => (v: number | null) => {
      setLone((held) => { const next = { ...held }; delete next[from]; delete next[to]; return next; });
      if (v === null) { set({ [from]: null, [to]: null }); return; }
      const partner = shown(other);
      if (partner === null || partner === undefined) setLone((held) => ({ ...held, [key]: v }));
      else set({ [key]: v, [other]: partner });
    };
    return (
      <div className="track-range">
        <NumberField feature={features[0]} label={fromLabel} value={shown(from)} min={0} max={360} step={10}
          title={title} onCommit={commit(from, to)} />
        <NumberField feature={features[1]} label={t("to (°)")} value={shown(to)} min={0} max={360} step={10}
          title={title} onCommit={commit(to, from)} />
      </div>
    );
  };
  return (
    <fieldset className="track-filters">
      <legend>{t("Sample filters")}</legend>
      {track && <p className="muted">
        {t("{filtered} of {count} samples filtered out; they stay in the project and show dimmed.", { filtered: track.filtered, count: track.samples })}
      </p>}
      {prefix === "tracks" && <TimeField feature={filterFeature(prefix, "time-start")} label={t("From (UTC)")} value={f.time_start}
        title={t("Leave out samples before this time, such as before the start")}
        onCommit={(v) => set({ time_start: v })} />}
      {prefix === "tracks" && <TimeField feature={filterFeature(prefix, "time-end")} label={t("To (UTC)")} value={f.time_end}
        title={t("Leave out samples after this time, such as after the finish")}
        onCommit={(v) => set({ time_end: v })} />}
      <fieldset className="track-env-filters">
        <legend>{t("Boat speed")}</legend>
        {sources?.speed && <SourceChoice feature="tracks:speed-source" label={t("Use")} choice={sources.speed} derived={t("Derived from the positions")} />}
        <div className="track-range">
          <NumberField feature={filterFeature(prefix, "min-bsp")} label={t("BSP from ({unit})", { unit: speed.symbol })} value={f.min_bsp} min={0} max={100} step={0.5} factor={speed.factor}
            title={t("Leave out samples slower than this; empty for no minimum")} onCommit={(v) => set({ min_bsp: v })} />
          <NumberField feature={filterFeature(prefix, "max-bsp")} label={t("to ({unit})", { unit: speed.symbol })} value={f.max_bsp} min={0} max={100} step={0.5} factor={speed.factor}
            title={t("Leave out samples faster than this; empty for no maximum")} onCommit={(v) => set({ max_bsp: v })} />
        </div>
        <div className="track-range">
          <NumberField feature={filterFeature(prefix, "vmg-min")} label={t("VMG from ({unit})", { unit: speed.symbol })} value={f.vmg_min} min={-100} max={100} step={0.5} factor={speed.factor}
            title={t("Speed made good to windward: positive upwind, negative downwind. Leave out samples below this")} onCommit={(v) => set({ vmg_min: v })} />
          <NumberField feature={filterFeature(prefix, "vmg-max")} label={t("to ({unit})", { unit: speed.symbol })} value={f.vmg_max} min={-100} max={100} step={0.5} factor={speed.factor}
            title={t("Leave out samples whose VMG is above this")} onCommit={(v) => set({ vmg_max: v })} />
        </div>
      </fieldset>
      <fieldset className="track-env-filters">
        <legend>{t("Boat heading")}</legend>
        {sources?.heading && <SourceChoice feature="tracks:heading-source" label={t("Use")} choice={sources.heading} derived={t("Derived from the positions")} />}
        {compass("heading_from", "heading_to", [filterFeature(prefix, "heading-from"), filterFeature(prefix, "heading-to")], t("Heading from (°)"), t("Keep samples heading from this compass direction clockwise to the next; through the water where the current is corrected for. Both empty for any"))}
        {compass("cog_from", "cog_to", [filterFeature(prefix, "cog-from"), filterFeature(prefix, "cog-to")], t("COG from (°)"), t("Keep samples whose course over the ground runs from this compass direction clockwise to the next. Both empty for any"))}
        <NumberField feature={filterFeature(prefix, "manoeuvre")} label={t("Direction change (°)")} value={f.max_heading_change} min={1} max={180} step={5}
          title={t("Exclude points when heading changes by more than this from the previous or next point. Empty disables the filter.")}
          onCommit={(v) => set({ max_heading_change: v })} />
        <label className="track-field">
          <input type="checkbox" data-feature={filterFeature(prefix, "tacks")} checked={f.exclude_tacks}
            title={t("Leave out the sample on either side of each tack and gybe: where the wind crosses the bow or the stern between neighbours")}
            onChange={(e) => set({ exclude_tacks: e.target.checked })} />
          {t("Remove tacks and gybes")}
        </label>
      </fieldset>
      <fieldset className="track-env-filters">
        <legend>{t("Wind, waves and current")}</legend>
        {track?.env_status === "not_fetched" && track.supplied_wind === 0 && <p className="muted">{needsWind}</p>}
        {sources?.wind && <SourceChoice feature="tracks:wind-source" label={t("Use")} choice={sources.wind} derived={t("Downloaded weather")} />}
        <div className="track-range">
          <NumberField feature={filterFeature(prefix, "tws-min")} label={t("TWS from ({unit})", { unit: speed.symbol })} value={f.tws_min} min={0} max={200} step={1} factor={speed.factor}
            title={t("Leave out samples in less wind than this; empty for no minimum")} onCommit={(v) => set({ tws_min: v })} />
          <NumberField feature={filterFeature(prefix, "tws-max")} label={t("to ({unit})", { unit: speed.symbol })} value={f.tws_max} min={0} max={200} step={1} factor={speed.factor}
            title={t("Leave out samples in more wind than this; empty for no maximum")} onCommit={(v) => set({ tws_max: v })} />
        </div>
        {compass("twd_from", "twd_to", [filterFeature(prefix, "twd-from"), filterFeature(prefix, "twd-to")], t("TWD from (°)"), t("Keep samples whose true wind comes from this compass direction clockwise to the next. Both empty for any"))}
        <div className="track-range">
          <NumberField feature={filterFeature(prefix, "twa-min")} label={t("TWA from (°)")} value={f.twa_min} min={0} max={180} step={5}
            title={t("Leave out samples closer to the wind than this")} onCommit={(v) => set({ twa_min: v })} />
          <NumberField feature={filterFeature(prefix, "twa-max")} label={t("to (°)")} value={f.twa_max} min={0} max={180} step={5}
            title={t("Leave out samples further off the wind than this")} onCommit={(v) => set({ twa_max: v })} />
        </div>
        <NumberField feature={filterFeature(prefix, "awa-change")} label={t("AWA change (°)")} value={f.max_awa_change} min={0} max={180} step={5}
          title={t("Exclude points when apparent wind angle changes by more than this from the previous or next point.")} onCommit={(v) => set({ max_awa_change: v })} />
        <NumberField feature={filterFeature(prefix, "wind-speed-change")} label={t("Wind speed change ({unit})", { unit: speed.symbol })} value={f.max_wind_speed_change} min={0} max={200} step={0.5} factor={speed.factor}
          title={t("Exclude points when true wind speed changes by more than this from the previous or next point.")} onCommit={(v) => set({ max_wind_speed_change: v })} />
        <NumberField feature={filterFeature(prefix, "wind-direction-change")} label={t("Wind direction change (°)")} value={f.max_wind_direction_change} min={0} max={180} step={5}
          title={t("Exclude points when true wind direction changes by more than this from the previous or next point.")} onCommit={(v) => set({ max_wind_direction_change: v })} />
        <label className="track-field">
          <input type="checkbox" data-feature={filterFeature(prefix, "unknown-wave")} checked={f.exclude_unknown_wave}
            onChange={(e) => set({ exclude_unknown_wave: e.target.checked })} />
          {t("Exclude unknown waves")}
        </label>
        <label className="track-field">
          <input type="checkbox" data-feature={filterFeature(prefix, "unknown-current")} checked={f.exclude_unknown_current}
            onChange={(e) => set({ exclude_unknown_current: e.target.checked })} />
          {t("Exclude unknown current")}
        </label>
        <div className="track-range">
          <NumberField feature={filterFeature(prefix, "hs-min")} label={t("Wave height from ({unit})", { unit: wave.symbol })} value={f.hs_min} min={0} max={100} step={0.5} factor={wave.factor}
            title={t("Leave out samples in smaller waves than this")} onCommit={(v) => set({ hs_min: v })} />
          <NumberField feature={filterFeature(prefix, "hs-max")} label={t("to ({unit})", { unit: wave.symbol })} value={f.hs_max} min={0} max={100} step={0.5} factor={wave.factor}
            title={t("Leave out samples in bigger waves than this")} onCommit={(v) => set({ hs_max: v })} />
        </div>
        <label className="track-field track-choice-field">
          {t("Wave direction")}
          <select data-feature={filterFeature(prefix, "wave-mode")} value={f.wave_mode}
            title={t("Keep samples by where the waves come from: off the bow by sector or angle, or by compass direction")}
            onChange={(e) => set({ wave_mode: e.target.value })}>
            <option value="off">{t("Any")}</option>
            <option value="sectors">{t("Sectors off the bow")}</option>
            <option value="relative">{t("Angle off the bow")}</option>
            <option value="cog">{t("Angle to COG")}</option>
            <option value="absolute">{t("Compass direction")}</option>
          </select>
        </label>
        {f.wave_mode === "sectors" && (
          <fieldset className="track-wave-sectors" data-feature={filterFeature(prefix, "wave-sectors")}>
            <legend>{t("Waves from")}</legend>
            {WAVE_SECTORS.map(([id, label]) => (
              <label key={id}>
                <input type="checkbox" checked={f.wave_sectors.includes(id)} onChange={(e) => set({
                  wave_sectors: e.target.checked ? [...f.wave_sectors, id] : f.wave_sectors.filter((s) => s !== id),
                })} />
                {t(label)}
              </label>
            ))}
          </fieldset>
        )}
        {(f.wave_mode === "relative" || f.wave_mode === "cog") && (
          <div className="track-range">
            <NumberField feature={filterFeature(prefix, "wave-min")} label={t("Off the bow from (°)")} value={f.wave_min} min={0} max={180} step={5}
              title={t("0° is head seas, 180° following seas")} onCommit={(v) => set({ wave_min: v })} />
            <NumberField feature={filterFeature(prefix, "wave-max")} label={t("to (°)")} value={f.wave_max} min={0} max={180} step={5}
              title={t("0° is head seas, 180° following seas")} onCommit={(v) => set({ wave_max: v })} />
          </div>
        )}
        {f.wave_mode === "absolute" && (
          <div className="track-range">
            <NumberField feature={filterFeature(prefix, "wave-from")} label={t("From the compass (°)")} value={f.wave_from} min={0} max={360} step={10}
              title={t("Waves coming from this direction, clockwise to the next")} onCommit={(v) => set({ wave_from: v })} />
            <NumberField feature={filterFeature(prefix, "wave-to")} label={t("to (°)")} value={f.wave_to} min={0} max={360} step={10}
              title={t("Waves coming from up to this direction")} onCommit={(v) => set({ wave_to: v })} />
          </div>
        )}
        <div className="track-range">
          <NumberField feature={filterFeature(prefix, "current-min")} label={t("Current from ({unit})", { unit: speed.symbol })} value={f.current_min} min={0} max={100} step={0.1} factor={speed.factor}
            title={t("Leave out samples in less current than this")} onCommit={(v) => set({ current_min: v })} />
          <NumberField feature={filterFeature(prefix, "current-max")} label={t("to ({unit})", { unit: speed.symbol })} value={f.current_max} min={0} max={100} step={0.1} factor={speed.factor}
            title={t("Leave out samples in more current than this")} onCommit={(v) => set({ current_max: v })} />
        </div>
        <label className="track-field">
          <input type="checkbox" data-feature={filterFeature(prefix, "no-tide")} checked={f.exclude_no_tide}
            title={t("Leave out samples whose current comes from a source without tides")}
            onChange={(e) => set({ exclude_no_tide: e.target.checked })} />
          {t("Leave out currents without tide ({count})", { count: track?.no_tide ?? 0 })}
        </label>
      </fieldset>
    </fieldset>
  );
}
