/**
 * Errors as a person reads them, in the interface language (invariant 7,
 * spec.md 3.5).
 *
 * Rust's messages are English sentences written for logs and bug reports.
 * What the interface shows is keyed by the error's `kind` — the stable
 * discriminant every `AppError` carries — and translated here; the English
 * message goes in `detail`, which the interface shows only as a tooltip.
 * A kind this table does not know, and a failure that is not from Rust at
 * all (a dialog plugin that threw), fall back to a translated generic line.
 */

import { reportError } from "./hint";
import { t } from "./i18n";

export interface ShownError {
  /** The line to show, translated. */
  text: string;
  /** The kind and the English message, for the tooltip. */
  detail: string;
}

/** The translated line for an error kind; null for a kind with no line of its own. */
function lineFor(kind: string): string | null {
  switch (kind) {
    case "io": return t("A file could not be read or written.");
    case "core": return t("The project refused this change: it is not valid.");
    case "schema-too-new": return t("This project was saved by a newer version of PolarExplorer. Update PolarExplorer to open it.");
    case "no-project": return t("No project is open.");
    case "read-only-tab": return t("This tab or project is read only and cannot be changed.");
    case "never-saved": return t("This project has not been saved yet.");
    case "unsaved-changes": return t("The project has unsaved changes.");
    case "bad-option": return t("That value is not accepted.");
    case "orc-duplicate": return t("The project already holds this certificate.");
    case "doing": return t("The operation could not be completed.");
    case "internal": return t("Something went wrong inside PolarExplorer.");
    case "tracker-address": return t("This is not an event address this tracker serves.");
    case "tracker-unavailable": return t("The tracker is not answering right now. Try again in a moment.");
    case "tracker-no-event": return t("The tracker has no public event at this address.");
    case "tracker-decode": return t("The tracker's data could not be read.");
    case "tracker-unsupported": return t("The tracker's data is in a version PolarExplorer does not read. The tracker may have changed its format.");
    case "tracker-legacy": return t("This is an older tracker (Flash, or Geovoile before about 2016), which PolarExplorer does not read.");
    case "tracker-network": return t("The tracker could not be reached or refused the request.");
    case "cancelled": return t("Cancelled.");
    case "export-refused": return t("The polar cannot be exported as it is: its grid would not read back, or it holds nothing. The export dialog says which.");
    default: return null;
  }
}

function isKinded(error: unknown): error is { kind: string; message: string } {
  return typeof error === "object" && error !== null
    && typeof (error as { kind?: unknown }).kind === "string"
    && typeof (error as { message?: unknown }).message === "string";
}

/** An error, translated for display, with the English kept for the tooltip. */
export function describeError(error: unknown): ShownError {
  // Read by shape rather than by class: an `IpcError`, or a raw
  // `{ kind, message }` payload, reads the same.
  if (isKinded(error)) {
    return { text: lineFor(error.kind) ?? t("Something went wrong."), detail: `${error.kind}: ${error.message}` };
  }
  const message = error instanceof Error ? error.message : String(error);
  return { text: t("Something went wrong."), detail: message };
}

/** Puts an error on the status line: translated text, English detail as the tooltip. */
export function reportFailure(error: unknown): void {
  const shown = describeError(error);
  reportError(shown.text, shown.detail, error);
}
