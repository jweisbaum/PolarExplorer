import type { ProjectSummary } from "../generated/ProjectSummary";
import type { SpeedUnit } from "../generated/SpeedUnit";
import { useT } from "../i18n";
import type { PanelState } from "./layout";
import Section from "./Section";
import SourceList, { type SoloState } from "./SourceList";

/**
 * The right panel (spec.md 3.2): the source list (§8). The 2D polar plot
 * is a stage of its own (§9.2) since 2026-10-02.
 */
export default function RightPanel({ project, onProject, panels, onToggle, solo, onSolo, speedUnit = "kn" }: {
  project: ProjectSummary;
  onProject: (project: ProjectSummary) => void;
  panels: PanelState;
  onToggle: (panel: keyof PanelState) => void;
  /** The display speed unit (Settings), for the source list's text. */
  speedUnit?: SpeedUnit;
  solo: SoloState;
  onSolo: (solo: SoloState) => void;
}) {
  const t = useT();
  return (
    <div className="right-panel">
      <Section feature="panel:sources" title={t("Sources")} tooltip={t("Every source, with its colour, visibility and weight")}
        open={panels.sources} onToggle={() => onToggle("sources")}>
        <SourceList unit={speedUnit} project={project} onProject={onProject} solo={solo} onSolo={onSolo} />
      </Section>
    </div>
  );
}
