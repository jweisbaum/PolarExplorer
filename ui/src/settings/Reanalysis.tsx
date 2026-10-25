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

/** Also understands remembered endpoint labels from older settings files. */
const provider = (place: string) => /(\bR2\b|\.r2\.cloudflarestorage\.)/i.test(place) ? "R2"
  : /(\bTigris\b|\.tigris\.dev)/i.test(place) ? "Tigris" : "S3";
const basename = (name: string) => name.replace(/\\/g, "/").split("/").filter(Boolean).at(-1) ?? name;

/** Provider names and archive metadata read the same in every language. */
const row = (a: RememberedArchive) =>
  [a.storage === "cloud" ? provider(a.place) : basename(a.name || a.place), `${a.chunk_degrees}° × ${a.chunk_hours} h`, a.products.join(", ")].join(" — ");

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

  /**
   * A probe or scan that finds Hindsight Sync saves what it found (the port
   * it answered on, that it connected, the chosen archive's fresh copy), so
   * the settings are read again: what is shown is what is on disk.
   */
  const adopt = async (found: ScanResult) => {
    if (found.detected) onSettings(await api.appSettings());
    setResult(found);
  };

  useEffect(() => {
    let live = true;
    api.reanalysisProbe().then(found => live ? adopt(found) : undefined).catch(failure => { if (live) onError(failure); });
    return () => { live = false; };
  }, []);
  useEffect(() => { if (pushed) setResult(pushed); }, [pushed]);

  const scan = () => {
    setScanning(true);
    api.reanalysisScan().then(adopt).catch(onError).finally(() => setScanning(false));
  };
  const savePort = (value: number) => {
    api.setReanalysisPort(value)
      .then(next => { onSettings(next); return api.reanalysisProbe(); })
      .then(adopt)
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

  const selected = result.archives.find(a => a.id === archive?.id) ?? null;
  const empty = result.archives.length === 0;
  // The mode is Hindsight Sync's identifier (`global`, …), shown as it is.
  const range = selected === null ? null : `${selected.stop === null
    ? t("from {start}, ongoing", { start: day(selected.start) })
    : t("{start} to {stop}", { start: day(selected.start), stop: day(selected.stop) })} · ${selected.mode}`;
  const status = [t("Connected to Hindsight Sync on port {port}", { port: result.port }), range].filter(Boolean).join(" — ");

  return <>
    {empty && <p className="muted">{t("Hindsight Sync lists no reanalysis archives.")}</p>}
    {!empty && <label className="settings-field">
      {t("Reanalysis archive")}
      <select data-feature="settings:reanalysis-archive" aria-label={t("Reanalysis archive")}
        value={selected?.id ?? ""} onChange={event => choose(event.target.value)}>
        {selected === null && <option value="">{t("Choose an archive…")}</option>}
        {STORAGES.map(storage => {
          const archives = result.archives.filter(a => a.storage === storage);
          if (archives.length === 0) return null;
          return <optgroup key={storage} label={storage === "cloud" ? t("Cloud") : t("This computer")}>
            {archives.map(a => <option key={a.id} value={a.id}>{row(a)}</option>)}
          </optgroup>;
        })}
      </select>
    </label>}
    <p className="muted">{status}</p>
    {controls}
  </>;
}
