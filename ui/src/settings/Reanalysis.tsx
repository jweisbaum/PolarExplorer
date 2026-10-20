/**
 * Settings → Reanalysis (design 2026-10-19): the archive weather fetches
 * read, chosen among those Hindsight Sync lists. Three states, from what the
 * last probe or scan found and whether one ever found Hindsight Sync: the
 * warning with its logo and site, a quiet "not running" line, or the list.
 * Scan and Port are in all three, since they are how a state is left.
 */
import { useEffect, useState } from "react";

import logo from "../assets/hindsight-sync.png";
import type { AppSettings } from "../generated/AppSettings";
import type { ArchiveStorage } from "../generated/ArchiveStorage";
import type { RememberedArchive } from "../generated/RememberedArchive";
import type { ScanResult } from "../generated/ScanResult";
import { useT } from "../i18n";
import { api } from "../ipc";
import IntegerField from "./IntegerField";

/** A product domain, read the same in every language. */
const SITE = "hindsight-sync.whirlwindai.com";
const STORAGES: ArchiveStorage[] = ["cloud", "local"];

/** A row of the list: what it is, where, how it is chunked, what it holds. Built here, not translated. */
const row = (a: RememberedArchive) =>
  [a.name, a.place, `${a.chunk_degrees}° × ${a.chunk_hours} h`, a.products.join(", ")].join(" — ");

/** The date part of an RFC 3339 time, as the range shows it. */
const day = (time: string) => time.slice(0, 10);

export default function Reanalysis({ settings, onSettings, onError, pushed }: {
  settings: AppSettings;
  onSettings: (settings: AppSettings) => void;
  /** Shown the way the dialog shows any refused setting. */
  onError: (error: unknown) => void;
  /** The last `ScanResult` Rust sent on its own (`reanalysis://changed`), if any. */
  pushed: ScanResult | null;
}) {
  const t = useT();
  const [result, setResult] = useState<ScanResult | null>(null);
  const [scanning, setScanning] = useState(false);
  const { port, connected_once, archive } = settings.reanalysis;

  useEffect(() => {
    let live = true;
    api.reanalysisProbe().then(found => { if (live) setResult(found); }, failure => { if (live) onError(failure); });
    return () => { live = false; };
  }, []);
  useEffect(() => { if (pushed) setResult(pushed); }, [pushed]);

  const scan = () => {
    setScanning(true);
    api.reanalysisScan().then(setResult, onError).finally(() => setScanning(false));
  };
  const savePort = (value: number) => {
    api.setReanalysisPort(value)
      .then(next => { onSettings(next); return api.reanalysisProbe(); })
      .then(setResult)
      .catch(onError);
  };
  const choose = (id: string) => {
    if (id === "" || id === archive?.id) return;
    api.setReanalysisArchive(id).then(onSettings, onError);
  };

  const controls = <div className="reanalysis-controls">
    <button data-feature="settings:reanalysis-scan" title={t("Look for Hindsight Sync on this computer")}
      disabled={scanning} onClick={scan}>{scanning ? t("Scanning…") : t("Scan")}</button>
    <label className="settings-field">
      {t("Port")}
      <IntegerField data-feature="settings:reanalysis-port" aria-label={t("Port")}
        title={t("The port Hindsight Sync's archive API listens on (1–65535)")}
        value={port} min={1} max={65535} onCommit={savePort} />
    </label>
  </div>;

  // Until the first probe answers, nothing was found, but a person who has
  // connected before is not shown the warning meant for a first run.
  if (!result?.detected) {
    if (!connected_once) {
      return <>
        <div className="reanalysis-warning">
          <img src={logo} alt={t("Hindsight Sync logo")} width={48} height={48} />
          <p>
            {t("PolarExplorer needs Hindsight Sync to download the reanalysis.")}
            <br />
            <a href="#" className="reanalysis-link" data-feature="settings:reanalysis-link"
              onClick={event => { event.preventDefault(); api.openHindsightSyncSite().catch(onError); }}>{SITE}</a>
          </p>
        </div>
        {controls}
      </>;
    }
    return <>
      <p className="muted">{archive
        ? t("Hindsight Sync is not running. The remembered archive is still used for fetches.")
        : t("Hindsight Sync is not running. No archive is chosen.")}</p>
      {controls}
    </>;
  }

  const listed = archive !== null && result.archives.some(a => a.id === archive.id);
  const empty = result.archives.length === 0 && result.unavailable.length === 0;
  const range = archive === null ? null : archive.stop === null
    ? t("from {start}, ongoing", { start: day(archive.start) })
    : t("{start} to {stop}", { start: day(archive.start), stop: day(archive.stop) });
  const status = [t("Connected to Hindsight Sync on port {port}", { port: result.port }), range].filter(Boolean).join(" — ");

  return <>
    {empty && <p className="muted">{t("Hindsight Sync lists no reanalysis archives.")}</p>}
    {(!empty || archive !== null) && <label className="settings-field">
      {t("Reanalysis archive")}
      <select data-feature="settings:reanalysis-archive" aria-label={t("Reanalysis archive")}
        value={archive?.id ?? ""} onChange={event => choose(event.target.value)}>
        {archive === null && <option value="">{t("Choose an archive…")}</option>}
        {STORAGES.map(storage => {
          const archives = result.archives.filter(a => a.storage === storage);
          const unavailable = result.unavailable.filter(u => u.storage === storage);
          const gone = archive !== null && !listed && archive.storage === storage ? archive : null;
          if (archives.length === 0 && unavailable.length === 0 && gone === null) return null;
          return <optgroup key={storage} label={storage === "cloud" ? t("Cloud") : t("This computer")}>
            {archives.map(a => <option key={a.id} value={a.id}>{row(a)}</option>)}
            {unavailable.map(u => <option key={`unavailable:${u.id}`} value={`unavailable:${u.id}`} disabled title={u.reason}>{u.name ?? u.id}</option>)}
            {gone && <option value={gone.id}>{`${row(gone)} ${t("(no longer listed)")}`}</option>}
          </optgroup>;
        })}
      </select>
    </label>}
    <p className="muted">{status}</p>
    {controls}
  </>;
}
