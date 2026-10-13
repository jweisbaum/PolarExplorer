import { msg } from "../../i18n";
import type { Feature } from "../features";

/**
 * The 2D polar plot's own controls (spec.md 9.2), on the 2D stage
 * (`stage:2d`, registered in `shell.ts`).
 */
const inPanel = ["stage:2d"];

const features: Feature[] = [
  { id: "plot:all", label: msg("All wind speeds"), description: msg("Show every wind speed a source has, instead of one slice."),
    keywords: ["TWS", msg("all")], topic: "sources", reveal: inPanel },
  { id: "plot:tws", label: msg("Wind speed slice"), description: msg("The true wind speed the polar plot slices at."),
    keywords: ["TWS", msg("slider"), msg("wind speed")], topic: "sources", reveal: inPanel },
  { id: "plot:tws-range", label: msg("Wind speed range"), description: msg("Show the polar over a selected range of true wind speeds."),
    keywords: ["TWS", msg("range"), msg("slider")], topic: "sources", reveal: inPanel },
  { id: "plot:display", label: msg("Plot display"), description: msg("Show the polar curves or probability density."),
    keywords: [msg("PDF"), msg("probability density"), msg("polar")], topic: "sources", reveal: inPanel },
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
