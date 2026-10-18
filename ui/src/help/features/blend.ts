import { msg } from "../../i18n";
import type { Feature } from "../features";

/**
 * The Blend settings and Export dialogs (spec.md 12). Their controls exist
 * only while a dialog is open, so each lands on the button that opens it
 * (`landing`), and its description says so.
 */
const list = ["section:sources"];
const settings = { topic: "blend", reveal: list, landing: "sources:blend-settings" };
const exporting = { topic: "blend", reveal: list, landing: "sources:export" };

const matrixDescription = msg("Choose the matrix modes and the number and bounds of wave bins in either export dialog.");
const matrixFeatures: Feature[] = [
  ["toggle", msg("Matrix data modes")],
  ["sog", msg("Boat speed (SOG)")], ["histogram", msg("Histogram (sampled PDF)")],
  ["gaussian", msg("Gaussian PDF")], ["waves", msg("Split by wave conditions")],
  ["speed-bins", msg("Boat-speed bins (0–60 kn)")],
  ["height-cutoffs", msg("Wave height cutoffs (m)")], ["period-cutoffs", msg("Wave period cutoffs (s)")],
  ["directions", msg("Wave directions")],
  ["customize", msg("Customize wave buckets (optional)")], ["reset", msg("Reset wave buckets")],
].map(([id, label]) => ({ id: `matrix-export:${id}`, label: label!, description: matrixDescription,
  keywords: [msg("export"), msg("Bins"), "WhirlwindMatrixPolar"], ...exporting }));

const features: Feature[] = [
  ...matrixFeatures,
  { id: "blend-settings:twa", label: msg("Output grid wind angles"),
    description: msg("In Blend settings: the true wind angles the blend is made on, 0 to 180, at least 0.01 apart."),
    keywords: [msg("output grid"), "TWA", msg("axis")], ...settings },
  { id: "blend-settings:tws", label: msg("Output grid wind speeds"),
    description: msg("In Blend settings: the true wind speeds the blend is made on, 0 to 70 kn, at least 0.01 apart."),
    keywords: [msg("output grid"), "TWS", msg("axis")], ...settings },
  { id: "blend-settings:default-grid", label: msg("Default grid"),
    description: msg("In Blend settings: put back the output grid a new project starts with."),
    keywords: [msg("output grid"), msg("reset")], ...settings },
  { id: "blend-settings:polar-statistic", label: msg("Polar blend statistic"),
    description: msg("Combine visible polars per cell. Mean uses source weights; median and p90 interpolate sorted speeds. Track settings do not affect polar-only views."),
    keywords: [msg("percentile"), msg("median"), msg("mean")], ...settings },
  { id: "blend-settings:statistic", label: msg("Track statistic to use for blend"),
    description: msg("In Blend settings: the statistic used to calculate each track's polar values for the blend; new tracks start with this, and each track can be changed individually."),
    keywords: [msg("minimum"), msg("maximum"), msg("percentile"), msg("median"), msg("mean")], ...settings },
  { id: "blend-settings:min-samples", label: msg("Samples a cell needs"),
    description: msg("In Blend settings: a track cell with fewer samples than this has no value."),
    keywords: [msg("minimum"), msg("samples"), msg("cell")], ...settings },
  { id: "blend-settings:n-full", label: msg("Samples for full confidence"),
    description: msg("In Blend settings: a track cell counts by its samples over this number, at most fully."),
    keywords: [msg("confidence"), msg("weight"), msg("samples")], ...settings },
  { id: "blend-settings:use-corrected", label: msg("Correct for current (blend)"),
    description: msg("In Blend settings: feed the polar from boat speed and wind through the water."),
    keywords: [msg("current"), msg("tide")], ...settings },
  { id: "blend-settings:smoothing", label: msg("Smooth the blend"),
    description: msg("In Blend settings: smooth the blended grid once its empty cells are filled. Off by default."),
    keywords: [msg("smooth"), msg("blend")], ...settings },
  { id: "export:format", label: msg("Export format"),
    description: msg("Write the polar in this format"),
    keywords: [msg("export"), msg("format"), "WhirlwindMatrixPolar"], ...exporting },
  { id: "export:expedition", label: msg("Export as Expedition"),
    description: msg("In the export dialog: one row per wind speed with the angles that have a value (.txt)."),
    keywords: [msg("export"), "Expedition", msg("format")], ...exporting },
  { id: "export:adrena", label: msg("Export as Adrena"),
    description: msg("In the export dialog: a tab-separated TWA × TWS table (.pol)."),
    keywords: [msg("export"), "Adrena", msg("format")], ...exporting },
  { id: "export:csv", label: msg("Export as CSV"),
    description: msg("In the export dialog: a semicolon-separated TWA × TWS table (.csv)."),
    keywords: [msg("export"), "CSV", msg("format")], ...exporting },
  { id: "export:qtvlm", label: msg("Export as qtVlm"),
    description: msg("In the export dialog: a semicolon-separated TWA × TWS table for qtVlm (.csv)."),
    keywords: [msg("export"), "qtVlm", msg("format")], ...exporting },
  { id: "export:opencpn", label: msg("Export as OpenCPN"),
    description: msg("In the export dialog: a semicolon-separated TWA × TWS table for OpenCPN (.pol)."),
    keywords: [msg("export"), "OpenCPN", msg("format")], ...exporting },
  { id: "export:luckgrib", label: msg("Export as LuckGrib"),
    description: msg("In the export dialog: a tab-separated TWA × TWS table for LuckGrib (.tsv)."),
    keywords: [msg("export"), "LuckGrib", msg("format")], ...exporting },
  { id: "export:whirlwind-matrix-polar", label: msg("Export as WhirlwindMatrixPolar"),
    description: msg("In the export dialog: a compact, self-describing binary SOG matrix (.wmpolar)."),
    keywords: [msg("export"), "WhirlwindMatrixPolar", msg("format")], ...exporting },
  { id: "export:project-grid", label: msg("Export on the output grid"),
    description: msg("In the export dialog: write the blend on the project's output grid."),
    keywords: [msg("export"), msg("output grid")], ...exporting },
  { id: "export:custom-grid", label: msg("Export on custom axes"),
    description: msg("In the export dialog: write the blend read onto other axes, without extrapolating."),
    keywords: [msg("export"), msg("axis"), msg("resample")], ...exporting },
  { id: "export:custom-twa", label: msg("Custom wind angles"),
    description: msg("In the export dialog, with custom axes: the true wind angles to write."),
    keywords: [msg("export"), "TWA"], ...exporting },
  { id: "export:custom-tws", label: msg("Custom wind speeds"),
    description: msg("In the export dialog, with custom axes: the true wind speeds to write."),
    keywords: [msg("export"), "TWS"], ...exporting },
  { id: "export:preview", label: msg("Export preview"),
    description: msg("In the export dialog: the grid as it would be written, filled cells muted."),
    keywords: [msg("export"), msg("preview"), msg("table")], ...exporting },
];

export default features;
