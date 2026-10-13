import { useSyncExternalStore } from "react";
import type { SourceSummary } from "../generated/SourceSummary";
import { fold, msg } from "../i18n";

export const SOURCE_KINDS: Readonly<Record<string, { glyph: string; name: string }>> = {
  orr: { glyph: "◆", name: msg("ORR polar") },
  orc: { glyph: "◆", name: msg("ORC polar") },
  polar_file: { glyph: "▦", name: msg("Polar file") },
  track: { glyph: "〰", name: msg("Track") },
};

/** Match included sources, including hidden ones, by name or import metadata. */
export function matchingSources(sources: readonly SourceSummary[], query: string, t: (key: string) => string): SourceSummary[] {
  const terms = fold(query).trim().split(/\s+/).filter(Boolean);
  return sources.filter(source => {
    const certificate = source.orc ?? source.orr;
    const text = fold([
      source.label, source.kind, t(SOURCE_KINDS[source.kind]?.name ?? source.kind),
      source.polar_file?.file_name, source.polar_file?.format,
      certificate?.sail_no, certificate?.model, certificate?.year, certificate?.certificate_year,
      source.track?.boat_name, source.track?.event_title, source.track?.origin,
    ].filter(value => value !== null && value !== undefined).join(" "));
    return terms.every(term => text.includes(term));
  });
}

// The titlebar search lives outside the boat providers. Only the active tab
// publishes here; updates do not duplicate project state or alter visibility.
type SearchSources = { boat: number; sources: readonly SourceSummary[] } | null;
let current: SearchSources = null;
const listeners = new Set<() => void>();
export function publishSearchSources(next: SearchSources): void {
  current = next;
  for (const notify of listeners) notify();
}
export function useSearchSources(): SearchSources {
  return useSyncExternalStore(
    notify => { listeners.add(notify); return () => { listeners.delete(notify); }; },
    () => current,
  );
}
