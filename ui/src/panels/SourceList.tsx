import { useBoatApi, useBoatReadOnly } from "../boats/context";
import { useEffect, useRef, useState, type KeyboardEvent } from "react";

import { needsOutline, themeColour } from "../colourContrast";
import { reportFailure } from "../errors";
import type { ProjectSummary } from "../generated/ProjectSummary";
import type { SourceSummary } from "../generated/SourceSummary";
import { useBoatReveal } from "../boats/context";
import { useT } from "../i18n";

import { compareSource } from "../compare/compareState";
import { useBoatEditing } from "../polar/editFocus";
import { onThemeChange } from "../settings/themes";
import BlendCorrectionDialog from "./BlendCorrectionDialog";
import BlendSettingsDialog from "./BlendSettingsDialog";
import ExportDialog from "./ExportDialog";
import PALETTE from "./palette.json";

import { matchingSources, SOURCE_KINDS } from "./sourceSearch";

let gestures = 0;

/** Kept by the tab so folding the source panel does not lose Solo's restore point. */
export type SoloState = { id: number; before: number[] } | null;

/**
 * The source list (spec.md 8): the Blend entry at the top — its colour, a
 * show or hide switch, its coverage (cells with direct evidence and cells
 * filled), Blend settings and Export — then every source
 * with its colour (a palette of sixteen plus a custom picker), a show or hide
 * switch, its label (renamed in place), kind, count and blend weight, and
 * Edit, Compare and Remove. Rows reorder by dragging the handle, or with the
 * arrow keys on it. Every change is one undoable command in Rust; the list
 * shows whatever summary comes back.
 */
export default function SourceList({ project, onProject, solo, onSolo: setSolo, unit = "kn" }: {
  unit?: import("../generated/SpeedUnit").SpeedUnit;
  project: ProjectSummary;
  onProject: (project: ProjectSummary) => void;
  solo: SoloState;
  onSolo: (solo: SoloState) => void;
}) {
  const { editSource } = useBoatEditing();
  const api = useBoatApi();
  const readOnly = useBoatReadOnly();
  const onReveal = useBoatReveal();
  const t = useT();
  const [picking, setPicking] = useState<number | null>(null);
  const [pickingBlend, setPickingBlend] = useState(false);
  const [dialog, setDialog] = useState<"settings" | "export" | "correction" | null>(null);
  // The blend swatch's outline depends on the theme's background.
  const [, setThemeTick] = useState(0);
  useEffect(() => {
    const redraw = () => setThemeTick((tick) => tick + 1);
    // Once now too: the theme may have changed between render and here.
    redraw();
    return onThemeChange(redraw);
  }, []);
  const [renaming, setRenaming] = useState<{ id: number; text: string } | null>(null);
  const [dragging, setDragging] = useState<number | null>(null);
  const [weights, setWeights] = useState<Record<number, number>>({});
  const gesture = useRef<string | null>(null);
  const sources = project.sources;
  const [query, setQuery] = useState("");
  const matching = matchingSources(sources, query, t);
  const [visibilityBusy, setVisibilityBusy] = useState(false);
  const visibilityPending = useRef(false);
  useEffect(() => onReveal("sources:reset-search", () => setQuery("")), [onReveal]);
  useEffect(() => { setQuery(""); }, [project.id]);
  useEffect(() => {
    // Undo and external changes to visibility end Solo, rather than leaving a
    // pressed button that no longer describes what is shown.
    if (!visibilityPending.current && solo &&
      (!sources.some(source => source.id === solo.id) || sources.some(source => source.visible !== (source.id === solo.id)))) setSolo(null);
  }, [sources, solo, visibilityBusy]);
  const changeVisibility = async (change: () => Promise<ProjectSummary>, nextSolo: typeof solo = null) => {
    if (readOnly || visibilityPending.current) return;
    visibilityPending.current = true;
    setVisibilityBusy(true);
    try {
      const next = await change();
      setSolo(nextSolo);
      onProject(next);
    } catch (error) { reportFailure(error); }
    finally { visibilityPending.current = false; setVisibilityBusy(false); }
  };
  const setVisible = (ids: number[], nextSolo: typeof solo = null) => changeVisibility(() => api.setVisibleSources(ids), nextSolo);
  const selectMatching = (visible: boolean) => {
    const matched = new Set(matching.map(source => source.id));
    void setVisible(sources.filter(source => matched.has(source.id) ? visible : source.visible).map(source => source.id));
  };
  const toggleSolo = (source: SourceSummary) => {
    if (solo?.id === source.id) {
      const existing = new Set(sources.map(item => item.id));
      void setVisible(solo.before.filter(id => existing.has(id)));
    } else {
      void setVisible([source.id], { id: source.id, before: solo?.before ?? sources.filter(item => item.visible).map(item => item.id) });
    }
  };
  const blend = project.blend;
  const first = useRef<number | null>(null);
  first.current = sources[0]?.id ?? null;

  // The search's reveal step for the colour picker (spec.md 3.6): open it on
  // the first source.
  useEffect(() => onReveal("sources:colours", () => setPicking(first.current)), []);

  const run = (change: Promise<ProjectSummary>) => change.then(onProject).catch(reportFailure);

  const commitRename = () => {
    if (!renaming) return;
    const source = sources.find((s) => s.id === renaming.id);
    const text = renaming.text.trim();
    setRenaming(null);
    if (source && text.length > 0 && text !== source.label) void run(api.setSourceLabel(source.id, text));
  };

  const setWeight = (source: SourceSummary, weight: number) => {
    setWeights((current) => ({ ...current, [source.id]: weight }));
    void run(api.setSourceWeight(source.id, weight, gesture.current));
  };
  const endWeight = (source: SourceSummary) => {
    gesture.current = null;
    setWeights((current) => {
      const next = { ...current };
      delete next[source.id];
      return next;
    });
  };

  const move = (source: SourceSummary, to: number) => {
    if (readOnly || to < 0 || to >= sources.length) return;
    void run(api.moveSource(source.id, to));
  };
  const onHandleKey = (event: KeyboardEvent, source: SourceSummary, index: number) => {
    if (event.key === "ArrowUp" || event.key === "ArrowDown") {
      event.preventDefault();
      move(source, index + (event.key === "ArrowUp" ? -1 : 1));
    }
  };

  return (
    <ul className="source-list">
      <li className="source-list-tools">
        <input type="search" data-feature="sources:search" aria-label={t("Search included tracks and polars")}
          placeholder={t("Search included tracks and polars")} value={query} onChange={event => setQuery(event.target.value)} />
        <div className="source-line">
          <button className="small" data-feature="sources:select-all" disabled={readOnly || visibilityBusy || !matching.some(source => !source.visible)}
            title={t("Show all matching sources")} onClick={() => selectMatching(true)}>{t("Select all")}</button>
          <button className="small" data-feature="sources:select-none" disabled={readOnly || visibilityBusy || !matching.some(source => source.visible)}
            title={t("Hide all matching sources")} onClick={() => selectMatching(false)}>{t("Select none")}</button>
        </div>
      </li>
      {/* The blend (spec.md 8, 12). */}
      <li className={["source-row", "blend-row", blend.visible ? "" : "hidden-source"].join(" ").trim()}>
        <div className="source-line">
          <button data-feature="sources:blend-colour" disabled={readOnly}
            // A colour too close to the background gets a contrasting outline.
            className={needsOutline(blend.colour, themeColour("--surface", "#253447")) ? "swatch-button outlined" : "swatch-button"}
            style={{ backgroundColor: blend.colour }} aria-expanded={pickingBlend}
            title={t("Change the blend's colour")} aria-label={t("Blend colour")}
            onClick={() => setPickingBlend(!pickingBlend)} />
          <input type="checkbox" checked={blend.visible} data-feature="sources:blend-visible" disabled={readOnly}
            aria-label={t("Show the blend")}
            title={t("Show or hide the blend in every plot. Export is not affected.")}
            onChange={(event) => void run(api.setBlendVisible(event.target.checked))} />
          <span className="source-label blend-label">{t("Blend")}</span>
          <span className="muted source-count"
            title={t("{direct} cells have direct evidence from a source, {filled} are filled between them (and the 0° row), {empty} stay empty.", { direct: blend.direct, filled: blend.filled, empty: blend.empty })}>
            {t("{direct} direct, {filled} filled", { direct: blend.direct, filled: blend.filled })}
          </span>
        </div>
        <div className="source-line">
          <button className="small" data-feature="sources:blend-settings" disabled={readOnly}
            title={t("The output grid and how sources are blended")}
            onClick={() => setDialog("settings")}>
            {t("Blend settings")}
          </button>
          <button className="small" data-feature="sources:blend-edit" disabled={readOnly} onClick={() => setDialog("correction")}>
            {t("Correct blend")}
          </button>
          <button className="small" data-feature="sources:export"
            title={t("Write the blend as an Expedition, Adrena or CSV polar")}
            onClick={() => setDialog("export")}>
            {t("Export…")}
          </button>
        </div>
        {pickingBlend && (
          <ColourPicker colour={blend.colour} onClose={() => setPickingBlend(false)}
            onChoose={(colour) => {
              if (colour !== blend.colour) void run(api.setBlendColour(colour));
            }} />
        )}
        {dialog === "settings" && (
          <BlendSettingsDialog project={project} onProject={onProject} onClose={() => setDialog(null)} />
        )}
        {dialog === "correction" && <BlendCorrectionDialog unit={unit} project={project} onProject={onProject} onClose={() => setDialog(null)} />}
        {dialog === "export" && <ExportDialog project={project} onClose={() => setDialog(null)} />}
      </li>
      {sources.length === 0 && <li className="muted placeholder">{t("No sources yet.")}</li>}
      {sources.length > 0 && matching.length === 0 && <li className="muted placeholder">{t("Nothing matches “{query}”.", { query })}</li>}
      {matching.map(source => {
        const index = sources.indexOf(source);
        const kind = SOURCE_KINDS[source.kind];
        const weight = weights[source.id] ?? source.weight;
        return (
          <li key={source.id} data-source-id={source.id} tabIndex={-1}
            className={["source-row", source.visible ? "" : "hidden-source", dragging === source.id ? "dragging" : ""].join(" ").trim()}
            onDragOver={(event) => { if (dragging !== null) event.preventDefault(); }}
            onDrop={(event) => {
              event.preventDefault();
              const moved = sources.find((s) => s.id === dragging);
              setDragging(null);
              if (moved && moved.id !== source.id) move(moved, index);
            }}>
            <div className="source-line">
              <span className="drag-handle" draggable={!readOnly} tabIndex={readOnly ? -1 : 0} data-feature="sources:reorder"
                title={t("Drag to reorder the list, or press the up and down arrow keys")}
                aria-label={t("Reorder")}
                onDragStart={(event) => {
                  event.dataTransfer?.setData("text/plain", String(source.id));
                  setDragging(source.id);
                }}
                onDragEnd={() => setDragging(null)}
                onKeyDown={(event) => onHandleKey(event, source, index)}>
                ⠿
              </span>
              <button className="swatch-button" data-feature="sources:colour" disabled={readOnly}
                style={{ backgroundColor: source.colour }} aria-expanded={picking === source.id}
                title={t("Change the colour")} aria-label={t("Colour")}
                onClick={() => setPicking(picking === source.id ? null : source.id)} />
              <input type="checkbox" checked={source.visible} data-feature="sources:visible" disabled={readOnly || visibilityBusy}
                title={t("Show or hide this source. A hidden source is left out of the blend and every plot.")}
                aria-label={t("Visible")}
                onChange={(event) => { const visible = event.target.checked; void changeVisibility(() => api.setSourceVisible(source.id, visible)); }} />
              <span className="kind-icon" title={kind ? t(kind.name) : source.kind}
                aria-label={kind ? t(kind.name) : source.kind}>{kind?.glyph ?? "?"}</span>
              {renaming?.id === source.id
                ? <input className="source-rename" autoFocus value={renaming.text} data-feature="sources:rename" disabled={readOnly}
                  aria-label={t("Source name")} title={t("Type a new name; Enter to keep it, Esc to cancel")}
                  onChange={(event) => setRenaming({ id: source.id, text: event.target.value })}
                  onBlur={commitRename}
                  onKeyDown={(event) => {
                    if (event.key === "Enter") commitRename();
                    else if (event.key === "Escape") setRenaming(null);
                  }} />
                : <button className="source-label" data-feature="sources:rename" disabled={readOnly}
                  title={t("Click to rename")}
                  onClick={() => setRenaming({ id: source.id, text: source.label })}>
                  {source.label}
                </button>}
              <span className="muted source-count">
                {source.used === null
                  ? t("{count} cells", { count: source.count })
                  : t("{used}/{count} samples", { used: source.used, count: source.count })}
              </span>
            </div>
            <div className="source-line">
              <span className="source-weight-label muted">{t("Weight")}</span>
              <input type="range" min={0} max={1} step={0.05} value={weight} data-feature="sources:weight" disabled={readOnly}
                aria-label={t("Weight")} title={t("Blend weight, 0 to 1. 1 is the default.")}
                onPointerDown={() => { gestures += 1; gesture.current = `drag-${gestures}`; }}
                onPointerUp={() => endWeight(source)}
                onBlur={() => endWeight(source)}
                onChange={(event) => setWeight(source, Number(event.target.value))} />
              <span className="source-weight">{weight.toFixed(2)}</span>
              <button className="small" data-feature="sources:solo" aria-pressed={solo?.id === source.id}
                disabled={readOnly || visibilityBusy} title={t("Show only this source; click again to restore previous visibility")}
                onClick={() => toggleSolo(source)}>{t("Solo")}</button>
              <button className="small" data-feature="sources:edit" disabled={readOnly}
                title={t("Edit this source's polar in the 3D view, with its table")}
                onClick={() => editSource(source.id)}>
                {t("Edit")}{source.edits > 0 && <span className="muted" title={t("{count} edits", { count: source.edits })}> ✎</span>}
              </button>
              <button className="small" data-feature="sources:compare"
                title={t("Compare this source with the blend in the Compare stage; pick another operand there")}
                onClick={() => compareSource(project, source.id)}>{t("Compare@@verb")}</button>
              <button className="small" data-feature="sources:remove" disabled={readOnly}
                title={t("Remove this source from the project (undoable)")}
                onClick={() => void run(api.removeSource(source.id))}>{t("Remove")}</button>
            </div>
            {picking === source.id && (
              <ColourPicker colour={source.colour} onClose={() => setPicking(null)}
                onChoose={(colour) => {
                  if (colour !== source.colour) void run(api.setSourceColour(source.id, colour));
                }} />
            )}
          </li>
        );
      })}
    </ul>
  );
}

/**
 * The colour popover: the sixteen palette colours new sources take, and a
 * custom picker. A palette colour applies and closes; the custom picker
 * applies when the system picker is closed, so its drag is one change.
 */
function ColourPicker({ colour, onChoose, onClose }: {
  colour: string;
  onChoose: (colour: string) => void;
  onClose: () => void;
}) {
  const t = useT();
  const custom = useRef<HTMLInputElement>(null);
  const box = useRef<HTMLDivElement>(null);
  const choose = useRef(onChoose);
  choose.current = onChoose;
  const close = useRef(onClose);
  close.current = onClose;

  useEffect(() => {
    const input = custom.current;
    // React's onChange is the DOM's `input`, which fires on every movement
    // of the system picker; `change` fires once, when it is closed.
    const commit = () => { if (input) choose.current(input.value); };
    input?.addEventListener("change", commit);
    const onKey = (event: globalThis.KeyboardEvent) => { if (event.key === "Escape") close.current(); };
    const onDown = (event: MouseEvent) => {
      const target = event.target as Node | null;
      if (box.current && target && !box.current.contains(target)
        && !(target instanceof HTMLElement && target.closest(".swatch-button"))) close.current();
    };
    window.addEventListener("keydown", onKey);
    window.addEventListener("mousedown", onDown);
    return () => {
      input?.removeEventListener("change", commit);
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("mousedown", onDown);
    };
  }, []);

  return (
    <div className="colour-popover" ref={box} role="dialog" aria-label={t("Source colour")}>
      <div className="palette" data-feature="sources:palette">
        {PALETTE.map((option) => (
          <button key={option} className={option === colour ? "palette-swatch selected" : "palette-swatch"}
            style={{ backgroundColor: option }} aria-pressed={option === colour} title={option} aria-label={option}
            onClick={() => { onChoose(option); onClose(); }} />
        ))}
      </div>
      <label className="custom-colour">
        {t("Custom colour")}
        <input ref={custom} type="color" defaultValue={colour} data-feature="sources:custom-colour"
          title={t("Pick any colour")} />
      </label>
    </div>
  );
}
