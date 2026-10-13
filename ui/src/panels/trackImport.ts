/**
 * What the track import and the Tracks section say (spec.md 7.1, 7.3, 7.4),
 * in the interface language, kept free of React so it can be tested.
 *
 * Rust reports each failed file with a stable reason code (`pe_tracks`'
 * `Reason::code`, or `"unreadable"`, `"no-mapping"`, `"no-boat"`, and for a
 * tracker boat `"no-positions"`) and an
 * English message; the line shown is built here from the code, and the
 * English goes in the tooltip, like every other error.
 */
import type { CsvMappingInput } from "../generated/CsvMappingInput";
import type { EnvJobTrack } from "../generated/EnvJobTrack";
import type { TrackImportFailure } from "../generated/TrackImportFailure";
import type { TrackImportLine } from "../generated/TrackImportLine";
import type { TrackSummary } from "../generated/TrackSummary";
import { msg, t } from "../i18n";

/** Every reason code Rust can send, with what it means. */
export function trackReasonText(code: string): string {
  switch (code) {
    case "empty": return t("the file is empty");
    case "too-large": return t("the file is too large to be a track");
    case "not-text": return t("the file is not text");
    case "not-json": return t("this is not valid JSON");
    case "not-geojson": return t("this is not a GeoJSON Feature or FeatureCollection");
    case "bad-gpx": return t("this is not a usable GPX track");
    case "unsupported-geometry": return t("only Point, LineString and MultiLineString features are tracks");
    case "bad-position": return t("this position is not a longitude and latitude on Earth");
    case "no-time": return t("this position has no time");
    case "bad-time": return t("this time is not in the expected format");
    case "times-mismatch": return t("the line has a different number of times and positions");
    case "no-header": return t("the file has no header row");
    case "short-row": return t("this row has too few cells");
    case "unclosed-quote": return t("a quoted field is not closed");
    case "missing-column": return t("a chosen column is not in the file");
    case "not-a-number": return t("this is not a number");
    case "out-of-range": return t("this is not a heading or speed a boat can have");
    case "no-fixes": return t("the file holds no positions");
    case "too-many-fixes": return t("the file holds more positions than a track may have");
    case "unreadable": return t("the file could not be read");
    case "no-mapping": return t("choose the time, latitude and longitude columns");
    case "no-boat": return t("no boat was chosen");
    case "no-positions": return t("the tracker has no positions for this boat");
    default: return t("the file could not be imported");
  }
}

/** One failed file as a line: its name, where, and why. */
export function describeTrackFailure(failure: TrackImportFailure): string {
  const reason = trackReasonText(failure.reason);
  if (failure.line !== null) {
    return t("{file}, line {line}, column {column}: {reason}", {
      file: failure.file, line: failure.line, column: failure.column ?? 1, reason,
    });
  }
  if (failure.feature !== null) {
    return t("{file}, feature {feature}: {reason}", { file: failure.file, feature: failure.feature + 1, reason });
  }
  return t("{file}: {reason}", { file: failure.file, reason });
}

/** What one imported track's summary line says (spec.md 7.4). */
export function describeImportLine(line: TrackImportLine): string {
  const parts = [t("{count} positions", { count: line.fixes })];
  if (line.out_of_order > 0) parts.push(t("{count} out of order, sorted", { count: line.out_of_order }));
  if (line.duplicates > 0) parts.push(t("{count} duplicate times merged", { count: line.duplicates }));
  parts.push(t("heading {given} given, {derived} derived", { given: line.heading_given, derived: line.heading_derived }));
  parts.push(t("speed {given} given, {derived} derived", { given: line.speed_given, derived: line.speed_derived }));
  return t("{label}: {details}", { label: line.label, details: parts.join(" · ") });
}

/** A UTC date as `2025-07-26`. */
export function utcDate(seconds: number): string {
  return new Date(seconds * 1000).toISOString().slice(0, 10);
}

/** A track's dates: one day, or first to last. */
export function dateRange(start: number | null, end: number | null): string {
  if (start === null || end === null) return "–";
  const a = utcDate(start), b = utcDate(end);
  return a === b ? a : `${a} – ${b}`;
}

/** Epoch seconds as a `datetime-local` value in UTC, or "" for none. */
export function toLocalInput(seconds: number | null): string {
  return seconds === null ? "" : new Date(seconds * 1000).toISOString().slice(0, seconds % 60 === 0 ? 16 : 19);
}

/** A `datetime-local` value read as UTC, or null when empty or unreadable. */
export function fromLocalInput(value: string): number | null {
  if (value.trim() === "") return null;
  const ms = Date.parse(`${value.length === 16 ? `${value}:00` : value}Z`);
  return Number.isFinite(ms) ? Math.round(ms / 1000) : null;
}

const ENV_STATUS: Record<string, string> = {
  not_fetched: msg("not fetched"),
  ready: msg("ready"),
  partial: msg("partial"),
  failed: msg("failed"),
};

/**
 * The environment status column (spec.md 7.1): "fetching n %" or "queued"
 * while a job has the track, else what the project records.
 */
export function envStatusText(track: TrackSummary, job?: EnvJobTrack): string {
  if (job?.state === "fetching") return t("fetching {percent} %", { percent: Math.floor(job.fraction * 100) });
  if (job?.state === "queued") return t("queued");
  if (track.env_status === "ready" && track.with_wind === 0) return t("no points to plot");
  return t(ENV_STATUS[track.env_status] ?? ENV_STATUS.not_fetched!);
}

/** A download or file size, for the fetch's pre-flight. */
export function formatBytes(bytes: number): string {
  if (bytes === 0) return "0 kB";
  if (bytes >= 1e9) return `${(bytes / 1e9).toFixed(1)} GB`;
  if (bytes >= 1e6) return `${Math.round(bytes / 1e6)} MB`;
  return `${Math.max(1, Math.round(bytes / 1e3))} kB`;
}

/** The column roles of the CSV mapping step, in the order they are shown. */
export const CSV_ROLES = ["time", "lat", "lon", "heading", "speed", "tws", "twd", "boat"] as const;
export type CsvRole = (typeof CSV_ROLES)[number];

export const CSV_ROLE_NAMES: Record<CsvRole, string> = {
  time: msg("Time"),
  lat: msg("Latitude"),
  lon: msg("Longitude"),
  heading: msg("Heading or COG"),
  speed: msg("Speed"),
  tws: msg("True wind speed"),
  twd: msg("True wind direction"),
  boat: msg("Boat"),
};

/** Which roles must be chosen before a CSV can be read. */
export const REQUIRED_ROLES: readonly CsvRole[] = ["time", "lat", "lon"];

/** The role a column has in a mapping, if any. */
export function roleOf(mapping: CsvMappingInput, column: number): CsvRole | null {
  return CSV_ROLES.find((role) => mapping[role] === column) ?? null;
}

/** A mapping with `role` set to `column` (or none), taking it from any role that had it. */
export function assign(mapping: CsvMappingInput, role: CsvRole, column: number | null): CsvMappingInput {
  const next = { ...mapping };
  if (column !== null) for (const other of CSV_ROLES) if (next[other] === column) next[other] = null;
  next[role] = column;
  return next;
}

export const TIME_FORMATS: readonly { id: string; label: string }[] = [
  { id: "auto", label: msg("Automatic") },
  { id: "iso", label: msg("ISO 8601") },
  { id: "epoch_s", label: msg("Epoch seconds") },
  { id: "epoch_ms", label: msg("Epoch milliseconds") },
  { id: "custom", label: msg("Custom format…") },
];

export const SPEED_UNITS: readonly { id: string; label: string }[] = [
  { id: "kn", label: msg("kn — knots") },
  { id: "ms", label: msg("m/s — metres per second") },
  { id: "kmh", label: msg("km/h — kilometres per hour") },
  { id: "mph", label: msg("mph — miles per hour") },
];
