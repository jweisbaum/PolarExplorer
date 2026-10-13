import { useCallback, useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import type { ProjectSummary } from "../generated/ProjectSummary";
import type { AppSettings } from "../generated/AppSettings";
import type { BoatTabs } from "../generated/BoatTabs";
import type { BoatExportResult } from "../generated/BoatExportResult";
import { api, boatApi } from "../ipc";
import { reportFailure } from "../errors";
import { useT } from "../i18n";
import { onReveal } from "../help/highlight";
import { onShowBoat } from "../mcp/follow";
import { pickExportDirectory, pickLockedProjectExportPath, pickTabExportPath, pickTabToImport } from "../project/dialogs";
import { BoatProvider } from "./context";
import BoatWorkspace from "./BoatWorkspace";
import { FleetSyncProvider } from "./synchronization";
import type { BoatViewState } from "./synchronization";
import ConfirmDialog from "../project/ConfirmDialog";

type Layout = 1 | 2 | 4;

/** One, two or four panes, drawn: the layout switch's icons. */
function LayoutIcon({ panes }: { panes: Layout }) {
  return <svg viewBox="0 0 16 16" width="16" height="16" aria-hidden="true" fill="none" stroke="currentColor" strokeWidth="1.5">
    <rect x="1.75" y="2.75" width="12.5" height="10.5" rx="1.5" />
    {panes >= 2 && <line x1="8" y1="2.75" x2="8" y2="13.25" />}
    {panes === 4 && <line x1="1.75" y1="8" x2="14.25" y2="8" />}
  </svg>;
}

export default function FleetWorkspace({ project, settings, onSettings, onProject, changedBoat, onActive, toolbarHost, statusHost, onReplace }: {
  project: ProjectSummary; settings: AppSettings | null; onSettings: (settings: AppSettings) => void;
  onProject: (project: ProjectSummary) => void; changedBoat: ProjectSummary | null; onActive: (id: number) => void;
  /** The title bar's place for Add Tab and the layout switch. */
  toolbarHost: HTMLElement | null;
  /** The status bar's place for Export all tabs, beside the version. */
  statusHost?: HTMLElement | null;
  onReplace: (previousId: number, project: ProjectSummary) => void;
}) {
  const t = useT();
  const [tabs, setTabs] = useState<BoatTabs | null>(null);
  const [summaries, setSummaries] = useState<Record<number, ProjectSummary>>({ [project.id]: project });
  const [active, setActive] = useState(project.id);
  const [layout, setLayout] = useState<Layout>(1);
  const viewStates = useRef(new Map<number, BoatViewState>());
  useEffect(() => onReveal("boats:tabs", () => setLayout(1)), []);
  const [panes, setPanes] = useState<number[]>([project.id]);
  const [renaming, setRenaming] = useState<{ id: number; name: string } | null>(null);
  const [deleting, setDeleting] = useState<{ id: number; name: string } | null>(null);
  const [changingBoats, setChangingBoats] = useState(false);
  const [exporting, setExporting] = useState(false);
  const [format, setFormat] = useState("whirlwind_matrix_polar");
  const readOnlyProject = format === "read_only_project";
  const [lockedProjectPath, setLockedProjectPath] = useState<string | null>(null);
  const [result, setResult] = useState<BoatExportResult | null>(null);
  const [busy, setBusy] = useState(false);
  const rootId = useRef(project.id);
  rootId.current = project.id;
  const removed = useRef(new Set<number>());
  const tabsRequest = useRef(0);
  /** The boats of the latest tabs read, and that read: what a failed reload asks before it is reported. */
  const tabIds = useRef<Set<number> | null>(null);
  const lastRefresh = useRef<Promise<void>>(Promise.resolve());
  const merge = useCallback((next: ProjectSummary) => setSummaries(current =>
    !removed.current.has(next.id) && (!current[next.id] || current[next.id]!.revision <= next.revision) ? { ...current, [next.id]: next } : current), []);
  const refreshTabs = useCallback(() => {
    const request = ++tabsRequest.current;
    const done = api.boatTabs().then(value => {
      if (request === tabsRequest.current && value?.tabs && value.project_id === rootId.current) {
        tabIds.current = new Set(value.tabs.map(tab => tab.id));
        setTabs(value);
      }
    }).catch(reportFailure);
    lastRefresh.current = done;
    return done;
  }, []);
  useEffect(() => { merge(project); void refreshTabs(); }, [project, merge, refreshTabs]);
  useEffect(() => { if (changedBoat) { merge(changedBoat); void refreshTabs(); } }, [changedBoat, merge, refreshTabs]);
  const rows: BoatTabs["tabs"] = tabs?.tabs ?? [{ id: project.id, name: project.boat_name || project.name, details: {}, read_only: false }];
  const shown = layout === 1 ? [active] : panes.slice(0, layout);
  const load = (id: number) => boatApi(id).projectSummary().then(value => { if (value) merge(value); }).catch(reportFailure);
  const loaded = useRef<number[]>([]);
  loaded.current = Object.keys(summaries).map(Number);
  useEffect(() => {
    // A child write also advances the root revision. Refresh cached child
    // summaries after weather writes and save/undo, without a second event listener.
    for (const id of loaded.current) if (id !== project.id) void boatApi(id).projectSummary().then(value => { if (value) merge(value); }).catch(error => {
      // A boat that went with this very change (the MCP service's
      // boat_remove) has no summary to read, and that is not a failure:
      // the tabs, read again for the same change, say whether it is there.
      void lastRefresh.current.then(() => { if (tabIds.current?.has(id) ?? true) reportFailure(error); });
    });
  }, [project.revision, project.dirty, project.id, merge]);
  const select = (id: number) => { setActive(id); onActive(id); setRenaming(null); void load(id); };
  // The MCP service's `view://boat`: show a boat's tab, as a click on it would.
  const selectRef = useRef(select);
  selectRef.current = select;
  const activeRef = useRef(active);
  activeRef.current = active;
  useEffect(() => onShowBoat(id => { setLayout(1); selectRef.current(id); }), []);
  // The tabs are what boats there are. One can go or come back without this
  // component having done it (the MCP service's boat_remove and
  // boat_restore): drop the view of a boat that is gone, stop refusing one
  // that is back, and show another when the one on show went.
  useEffect(() => {
    if (!tabs) return;
    const ids = new Set(tabs.tabs.map(tab => tab.id));
    for (const id of [...removed.current]) if (ids.has(id)) removed.current.delete(id);
    setSummaries(current => Object.keys(current).every(id => ids.has(Number(id))) ? current
      : Object.fromEntries(Object.entries(current).filter(([id]) => ids.has(Number(id)))));
    setPanes(current => current.every(id => ids.has(id)) ? current : current.filter(id => ids.has(id)));
    if (!ids.has(activeRef.current) && tabs.tabs[0]) selectRef.current(tabs.tabs[0].id);
    // Only when the tabs were read again: a boat just added here is on show
    // before the tabs that name it arrive.
  }, [tabs]);
  const update = (next: ProjectSummary) => {
    merge(next);
    void refreshTabs();
    if (next.id === project.id) onProject(next);
    else void api.projectSummary().then(root => { if (root) onProject(root); }).catch(reportFailure);
  };
  const chooseLayout = (next: Layout) => {
    setLayout(next);
    if (next === 1) return;
    const ids = [active, ...rows.map(row => row.id).filter(id => id !== active)].slice(0, next);
    setPanes(ids);
    for (const id of ids) void load(id);
  };
  const add = async () => {
    setChangingBoats(true);
    try {
      const next = await api.addBoat(t("Polar {number}", { number: rows.length + 1 }));
      update(next); select(next.id);
      if (layout > 1) setPanes(current => current.length < layout ? [...current, next.id] : current);
    } catch (error) { reportFailure(error); }
    finally { setChangingBoats(false); }
  };
  const commitName = () => {
    const edit = renaming; setRenaming(null);
    if (edit?.name.trim()) void boatApi(edit.id).renameBoat(edit.name.trim()).then(update).catch(reportFailure);
  };
  const changedStructure = (next: ProjectSummary) => {
    if (next.id === project.id) update(next);
    else {
      // Promoting/restoring the first boat changes the file's root identity,
      // not the fleet opening. Keep sibling workspaces and comparison state.
      rootId.current = next.id;
      merge(next);
      onReplace(project.id, next);
      void refreshTabs();
    }
  };
  const remove = async () => {
    if (!deleting) return;
    const id = deleting.id;
    setDeleting(null); setChangingBoats(true);
    try {
      const next = await api.deleteBoat(project.id, id);
      removed.current.add(id);
      setSummaries(current => { const remaining = { ...current }; delete remaining[id]; return remaining; });
      const remaining = rows.filter(row => row.id !== id);
      const nextPanes = [...new Set([...panes.filter(value => value !== id), ...remaining.map(row => row.id)])].slice(0, layout);
      setPanes(nextPanes); for (const value of nextPanes) void load(value);
      if (active === id) select(remaining[0]!.id);
      changedStructure(next);
    } catch (error) { reportFailure(error); }
    finally { setChangingBoats(false); }
  };
  const restore = async () => {
    setChangingBoats(true);
    try {
      const next = await api.restoreBoat(project.id);
      removed.current.clear();
      changedStructure(next);
    } catch (error) { reportFailure(error); }
    finally { setChangingBoats(false); }
  };
  const exportAll = async () => {
    setBusy(true);
    try {
      if (readOnlyProject) {
        const path = await pickLockedProjectExportPath(project.name || "Project");
        if (path) setLockedProjectPath(await api.exportLockedProject(path));
      } else {
        const directory = await pickExportDirectory();
        if (directory) setResult(await api.exportAllPolars(directory, format));
      }
    }
    catch (error) { reportFailure(error); }
    finally { setBusy(false); }
  };
  const viewStateFor = (id: number): BoatViewState => {
    let state = viewStates.current.get(id);
    if (!state) {
      state = { camera: null, layout: null };
      viewStates.current.set(id, state);
    }
    return state;
  };
  const importDroppedRef = useRef<(paths: string[]) => void>(() => {});
  const importPaths = async (paths: string[]) => {
    const petabs = paths.filter(path => path.toLowerCase().endsWith(".petab"));
    if (petabs.length === 0) return;
    setChangingBoats(true);
    try {
      for (const path of petabs) {
        try {
          const next = await api.importTab(path);
          update(next);
          select(next.id);
          if (layout > 1) setPanes(current => current.length < layout
            ? [...current, next.id]
            : current.map((id, slot) => slot === layout - 1 ? next.id : id));
        } catch (error) { reportFailure(error); }
      }
    } finally { setChangingBoats(false); }
  };
  importDroppedRef.current = paths => { void importPaths(paths); };
  useEffect(() => {
    let live = true;
    let unlisten: (() => void) | undefined;
    try {
      void getCurrentWebview().onDragDropEvent(event => {
        if (event.payload.type === "drop") importDroppedRef.current(event.payload.paths);
      }).then(off => {
        if (live) unlisten = off;
        else off();
      }).catch(() => undefined);
    } catch { /* The component also runs in the browser-only preview. */ }
    return () => { live = false; unlisten?.(); };
  }, []);
  const importTab = async () => {
    const path = await pickTabToImport();
    if (path) await importPaths([path]);
  };
  const exportTab = async () => {
    const name = rows.find(row => row.id === active)?.name ?? "Polar";
    const path = await pickTabExportPath(name);
    if (!path) return;
    setBusy(true);
    try { await boatApi(active).exportTab(path); }
    catch (error) { reportFailure(error); }
    finally { setBusy(false); }
  };
  const activeReadOnly = rows.find(row => row.id === active)?.read_only ?? false;
  return <div className="fleet-workspace">
    {toolbarHost && createPortal(<>
      <button data-feature="boats:add" disabled={changingBoats} onClick={() => void add()}>{t("Add Tab")}</button>
      <button data-feature="boats:import-tab" disabled={changingBoats} title={t("Import a read-only tab from a .petab file")}
        onClick={() => void importTab()}>{t("Import Tab")}</button>
      <button data-feature="boats:export-tab" disabled={busy} title={t("Export this tab as a portable .petab file")}
        onClick={() => void exportTab()}>{t("Export Tab")}</button>
      {/* Icons, named in their tooltips: one pane, two, four. */}
      <div className="layout-switch" role="group" aria-label={t("Boat comparison layout")}>
        <button data-feature="boats:single" aria-pressed={layout === 1} aria-label={t("Single view")} title={t("Single view")}
          onClick={() => chooseLayout(1)}><LayoutIcon panes={1} /></button>
        <button data-feature="boats:split" aria-pressed={layout === 2} aria-label={t("Split view")} title={t("Split view")}
          onClick={() => chooseLayout(2)}><LayoutIcon panes={2} /></button>
        <button data-feature="boats:four" aria-pressed={layout === 4} aria-label={t("Four-way view")} title={t("Four-way view")}
          onClick={() => chooseLayout(4)}><LayoutIcon panes={4} /></button>
      </div>
    </>, toolbarHost)}
    {statusHost && createPortal(<>
      {activeReadOnly && <span className="read-only-status" data-feature="boats:read-only">{t("Read only")}</span>}
      {rows.length > 1 && <button className="small" data-feature="boats:export-all" onClick={() => { setResult(null); setLockedProjectPath(null); setExporting(true); }}>{t("Export all tabs")}</button>}
    </>, statusHost)}
    {layout === 1 && <div className="boat-tabs-row">
      <div className="boat-tabs" role="tablist" aria-label={t("Boats")}>
        {rows.map(row => <div key={row.id} role="presentation" className={`boat-tab${active === row.id ? " selected" : ""}${row.read_only ? " read-only" : ""}`}>
          {renaming?.id === row.id
            ? <input className="boat-tab-name" value={renaming.name} autoFocus aria-label={t("Boat name")} data-feature="boats:name"
              onFocus={event => event.currentTarget.select()} onChange={event => setRenaming({ id: row.id, name: event.target.value })} onBlur={commitName}
              onKeyDown={event => { event.stopPropagation(); if (event.key === "Enter") event.currentTarget.blur(); if (event.key === "Escape") setRenaming(null); }} />
            : <button role="tab" aria-selected={active === row.id} data-feature="boats:tab" title={row.read_only ? t("Read-only tab") : t("Double-click to rename the boat")}
              onClick={() => select(row.id)} onDoubleClick={() => { if (!row.read_only) { select(row.id); setRenaming({ id: row.id, name: row.name }); } }}
              onKeyDown={event => { if (event.key === "F2" && !row.read_only) { event.preventDefault(); select(row.id); setRenaming({ id: row.id, name: row.name }); } }}>{row.name}</button>}
          {/* Closing a tab deletes its polar, so it asks first; the last one cannot go. */}
          <button className="boat-tab-close" data-feature="boats:delete" disabled={changingBoats || rows.length < 2}
            aria-label={t("Delete {boat}", { boat: row.name })}
            title={rows.length < 2 ? t("A project must contain at least one polar") : t("Delete {boat}", { boat: row.name })}
            onClick={() => setDeleting(row)}>×</button>
        </div>)}
      </div>
      {tabs?.can_restore && <button className="small" data-feature="boats:restore" disabled={changingBoats} onClick={() => void restore()}>{t("Undo Delete Polar")}</button>}
    </div>}
    <FleetSyncProvider enabled={layout > 1}>
      <div className={`boat-grid boats-${layout}`}>
        {Object.values(summaries).map(summary => {
          const slot = shown.indexOf(summary.id);
          return <section key={summary.id} className={`boat-pane${active === summary.id ? " active" : ""}`} hidden={slot < 0}
            style={{ order: slot }} data-boat-id={summary.id} onPointerDownCapture={() => { if (active !== summary.id) select(summary.id); }}>
            {layout > 1 && slot >= 0 && <select className="boat-pane-picker" data-feature="boats:pane" aria-label={t("Boat in comparison pane")}
              value={summary.id} onChange={event => {
                const id = Number(event.target.value);
                setPanes(current => current.map((old, i) => i === slot ? id : old === id ? summary.id : old));
                select(id);
              }}>{rows.map(row => <option key={row.id} value={row.id}>{row.name}</option>)}</select>}
            <BoatProvider id={summary.id} active={active === summary.id && slot >= 0}><BoatWorkspace project={summary} settings={settings} onSettings={onSettings}
              onProject={update} split={layout > 1} visible={slot >= 0} active={active === summary.id && slot >= 0}
              viewState={viewStateFor(summary.id)} /></BoatProvider>
          </section>;
        })}
        {Array.from({ length: Math.max(0, layout - shown.length) }, (_, i) => <div className="boat-pane empty" key={`empty-${i}`} style={{ order: shown.length + i }}>
          <p>{t("Add another polar to compare.")}</p><button data-feature="boats:add-empty" disabled={changingBoats} onClick={() => void add()}>{t("Add Tab")}</button>
        </div>)}
      </div>
    </FleetSyncProvider>
    {deleting && <ConfirmDialog title={t("Delete {boat}", { boat: deleting.name })}
      body={t("Remove this polar and its sources from the project? You can undo this deletion while the project remains open.")}
      confirmLabel={t("Delete Polar")} onConfirm={() => void remove()} onCancel={() => setDeleting(null)} />}
    {exporting && <div className="modal-backdrop"><section className="modal boat-export-dialog" role="dialog" aria-modal="true" aria-label={t("Export all tabs")}>
      <h2>{t("Export all tabs")}</h2>
      <label className="boat-export-format">{t("Format")} <select data-feature="boats:export-format" value={format} disabled={busy}
        onChange={event => { setFormat(event.target.value); setResult(null); setLockedProjectPath(null); }}>
        <option value="whirlwind_matrix_polar">{t("WhirlwindMatrixPolar (.wmpolar)")}</option>
        <option value="expedition">{t("Expedition")}</option><option value="adrena">{t("Adrena")}</option><option value="csv">{t("CSV")}</option>
        <option value="qtvlm">{t("qtVlm")}</option><option value="opencpn">{t("OpenCPN")}</option><option value="luckgrib">{t("LuckGrib")}</option>
        <option value="read_only_project">{t("Read-only project")}</option>
      </select></label>
      {lockedProjectPath && <p role="status">{t("Exported read-only project to {path}.", { path: lockedProjectPath })}</p>}
      {result && <><p role="status">{t("Exported {count} boat polars.", { count: result.paths.length })}</p>
        {result.failures.length > 0 && <ul>{result.failures.map(([name, error], i) => <li key={i}>{name}: {error}</li>)}</ul>}</>}
      <div className="modal-actions"><button data-feature="boats:export-close" disabled={busy} onClick={() => setExporting(false)}>{t("Close")}</button>
        <button className="primary" data-feature="boats:export-confirm" disabled={busy} onClick={() => void exportAll()}>{readOnlyProject ? t("Export project…") : t("Choose export folder…")}</button></div>
    </section></div>}
  </div>;
}
