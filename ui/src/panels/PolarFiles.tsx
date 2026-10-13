import { useBoatApi } from "../boats/context";
import { useState } from "react";

import { reportFailure } from "../errors";
import type { PolarImportFailure } from "../generated/PolarImportFailure";
import type { ProjectSummary } from "../generated/ProjectSummary";
import { later, setHint } from "../hint";
import { msg, useT } from "../i18n";

import { pickPolarFiles } from "../project/dialogs";
import { describeAxes, describeFailure, FORMAT_NAMES } from "./polarImport";

/**
 * The Polar files section of the left navigation (spec.md 6): Import… opens
 * a native picker for several files, Rust parses them and adds one source per
 * file as a single undoable change, and the files that failed are listed with
 * their line, column and reason until the next import. Below, every imported
 * file with its colour, format and axes, and Remove (undoable).
 */
export default function PolarFiles({ project, onProject }: {
  project: ProjectSummary;
  onProject: (project: ProjectSummary) => void;
}) {
  const api = useBoatApi();
  const t = useT();
  const [failures, setFailures] = useState<PolarImportFailure[]>([]);
  const files = project.sources.filter((source) => source.polar_file !== null);

  const importFiles = async () => {
    try {
      const paths = await pickPolarFiles();
      if (paths.length === 0) return;
      const result = await api.importPolarFiles(paths);
      onProject(result.project);
      setFailures(result.failures);
      const count = result.imported.length;
      if (count > 0) {
        setHint(count === 1 ? later(msg("Imported 1 polar file.")) : later(msg("Imported {count} polar files."), { count }));
      }
    } catch (error) {
      reportFailure(error);
    }
  };

  const remove = (id: number) => {
    void api.removeSource(id).then(onProject).catch(reportFailure);
  };

  return (
    <>
      <div className="section-actions">
        <button data-feature="polar-files:import" onClick={() => void importFiles()}
          title={t("Import Expedition, Adrena, qtVlm, OpenCPN, LuckGrib and WhirlwindMatrixPolar files; several files at once")}>
          {t("Import…")}
        </button>
      </div>
      {failures.length > 0 && (
        <ul className="import-failures" role="alert" aria-label={t("Files that were not imported")}>
          {failures.map((failure, index) => (
            <li key={`${failure.file}-${index}`} title={failure.message}>{describeFailure(failure)}</li>
          ))}
        </ul>
      )}
      {files.length === 0
        ? <p className="muted placeholder">{t("No polar files in this project yet.")}</p>
        : <ul className="polar-file-list">
          {files.map((source) => (
            <li key={source.id}>
              <span className="swatch" style={{ backgroundColor: source.colour }} aria-hidden="true" />
              <span className="polar-file-text">
                <span className="polar-file-label" title={source.polar_file!.file_name}>{source.label}</span>
                <span className="muted polar-file-meta">
                  {FORMAT_NAMES[source.polar_file!.format] ?? source.polar_file!.format}
                  {" · "}
                  {describeAxes(source.polar_file!)}
                </span>
              </span>
              <button className="icon-button" data-feature="polar-files:remove" onClick={() => remove(source.id)}
                title={t("Remove this polar file from the project (undoable)")} aria-label={t("Remove")}>
                ✕
              </button>
            </li>
          ))}
        </ul>}
    </>
  );
}
