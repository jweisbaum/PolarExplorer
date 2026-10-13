/**
 * Native file dialogs. Copied from VectorEffects.
 *
 * A web `<input type="file">` cannot give a real path, which a desktop app
 * needs in order to save back to the same file, so these go through Tauri's
 * dialog plugin.
 */
import { open, save } from "@tauri-apps/plugin-dialog";

import { takeDialogAnswer } from "../automation";
import { whileChoosing as choosing } from "../busy";
import { msg, t } from "../i18n";

/**
 * Shows a native dialog, unless the WebDriver tools queued its answer
 * (development builds only, `automation.ts`).
 */
function whileChoosing<T>(busy: string, dialog: () => Promise<T>): Promise<T> {
  const queued = takeDialogAnswer();
  if (queued !== undefined) return Promise.resolve(queued as T);
  return choosing(busy, dialog);
}

/** The project file extension (spec.md 4.3). */
export const EXTENSION = "wpsproj";

/** A function, not a constant: the filter's name is read in the language on screen. */
const openFilters = () => [{ name: t("PolarExplorer project"), extensions: [EXTENSION, "pelocked"] }];
const saveFilters = () => [{ name: t("PolarExplorer project"), extensions: [EXTENSION] }];

/** Asks for a project to open. Returns null if the user cancelled. */
export async function pickProjectToOpen(): Promise<string | null> {
  const chosen = await whileChoosing(msg("Choosing a project"), () =>
    open({ multiple: false, directory: false, filters: openFilters() }),
  );
  return typeof chosen === "string" ? chosen : null;
}

/** Asks where to save. Returns null if the user cancelled. */
export async function pickProjectToSave(suggestedName: string): Promise<string | null> {
  const chosen = await whileChoosing(msg("Choosing where to save"), () =>
    save({ defaultPath: `${suggestedName}.${EXTENSION}`, filters: saveFilters() }),
  );
  return typeof chosen === "string" ? chosen : null;
}

/** Asks for a portable PolarExplorer tab to import into the open project. */
export async function pickTabToImport(): Promise<string | null> {
  const chosen = await whileChoosing(msg("Choosing a tab to import"), () =>
    open({
      multiple: false,
      directory: false,
      title: t("Import Tab"),
      filters: [{ name: t("PolarExplorer tabs"), extensions: ["petab"] }],
    }),
  );
  return typeof chosen === "string" ? chosen : null;
}

/** Asks where to write a portable copy of one tab. */
export async function pickTabExportPath(suggestedName: string): Promise<string | null> {
  const chosen = await whileChoosing(msg("Choosing where to export the tab"), () =>
    save({
      defaultPath: `${suggestedName}.petab`,
      title: t("Export Tab"),
      filters: [{ name: t("PolarExplorer tabs"), extensions: ["petab"] }],
    }),
  );
  return typeof chosen === "string" ? chosen : null;
}

/** Asks where to write all tabs as a locked project. */
export async function pickLockedProjectExportPath(suggestedName: string): Promise<string | null> {
  const chosen = await whileChoosing(msg("Choosing where to export the project"), () =>
    save({
      defaultPath: `${suggestedName}.pelocked`,
      title: t("Export read-only project"),
      filters: [{ name: t("Read-only project"), extensions: ["pelocked"] }],
    }),
  );
  return typeof chosen === "string" ? chosen : null;
}

/**
 * Asks for polar files to import (spec.md 6): several at once. The format is
 * read from the content, so "All files" is offered beside the usual
 * extensions. Returns the chosen paths; empty if the user cancelled.
 */
export async function pickPolarFiles(): Promise<string[]> {
  const chosen = await whileChoosing(msg("Choosing polar files"), () =>
    open({
      multiple: true,
      directory: false,
      title: t("Import polar files"),
      filters: [
        { name: t("Polar files"), extensions: ["txt", "pol", "csv", "tsv", "wmpolar"] },
        { name: t("All files"), extensions: ["*"] },
      ],
    }),
  );
  if (Array.isArray(chosen)) return chosen.filter((path): path is string => typeof path === "string");
  return typeof chosen === "string" ? [chosen] : [];
}

/**
 * Asks for track files to import (spec.md 7.3): GPX, GeoJSON or CSV, several
 * at once. Returns the chosen paths; empty if the user cancelled.
 */
export async function pickTrackFiles(): Promise<string[]> {
  const chosen = await whileChoosing(msg("Choosing track files"), () =>
    open({
      multiple: true,
      directory: false,
      title: t("Import tracks"),
      filters: [
        { name: t("Track files"), extensions: ["geojson", "json", "gpx", "csv", "txt", "tsv"] },
        { name: t("All files"), extensions: ["*"] },
      ],
    }),
  );
  if (Array.isArray(chosen)) return chosen.filter((path): path is string => typeof path === "string");
  return typeof chosen === "string" ? [chosen] : [];
}

/** The file extension each export format is written with (spec.md 6). */
export const EXPORT_EXTENSIONS: Readonly<Record<string, string>> = {
  expedition: "txt",
  adrena: "pol",
  csv: "csv",
  qtvlm: "csv",
  opencpn: "pol",
  luckgrib: "tsv",
  whirlwind_matrix_polar: "wmpolar",
};

/**
 * Asks where to export the blend (spec.md 12), suggesting `<name>.<ext>`
 * for the format. Returns null if the user cancelled.
 */
export async function pickExportPath(format: string, suggestedName: string): Promise<string | null> {
  const extension = EXPORT_EXTENSIONS[format] ?? "txt";
  const chosen = await whileChoosing(msg("Choosing where to export"), () =>
    save({
      title: t("Export the polar"),
      defaultPath: `${suggestedName}.${extension}`,
      filters: [{ name: t("Polar file"), extensions: [extension] }],
    }),
  );
  return typeof chosen === "string" ? chosen : null;
}

/** Local database library paths, shared with the development dialog seam. */
export async function pickLibraryDirectory(): Promise<string | null> {
  const chosen = await whileChoosing(msg("Choosing a library directory"), () => open({ directory: true, multiple: false }));
  return typeof chosen === "string" ? chosen : null;
}

export async function pickExportDirectory(): Promise<string | null> {
  const chosen = await whileChoosing(msg("Choosing where to export"), () => open({ directory: true, multiple: false, title: t("Export all polars") }));
  return typeof chosen === "string" ? chosen : null;
}

/** Whirlwind's reusable compressed chunks. */
export async function pickWeatherCacheDirectory(): Promise<string | null> {
  const chosen = await whileChoosing(msg("Choose cache directory"), () => open({ directory: true, multiple: false, title: t("Choose cache directory") }));
  return typeof chosen === "string" ? chosen : null;
}
