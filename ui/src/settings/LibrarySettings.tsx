import { listen } from "@tauri-apps/api/event";
import { type MutableRefObject, useEffect, useState } from "react";
import type { AppSettings } from "../generated/AppSettings";
import type { DatabaseConnection } from "../generated/DatabaseConnection";
import type { MetadataProgress } from "../generated/MetadataProgress";
import type { LibrarySettings as Preferences } from "../generated/LibrarySettings";
import { describeError } from "../errors";
import { useT } from "../i18n";
import { api, LIBRARY_METADATA } from "../ipc";
import { pickLibraryDirectory } from "../project/dialogs";

/** Local library folders and an explicitly requested, read-only metadata download. */
export default function LibrarySettings({ settings, onSettings, flush }: {
  settings: AppSettings;
  onSettings: (s: AppSettings) => void;
  /** Set to what saves the draft if it changed; the dialog calls it on closing. */
  flush?: MutableRefObject<(() => Promise<void>) | null>;
}) {
  const t = useT();
  const [draft, setDraft] = useState(settings.library);
  const [failure, setFailure] = useState<unknown>(null);
  const [saved, setSaved] = useState(false);
  const download = useMetadataDownload();
  const [connection, setConnection] = useState<"untested" | "testing" | "ok" | "failed">("untested");
  const running = download?.running === true;
  // Saving another section returns a fresh settings object. Preserve typed
  // library edits unless the saved library values themselves changed.
  const savedLibrary = JSON.stringify(settings.library);
  useEffect(() => setDraft(settings.library), [savedLibrary]);
  const change = (patch: Partial<Preferences>) => { setDraft(old => ({ ...old, ...patch })); setSaved(false); };
  const changeDatabase = (patch: Partial<DatabaseConnection>) => { change({ database: { ...draft.database, ...patch } }); setConnection("untested"); };
  const test = () => action(async () => {
    setConnection("testing");
    try { await api.testDatabaseConnection(draft.database); setConnection("ok"); } catch (e) { setConnection("failed"); throw e; }
  });
  // The download uses the settings on screen: they are saved first.
  const startDownload = () => action(async () => { await persist(); await api.startMetadataDownload(); });
  const persist = async () => { onSettings(await api.setLibrarySettings(draft)); setSaved(true); };
  const changed = JSON.stringify(draft) !== JSON.stringify(settings.library);
  useEffect(() => {
    if (!flush) return;
    flush.current = async () => { if (changed) await persist(); };
    return () => { flush.current = null; };
  });
  const action = async (task: () => Promise<void>) => { setFailure(null); try { await task(); } catch (e) { setFailure(e); } };
  const browse = (field: "geojson_directory" | "metadata_directory") => action(async () => {
    const path = await pickLibraryDirectory(); if (path) change({ [field]: path });
  });
  return <section className="library-settings" data-section="settings:library">
    <h3>{t("Track library")}</h3>
    <label className="settings-field">{t("GeoJSON track directory")}<input data-feature="settings:library-geojson" title={t("GeoJSON track directory")} value={draft.geojson_directory} disabled={running} onChange={e => change({ geojson_directory: e.target.value })} /></label>
    <button data-feature="settings:library-geojson-browse" title={t("Choose GeoJSON track directory")} disabled={running} onClick={() => void browse("geojson_directory")}>{t("Choose GeoJSON track directory")}</button>
    <label className="settings-field">{t("Boat metadata directory")}<input data-feature="settings:library-metadata" title={t("Leave empty to use the application data directory")} placeholder={t("Application data directory")} value={draft.metadata_directory} disabled={running} onChange={e => change({ metadata_directory: e.target.value })} /></label>
    <button data-feature="settings:library-metadata-browse" title={t("Choose boat metadata directory")} disabled={running} onClick={() => void browse("metadata_directory")}>{t("Choose boat metadata directory")}</button>
    <p className="muted">{t("Local boat searches include only YellowBrick, Geovoile, Blue Water, old Geovoile, Regadata and America's Cup records.")}</p>

    <h4>{t("SYRF database (read only)")}</h4>
    <label className="settings-field">{t("Database host")}<input data-feature="settings:library-db-host" title={t("Database host")} value={draft.database.host} disabled={running} onChange={e => changeDatabase({ host: e.target.value })} /></label>
    <label className="settings-field">{t("Database port")}<input data-feature="settings:library-db-port" title={t("Database port")} type="number" min={1} max={65535} value={draft.database.port} disabled={running} onChange={e => changeDatabase({ port: Number(e.target.value) })} /></label>
    <label className="settings-field">{t("Database name")}<input data-feature="settings:library-db-name" title={t("Database name")} value={draft.database.name} disabled={running} onChange={e => changeDatabase({ name: e.target.value })} /></label>
    <label className="settings-field">{t("Database user")}<input data-feature="settings:library-db-user" title={t("Database user")} value={draft.database.user} disabled={running} onChange={e => changeDatabase({ user: e.target.value })} /></label>
    <label className="settings-field">{t("Database password")}<input data-feature="settings:library-db-password" title={t("Database password")} type="password" autoComplete="off" value={draft.database.password} disabled={running} onChange={e => changeDatabase({ password: e.target.value })} /></label>
    <label className="settings-field">{t("Verify TLS certificate")}<input data-feature="settings:library-db-tls" title={t("Verify TLS certificate")} type="checkbox" checked={draft.database.tls} disabled={running} onChange={e => changeDatabase({ tls: e.target.checked })} /></label>
    <p className="muted">{t("Downloading reads the supported trackers' boats, races and track records into the boat metadata. Nothing is written to the database, and races saved by scraping stay.")}</p>
    <div className="settings-buttons">
      <button data-feature="settings:library-db-test" title={t("Test the connection and required database tables")} disabled={running || connection === "testing"} onClick={() => void test()}>{t("Test connection")}</button>
      <span role="status" className={`database-connection ${connection}`}>{connection === "ok" ? t("Connection successful") : connection === "failed" ? t("Connection failed") : connection === "testing" ? t("Testing connection…") : t("Connection not tested")}</span>
    </div>
    <div className="settings-buttons">
      <button data-feature="settings:library-db-download" title={t("Download boat metadata")} disabled={running} onClick={() => void startDownload()}>{t("Download boat metadata")}</button>
      <button data-feature="settings:library-db-cancel" title={t("Cancel download")} disabled={download?.running !== true} onClick={() => void api.cancelMetadataDownload().catch(setFailure)}>{t("Cancel download")}</button>
    </div>
    {download && (download.running || download.done > 0 || download.error || download.cancelled) && <div role="status" className="library-download-status">
      {download.running && <progress max={Math.max(1, download.total)} value={download.done} />}
      <p>{download.cancelled ? t("Metadata download cancelled") : download.running ? t("Downloading boat metadata…") : download.error ? t("Metadata download failed") : t("Metadata download finished")}</p>
      {download.current && <p className="library-current">{download.current}</p>}
      {!download.running && !download.error && !download.cancelled && <p>{t("Tracks from the database: {tracks}; scraped tracks kept: {kept}", { tracks: download.tracks, kept: download.kept })}</p>}
      {download.error && <p className="modal-error">{download.error}</p>}
    </div>}

    <div className="settings-buttons">
      <button data-feature="settings:library-save" title={t("Save library settings")} disabled={running} onClick={() => void action(persist)}>{t("Save library settings")}</button>
      {saved && <span role="status">{t("Library settings saved")}</span>}
    </div>
    {failure !== null && <p className="modal-error" role="alert">{describeError(failure).text}<br />{describeError(failure).detail}</p>}
  </section>;
}

/** The metadata download's progress: read once, then kept current by its event. */
function useMetadataDownload(): MetadataProgress | null {
  const [progress, setProgress] = useState<MetadataProgress | null>(null);
  useEffect(() => {
    let active = true;
    const stop = listen<MetadataProgress>(LIBRARY_METADATA, (event) => { if (active) setProgress(event.payload); });
    api.metadataDownloadStatus().then(p => { if (active) setProgress(p); }).catch(() => undefined);
    return () => { active = false; void stop.then(f => f()).catch(() => undefined); };
  }, []);
  return progress;
}
