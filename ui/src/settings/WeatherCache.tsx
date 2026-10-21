import { type MutableRefObject, useEffect, useRef, useState } from "react";
import type { AppSettings } from "../generated/AppSettings";
import type { WeatherCacheStatus } from "../generated/WeatherCacheStatus";
import { describeError } from "../errors";
import { useT } from "../i18n";
import { api } from "../ipc";
import { formatBytes } from "../panels/trackImport";
import { pickWeatherCacheDirectory } from "../project/dialogs";
import IntegerField from "./IntegerField";

type Config = AppSettings["weather_cache"];
export default function WeatherCache({ settings, onSettings, flush }: {
  settings: AppSettings;
  onSettings: (settings: AppSettings) => void;
  flush: MutableRefObject<(() => Promise<void>) | null>;
}) {
  const t = useT();
  const [draft, setDraft] = useState(settings.weather_cache);
  const current = useRef(draft);
  const saved = useRef(settings.weather_cache);
  const pending = useRef(Promise.resolve());
  const [status, setStatus] = useState<WeatherCacheStatus | null>(null);
  const [error, setError] = useState<unknown>(null);
  const [busy, setBusy] = useState(false);
  const [cleared, setCleared] = useState(false);
  const change = (config: Config) => { current.current = config; setDraft(config); setCleared(false); setError(null); };
  const persist = (config = current.current): Promise<void> => {
    const task = pending.current.catch(() => undefined).then(async () => {
      if (JSON.stringify(config) === JSON.stringify(saved.current)) return;
      const next = await api.setWeatherCache(config.directory, config.max_size_gb);
      saved.current = next.weather_cache;
      if (current.current === config) change(next.weather_cache);
      onSettings(next);
    });
    pending.current = task;
    return task;
  };
  useEffect(() => {
    flush.current = () => persist();
    return () => { flush.current = null; };
  });
  useEffect(() => {
    let live = true;
    api.weatherCacheStatus().then(value => { if (live) setStatus(value); }, failure => { if (live) setError(failure); });
    return () => { live = false; };
  }, [settings.weather_cache.directory, settings.weather_cache.max_size_gb]);
  const action = async (work: () => Promise<void>) => {
    setError(null); setBusy(true);
    try { await work(); } catch (failure) { setError(failure); } finally { setBusy(false); }
  };
  const browse = () => action(async () => {
    const directory = await pickWeatherCacheDirectory();
    if (directory !== null) { const config = { ...current.current, directory }; change(config); await persist(config); }
  });
  const clear = () => action(async () => { await persist(); setStatus(await api.clearWeatherCache()); setCleared(true); });
  return <div className="weather-cache-settings">
    <p className="muted">{t("Reanalysis keeps downloaded chunks on disk for reuse across routes and app restarts. The least recently used chunks are removed when the cache is full.")}</p>
    <label className="settings-field">
      {t("Cache directory")}
      <input data-feature="settings:cache-directory" aria-label={t("Cache directory")} title={t("Leave empty to use the application cache directory")}
        placeholder={t("Application cache directory")} value={draft.directory}
        onChange={e => change({ ...current.current, directory: e.target.value })}
        onBlur={() => { void persist().catch(setError); }}
        onKeyDown={e => { if (e.key === "Enter") void action(() => persist()); }} />
    </label>
    <button data-feature="settings:cache-browse" title={t("Choose cache directory")} disabled={busy} onClick={() => void browse()}>{t("Choose cache directory")}</button>
    <label className="settings-field">
      {t("Maximum cache size (GB)")}
      <IntegerField data-feature="settings:cache-size" aria-label={t("Maximum cache size (GB)")} title={t("Maximum cache size (GB)")}
        value={draft.max_size_gb} min={1} max={4096} onCommit={max_size_gb => {
          const config = { ...current.current, max_size_gb }; change(config); void action(() => persist(config));
        }} />
    </label>
    {status && <p className="muted" title={status.directory}>{t("Cached on disk: {size}", { size: formatBytes(status.bytes) })}</p>}
    <button data-feature="settings:cache-clear" title={t("Remove cached reanalysis downloads. Weather saved in projects is kept.")} disabled={busy} onClick={() => void clear()}>{t("Clear cache")}</button>
    {cleared && <p role="status">{t("Cache cleared")}</p>}
    {error !== null && <p className="modal-error" role="alert" title={describeError(error).detail}>{describeError(error).text}</p>}
  </div>;
}
