import { useEffect, useState } from "react";

import { chordText } from "../chords";
import { describeError } from "../errors";
import { api } from "../ipc";
import type { AppSettings } from "../generated/AppSettings";
import type { ProjectSummary } from "../generated/ProjectSummary";
import type { RecentProject } from "../generated/RecentProject";
import type { RecoveredProject } from "../generated/RecoveredProject";
import { openHelp } from "../help/open";
import { useLanguage, useT } from "../i18n";
import LanguagePicker from "../i18n/LanguagePicker";
import ConfirmDialog from "./ConfirmDialog";
import { pickProjectToOpen } from "./dialogs";
import NewProjectForm, { type NewProjectRequest } from "./NewProjectForm";

/**
 * Shown when no project is open (spec.md 3.1). Same layout and component
 * structure as VectorEffects' start screen: a centred panel with New
 * project, Open…, Recent projects and Recovered work, and the language
 * picker, Help and Settings in its header.
 *
 * No project is open here, so nothing can be lost by opening one: the save
 * guard has nothing to ask.
 */
export default function StartScreen({
  onOpened,
  onSettings,
  onPreferences,
  onOpenTracker,
}: {
  onOpenTracker?: () => void;
  onOpened: (project: ProjectSummary) => void;
  onSettings: () => void;
  /** The settings after the language picker saved them. */
  onPreferences: (settings: AppSettings) => void;
}) {
  const t = useT();
  const language = useLanguage();
  const [recent, setRecent] = useState<RecentProject[]>([]);
  const [recovered, setRecovered] = useState<RecoveredProject[]>([]);
  const [busy, setBusy] = useState(false);
  /** The last failure, described at render so a language switch relabels it. */
  const [error, setError] = useState<unknown>(null);
  /** The confirmation up, if any: clearing the list, or forgetting one missing file. */
  const [confirm, setConfirm] = useState<{ kind: "clear" } | { kind: "missing"; entry: RecentProject } | null>(null);

  useEffect(() => {
    api.recentProjects().then(setRecent).catch(() => setRecent([]));
    api.recoveredProjects().then(setRecovered).catch(() => setRecovered([]));
  }, []);

  const report = (err: unknown) => setError(err ?? new Error("unknown"));

  const run = async (action: () => Promise<ProjectSummary>) => {
    setBusy(true);
    setError(null);
    try {
      onOpened(await action());
    } catch (err) {
      report(err);
    } finally {
      setBusy(false);
    }
  };

  const create = (request: NewProjectRequest) => void run(() => api.newProject(request.name));
  const openFrom = (path: string) => run(() => api.openProject(path));
  const browse = async () => {
    let path: string | null;
    try {
      path = await pickProjectToOpen();
    } catch (err) {
      report(err);
      return;
    }
    if (path !== null) await openFrom(path);
  };

  /** The list is replaced with what the backend reports, so a refused write shows. */
  const clearRecent = async () => {
    setConfirm(null);
    setError(null);
    try {
      setRecent(await api.clearRecent());
    } catch (err) {
      report(err);
    }
  };
  const forget = async (entry: RecentProject) => {
    setConfirm(null);
    try {
      setRecent(await api.forgetRecentProject(entry.path));
    } catch (err) {
      report(err);
    }
  };

  return (
    <div className="start">
      <div className="start-panel">
        <header>
          <div className="start-header-controls">
            <LanguagePicker feature="start:language" onSettings={onPreferences} />
            <button data-feature="start:help" onClick={() => openHelp()} title={t("Open the help reference (F1)")}>
              {t("Help")}
            </button>
            <button data-feature="start:settings" onClick={onSettings} title={t("Language, theme, units, autosave, cache and network ({chord})", { chord: chordText(["accel", ","]) })}>
              {t("Settings")}
            </button>
          </div>
          <h1>PolarExplorer</h1>
          <p className="muted">
            {t("Build sailing polars from certificates, polar files and race tracks.")}
          </p>
        </header>

        <section className="start-new" data-feature="start:new">
          <h2>{t("New project")}</h2>
          <NewProjectForm disabled={busy} submitLabel={t("Create project")} onSubmit={create} />
        </section>

        {recovered.length > 0 && (
          <section className="start-recover" data-feature="start:recover">
            <h2>{t("Recovered work")}</h2>
            <p className="muted">
              {t("PolarExplorer did not close cleanly. These are snapshots of unsaved work; recovering one opens it as the project it came from, unsaved.")}
            </p>
            <ul className="recent">
              {recovered.map((entry) => (
                <li key={entry.id}>
                  <button
                    className="recent-item"
                    disabled={busy}
                    onClick={() => void run(() => api.openRecovered(entry.id))}
                    title={entry.original_path ?? t("never saved")}
                  >
                    <span className="recent-name">{entry.name}</span>
                    <span className="recent-path muted">
                      {entry.original_path ?? t("never saved")} · {new Date(entry.saved_unix_s * 1000).toLocaleString(language)}
                    </span>
                  </button>
                  <button
                    disabled={busy}
                    onClick={() => void api.discardRecovered(entry.id).then(setRecovered).catch(report)}
                    title={t("Delete this snapshot")}
                  >
                    {t("Discard")}
                  </button>
                </li>
              ))}
            </ul>
          </section>
        )}

        <section className="start-open">
          <h2>{t("Open")}</h2>
          <button onClick={() => void browse()} disabled={busy} data-feature="start:browse"
            title={t("Open a saved .wpsproj project or a read-only .pelocked project")}>
            {t("Open…")}
          </button>

          <button data-feature="start:tracker" onClick={onOpenTracker} disabled={busy}>{t("Open project from tracker…")}</button>

          {recent.length > 0 && (
            <>
              <div className="recent-header">
                <h3 className="muted">{t("Recent projects")}</h3>
                <button
                  disabled={busy}
                  data-feature="start:clear-recent"
                  onClick={() => setConfirm({ kind: "clear" })}
                  title={t("Forget every project in this list. The projects themselves are not deleted.")}
                >
                  {t("Clear")}
                </button>
              </div>
              <ul className="recent" data-feature="start:recent">
                {recent.map((entry) => (
                  <li key={entry.path}>
                    <button
                      className={entry.exists ? "recent-item" : "recent-item missing"}
                      disabled={busy}
                      onClick={() => entry.exists ? void openFrom(entry.path) : setConfirm({ kind: "missing", entry })}
                      title={entry.exists ? entry.path : t("{path} was not found. Click to remove it from the list.", { path: entry.path })}
                    >
                      <span className="recent-name">{entry.name}</span>
                      <span className="recent-path muted">{entry.path}</span>
                      {!entry.exists && <span className="not-found">{t("Not found")}</span>}
                    </button>
                  </li>
                ))}
              </ul>
            </>
          )}
        </section>

        {error !== null && <p className="error" role="alert" title={describeError(error).detail}>{describeError(error).text}</p>}
      </div>

      {confirm?.kind === "clear" && (
        <ConfirmDialog
          title={t("Clear recent projects")}
          body={t("This forgets the list of recently opened projects. The projects themselves are not deleted.")}
          confirmLabel={t("Clear")}
          onConfirm={() => void clearRecent()}
          onCancel={() => setConfirm(null)}
        />
      )}
      {confirm?.kind === "missing" && (
        <ConfirmDialog
          title={t("Project not found")}
          body={t("{path} is no longer there. Remove it from the recent projects?", { path: confirm.entry.path })}
          confirmLabel={t("Remove")}
          onConfirm={() => void forget(confirm.entry)}
          onCancel={() => setConfirm(null)}
        />
      )}
    </div>
  );
}
