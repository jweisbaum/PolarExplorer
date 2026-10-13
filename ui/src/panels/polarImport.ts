/**
 * What an import of polar files says back (spec.md 6), in the interface
 * language.
 *
 * Rust reports each failed file with a stable reason code (`pe_polar`'s
 * `Reason::code`, or `"unreadable"`) and an English message; the line shown is
 * built here from the code, and the English goes in the tooltip, like every
 * other error (`errors.ts`).
 */
import type { PolarFileSummary } from "../generated/PolarFileSummary";
import type { PolarImportFailure } from "../generated/PolarImportFailure";
import { t } from "../i18n";

/** Every reason code Rust can send, with what it means. */
export function reasonText(code: string): string {
  switch (code) {
    case "empty": return t("the file is empty");
    case "too-large": return t("the file is too large to be a polar");
    case "unknown-format": return t("this is neither an Expedition polar nor a TWA × TWS table");
    case "missing-header": return t("the top-left cell must be TWA\\TWS, TWA/TWS or TWA");
    case "not-a-number": return t("this is not a number");
    case "negative": return t("a speed or angle cannot be negative");
    case "too-fast": return t("speeds above 60 kn are refused");
    case "angle-out-of-range": return t("a wind angle must be between 0° and 360°");
    case "duplicate-tws": return t("this wind speed appears twice");
    case "duplicate-twa": return t("this wind angle appears twice");
    case "missing-bsp": return t("this wind angle has no boat speed after it");
    case "too-many-cells": return t("this row has more cells than the header has wind speeds");
    case "too-many-values": return t("the polar has too many wind angles or wind speeds");
    case "no-speeds": return t("the file holds no boat speeds");
    case "matrix-invalid": return t("the WhirlwindMatrixPolar data is invalid or contains no boat-speed mode");
    case "unreadable": return t("the file could not be read");
    default: return t("the file could not be imported");
  }
}

/** One failed file as a line: its name, where, and why. */
export function describeFailure(failure: PolarImportFailure): string {
  const reason = reasonText(failure.reason);
  if (failure.line === null) return t("{file}: {reason}", { file: failure.file, reason });
  return t("{file}, line {line}, column {column}: {reason}", {
    file: failure.file, line: failure.line, column: failure.column ?? 1, reason,
  });
}

/** How the formats are named; brands and file types, the same in every language. */
export const FORMAT_NAMES: Readonly<Record<string, string>> = {
  expedition: "Expedition",
  adrena: "Adrena",
  csv: "CSV",
  qtvlm: "qtVlm",
  opencpn: "OpenCPN",
  luckgrib: "LuckGrib",
  whirlwind_matrix_polar: "WhirlwindMatrixPolar",
};

/** An axis value as the list shows it: at most two decimals. */
function axisValue(value: number): string {
  return String(Math.round(value * 100) / 100);
}

/** A polar file's axes in one line: ranges and how many values each has. */
export function describeAxes(file: PolarFileSummary): string {
  const range = (axis: number[]) =>
    axis.length === 0 ? "–" : axis.length === 1 ? axisValue(axis[0]!) : `${axisValue(axis[0]!)}–${axisValue(axis[axis.length - 1]!)}`;
  return t("TWA {twa}° ({twaCount}) · TWS {tws} kn ({twsCount})", {
    twa: range(file.twa), twaCount: file.twa.length, tws: range(file.tws), twsCount: file.tws.length,
  });
}
