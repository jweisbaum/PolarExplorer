import { useEffect, useId, useRef, useState } from "react";
import { chordText, IS_MAC } from "../chords";
import { later, reportError } from "../hint";
import { msg, useT } from "../i18n";
import { searchFeatures, type Feature } from "./features";
import { locateFeature, locateSource } from "./highlight";
import { matchingSources, SOURCE_KINDS, useSearchSources } from "../panels/sourceSearch";
import type { SourceSummary } from "../generated/SourceSummary";
import { openHelp } from "./open";
import { searchTopics, type HelpTopic } from "./topics";

const present = (id: string) => document.querySelector(`[data-feature="${CSS.escape(id)}"]`) !== null;

/** How many of each kind the dropdown lists. */
const FEATURE_LIMIT = 8;
const TOPIC_LIMIT = 4;

type Result = { kind: "source"; source: SourceSummary; boat: number } | { kind: "feature"; feature: Feature } | { kind: "topic"; topic: HelpTopic } | { kind: "reference" };

/** Whether a key event is the search chord: Cmd-F on a Mac, Ctrl-F elsewhere. */
export function isSearchChord(event: Pick<KeyboardEvent, "key" | "metaKey" | "ctrlKey" | "altKey" | "shiftKey">,
  mac: boolean = IS_MAC): boolean {
  if (event.key.toLowerCase() !== "f" || event.altKey || event.shiftKey) return false;
  return mac ? event.metaKey && !event.ctrlKey : event.ctrlKey && !event.metaKey;
}

/**
 * The title bar's help search (spec.md 3.6), copied from VectorEffects' Help
 * menu: a search over the interface's features and the help reference, in
 * the language on screen, with results **as the user types**. Choosing a
 * feature runs its reveal steps and flashes an orange rectangle around it;
 * choosing a page opens the reference there. Cmd-F (Ctrl-F) focuses it; the
 * "?" beside it opens the reference.
 */
export default function HelpMenu() {
  const t = useT();
  const included = useSearchSources();
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const id = useId();
  const container = useRef<HTMLDivElement>(null);
  const input = useRef<HTMLInputElement>(null);
  const chord = chordText(["accel", "F"]);

  useEffect(() => {
    const key = (event: KeyboardEvent) => {
      if (!isSearchChord(event)) return;
      event.preventDefault();
      event.stopImmediatePropagation();
      // The reference, when it is up, has a search of its own.
      const reference = document.querySelector<HTMLInputElement>(".help-dialog input[type=search]");
      if (reference) { reference.focus(); reference.select(); return; }
      setOpen(true);
      input.current?.focus();
      input.current?.select();
    };
    window.addEventListener("keydown", key, true);
    return () => window.removeEventListener("keydown", key, true);
  }, []);

  useEffect(() => {
    if (!open) return;
    const outside = (event: Event) => {
      if (event.target instanceof Node && !container.current?.contains(event.target)) setOpen(false);
    };
    document.addEventListener("pointerdown", outside);
    document.addEventListener("focusin", outside);
    return () => {
      document.removeEventListener("pointerdown", outside);
      document.removeEventListener("focusin", outside);
    };
  }, [open]);

  // A feature with no way on screen from here would only lead to its help
  // page, which the pages below already do. One shown only while something
  // runs (Cancel fetch) is found where it appears: its landing.
  const features = searchFeatures(query)
    .filter(m => m.feature.reveal?.length || present(m.feature.landing ?? m.feature.id))
    .slice(0, FEATURE_LIMIT).map(m => ({ kind: "feature", feature: m.feature }) as const);
  const topics = query.trim() ? searchTopics(query).slice(0, TOPIC_LIMIT).map(topic => ({ kind: "topic", topic }) as const) : [];
  const sources = included && query.trim() ? matchingSources(included.sources, query, t).slice(0, FEATURE_LIMIT)
    .map(source => ({ kind: "source", source, boat: included.boat }) as const) : [];
  const results: Result[] = [...sources, ...features, ...topics, { kind: "reference" }];
  const selected = Math.min(active, results.length - 1);

  const choose = (result: Result) => {
    setOpen(false);
    setQuery("");
    setActive(0);
    input.current?.blur();
    if (result.kind === "reference") { openHelp(); return; }
    if (result.kind === "topic") { openHelp(result.topic.id); return; }
    if (result.kind === "source") { void locateSource(result.boat, result.source.id); return; }
    const feature = result.feature;
    void locateFeature(feature).then(found => {
      if (found) return;
      if (feature.topic) openHelp(feature.topic);
      else reportError(later(msg("{feature} is not available here. Open a project to use it."), { feature: later(feature.label) }));
    });
  };

  const optionId = (index: number) => `${id}-option-${index}`;
  const row = (result: Result, index: number) => {
    const props = {
      id: optionId(index), role: "option", "aria-selected": index === selected,
      className: index === selected ? "active" : undefined,
      onPointerMove: () => setActive(index),
      onPointerDown: (event: React.PointerEvent) => event.preventDefault(),
      onClick: () => choose(result),
    } as const;
    if (result.kind === "source") {
      return <li key={`s:${result.boat}:${result.source.id}`} {...props}>
        <span className="help-search-label">{result.source.label}</span>
        <span className="help-search-detail">{t(SOURCE_KINDS[result.source.kind]?.name ?? result.source.kind)}</span>
      </li>;
    }
    if (result.kind === "feature") {
      return <li key={`f:${result.feature.id}`} data-feature-target={result.feature.id} {...props}>
        <span className="help-search-label">{t(result.feature.label)}</span>
        {result.feature.description && <span className="help-search-detail">{t(result.feature.description)}</span>}
      </li>;
    }
    if (result.kind === "topic") {
      return <li key={`t:${result.topic.id}`} {...props}>
        <span className="help-search-label">{result.topic.title}</span>
        <span className="help-search-detail">{result.topic.group}</span>
      </li>;
    }
    return <li key="reference" {...props}>
      <span className="help-search-label">{t("Open the help reference")}</span>
      <span className="help-search-detail">F1</span>
    </li>;
  };

  return <div className="help-menu" ref={container}>
    <button className="help-button" data-feature="shell:help" title={t("Open the help reference (F1)")}
      aria-label={t("Help")} onClick={() => openHelp()}>?</button>
    <input ref={input} type="search" role="combobox" data-feature="shell:search"
      aria-expanded={open} aria-controls={`${id}-list`} aria-activedescendant={open ? optionId(selected) : undefined}
      aria-label={t("Search sources, features and help")} title={t("Search sources, features and help ({chord})", { chord })}
      placeholder={t("Search… ({chord})", { chord })} value={query}
      onFocus={() => setOpen(true)}
      onChange={event => { setQuery(event.target.value); setActive(0); setOpen(true); }}
      onKeyDown={event => {
        event.stopPropagation();
        if (event.key === "ArrowDown") { event.preventDefault(); setOpen(true); setActive((selected + 1) % results.length); }
        else if (event.key === "ArrowUp") { event.preventDefault(); setActive((selected + results.length - 1) % results.length); }
        else if (event.key === "Enter") { event.preventDefault(); const r = results[selected]; if (r) choose(r); }
        else if (event.key === "Escape") { event.preventDefault(); setOpen(false); event.currentTarget.blur(); }
      }} />
    {open && query.trim() !== "" && <div className="help-menu-popup">
      <ul id={`${id}-list`} role="listbox" aria-label={t("Search results")}>
        {sources.length > 0 && <li role="presentation" className="help-search-heading">{t("Sources")}</li>}
        {sources.map((result, index) => row(result, index))}
        {features.length > 0 && <li role="presentation" className="help-search-heading">{t("Features")}</li>}
        {features.map((result, index) => row(result, sources.length + index))}
        {topics.length > 0 && <li role="presentation" className="help-search-heading">{t("Help pages")}</li>}
        {topics.map((result, index) => row(result, sources.length + features.length + index))}
        {sources.length === 0 && features.length === 0 && topics.length === 0
          && <li role="presentation" className="help-search-empty">{t("Nothing matches “{query}”.", { query })}</li>}
        {row({ kind: "reference" }, results.length - 1)}
      </ul>
    </div>}
  </div>;
}
