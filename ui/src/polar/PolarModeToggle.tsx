import { useBoatApi } from "../boats/context";
import { useState } from "react";

import { reportFailure } from "../errors";
import type { ProjectSummary } from "../generated/ProjectSummary";
import { useT } from "../i18n";


/** Apply the mode and its output-axis edit together, through the undoable settings command. */
export default function PolarModeToggle({ project, onProject }: {
  project: ProjectSummary;
  onProject: (project: ProjectSummary) => void;
}) {
  const api = useBoatApi();
  const t = useT();
  const [busy, setBusy] = useState(false);
  const changeMode = async (asymmetric: boolean) => {
    setBusy(true);
    const blend = project.blend;
    const angles = asymmetric
      ? [...blend.twa, ...blend.twa.map((angle) => 360 - angle)]
      : blend.twa.map((angle) => angle > 180 ? 360 - angle : angle);
    try {
      onProject(await api.setBlendSettings({
        twa: [...new Set(angles)].sort((a, b) => a - b), tws: blend.tws,
        min_samples: blend.min_samples, n_full: blend.n_full,
        smoothing: blend.smoothing, default_statistic: blend.default_statistic, polar_statistic: blend.polar_statistic,
        use_corrected: project.use_corrected,
        asymmetric, interpolation: blend.interpolation,
      }));
    } catch (error) {
      reportFailure(error);
    } finally {
      setBusy(false);
    }
  };

  return <label className="polar-mode" title={t("Keep port and starboard separate in plots, blending and export.")}>
    <input type="checkbox" data-feature="shell:asymmetric" checked={project.blend.asymmetric}
      disabled={busy} onChange={(event) => void changeMode(event.target.checked)} />
    {t("Asymmetric polar")}
  </label>;
}
