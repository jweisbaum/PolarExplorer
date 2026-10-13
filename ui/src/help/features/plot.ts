import { msg } from "../../i18n";
import type { Feature } from "../features";

/**
 * The 2D polar plot's own controls (spec.md 9.2), on the 2D stage
 * (`stage:2d`, registered in `shell.ts`).
 */
const inPanel = ["stage:2d"];

const features: Feature[] = [
  { id: "plot:lasso", label: msg("Lasso selection"), description: msg("Draw round dots to select them; Shift adds to the selection"),
    keywords: [msg("select"), msg("freehand")], topic: "sources", reveal: inPanel },
  { id: "plot:box", label: msg("Box selection"), description: msg("Drag a box round dots to select them; Shift adds to the selection"),
    keywords: [msg("select"), msg("rectangle")], topic: "sources", reveal: inPanel },
  { id: "plot:exclude", label: msg("Exclude from the blend"), description: msg("Remove the selected dots from the blend. Undo puts them back."),
    keywords: [msg("remove"), msg("outlier"), msg("selection")], topic: "sources", reveal: inPanel },
  { id: "plot:include", label: msg("Include in the blend"), description: msg("Put excluded dots back into the blend."),
    keywords: [msg("restore"), msg("selection")], topic: "sources", reveal: inPanel },
  { id: "plot:clear-selection", label: msg("Clear the selection"), description: msg("Select nothing (Escape)"),
    keywords: [msg("deselect"), "Esc"], topic: "sources", reveal: inPanel },
  { id: "plot:wave-height-move", label: msg("Move wave height range"), description: msg("Drag to move both limits. Arrow keys move the range; Home and End move it to either end."), keywords: [msg("filters"), msg("samples"), msg("waves")], topic: "sources", reveal: inPanel },
  { id: "plot:wave-angle-move", label: msg("Move wave angle range"), description: msg("Drag to move both limits. Arrow keys move the range; Home and End move it to either end."), keywords: [msg("filters"), msg("samples"), msg("waves")], topic: "sources", reveal: inPanel },
  { id: "plot:wave-period-move", label: msg("Move wave period range"), description: msg("Drag to move both limits. Arrow keys move the range; Home and End move it to either end."), keywords: [msg("filters"), msg("samples"), msg("waves")], topic: "sources", reveal: inPanel },
  { id: "plot:wave-height-min", label: msg("Minimum wave height shown"), description: msg("Filter track samples and update the blend in both views. These ranges apply in addition to other filters."), keywords: [msg("filters"), msg("samples"), msg("waves")], topic: "sources", reveal: inPanel },
  { id: "plot:wave-height-max", label: msg("Maximum wave height shown"), description: msg("Filter track samples and update the blend in both views. These ranges apply in addition to other filters."), keywords: [msg("filters"), msg("samples"), msg("waves")], topic: "sources", reveal: inPanel },
  { id: "plot:wave-angle-min", label: msg("Minimum wave angle shown"), description: msg("Filter track samples and update the blend in both views. These ranges apply in addition to other filters."), keywords: [msg("filters"), msg("samples"), msg("waves")], topic: "sources", reveal: inPanel },
  { id: "plot:wave-angle-max", label: msg("Maximum wave angle shown"), description: msg("Filter track samples and update the blend in both views. These ranges apply in addition to other filters."), keywords: [msg("filters"), msg("samples"), msg("waves")], topic: "sources", reveal: inPanel },
  { id: "plot:wave-period-min", label: msg("Minimum wave period shown"), description: msg("Filter track samples and update the blend in both views. These ranges apply in addition to other filters."), keywords: [msg("filters"), msg("samples"), msg("waves")], topic: "sources", reveal: inPanel },
  { id: "plot:wave-period-max", label: msg("Maximum wave period shown"), description: msg("Filter track samples and update the blend in both views. These ranges apply in addition to other filters."), keywords: [msg("filters"), msg("samples"), msg("waves")], topic: "sources", reveal: inPanel },
  { id: "plot:wave-ranges-reset", label: msg("Reset ranges"), description: msg("Filter track samples and update the blend in both views. These ranges apply in addition to other filters."), keywords: [msg("filters"), msg("samples"), msg("waves")], topic: "sources", reveal: inPanel },

  { id: "plot:all", label: msg("All wind speeds"), description: msg("Show every wind speed a source has, instead of one slice."),
    keywords: ["TWS", msg("all")], topic: "sources", reveal: inPanel },
  { id: "plot:tws", label: msg("Wind speed slice"), description: msg("The true wind speed the polar plot slices at."),
    keywords: ["TWS", msg("slider"), msg("wind speed")], topic: "sources", reveal: inPanel },
  { id: "plot:tws-range", label: msg("Wind speed range"), description: msg("Show the polar over a selected range of true wind speeds."),
    keywords: ["TWS", msg("range"), msg("slider")], topic: "sources", reveal: inPanel },
  { id: "plot:display", label: msg("Plot display"), description: msg("Show polars, PDFs, or dots with PDFs."),
    keywords: [msg("PDF"), msg("probability density"), msg("polar"), msg("Points and PDF")], topic: "sources", reveal: inPanel },
  { id: "plot:show-filtered", label: msg("Show filtered samples in the plot"), description: msg("Also draw the samples the filters take out, dimmed."),
    keywords: [msg("filters"), msg("dots"), msg("dimmed")], topic: "sources", reveal: inPanel },
  { id: "plot:show-excluded", label: msg("Show excluded samples in the plot"), description: msg("Also draw the samples you excluded, hollow."),
    keywords: [msg("excluded"), msg("outliers"), msg("dots")], topic: "sources", reveal: inPanel },
  { id: "plot:colour", label: msg("Colour plot dots by"), description: msg("Colour the plot's sample dots by their track or by the time of day they were sailed at."),
    keywords: [msg("colour"), msg("night"), msg("morning"), msg("time of day")], topic: "sources", reveal: inPanel },
  { id: "plot:measure", label: msg("Measure on the polar plot"),
    description: msg("Compare every curve's boat speed at one wind angle, and measure from a pinned point to the pointer."),
    keywords: [msg("ruler"), msg("compare"), msg("difference"), "BSP", msg("speed")], topic: "sources", reveal: inPanel },
];

export default features;
