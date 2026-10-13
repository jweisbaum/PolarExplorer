import { msg } from "../../i18n";
import type { Feature } from "../features";

/**
 * The 3D polar stage's controls (spec.md 10.1–10.3). They are on screen only
 * while the 3D stage is, so each reveals it first.
 */
const onStage = ["stage:3d"];
const topic = "polar-3d";

const features: Feature[] = [
  { id: "view3d:display", label: msg("3D display mode"), description: msg("Show polars, PDFs, or dots with PDFs."),
    keywords: [msg("PDF"), msg("probability density"), msg("distribution"), msg("Points and PDF")], topic, reveal: onStage },
  { id: "view3d:wave-height-move", label: msg("Move wave height range"), description: msg("Drag to move both limits. Arrow keys move the range; Home and End move it to either end."), keywords: [msg("filters"), msg("samples"), msg("waves")], topic, reveal: onStage },
  { id: "view3d:wave-angle-move", label: msg("Move wave angle range"), description: msg("Drag to move both limits. Arrow keys move the range; Home and End move it to either end."), keywords: [msg("filters"), msg("samples"), msg("waves")], topic, reveal: onStage },
  { id: "view3d:wave-period-move", label: msg("Move wave period range"), description: msg("Drag to move both limits. Arrow keys move the range; Home and End move it to either end."), keywords: [msg("filters"), msg("samples"), msg("waves")], topic, reveal: onStage },
  { id: "view3d:wave-height-min", label: msg("Minimum wave height shown"), description: msg("Filter track samples and update the blend in both views. These ranges apply in addition to other filters."), keywords: [msg("filters"), msg("samples"), msg("waves")], topic, reveal: onStage },
  { id: "view3d:wave-height-max", label: msg("Maximum wave height shown"), description: msg("Filter track samples and update the blend in both views. These ranges apply in addition to other filters."), keywords: [msg("filters"), msg("samples"), msg("waves")], topic, reveal: onStage },
  { id: "view3d:wave-angle-min", label: msg("Minimum wave angle shown"), description: msg("Filter track samples and update the blend in both views. These ranges apply in addition to other filters."), keywords: [msg("filters"), msg("samples"), msg("waves")], topic, reveal: onStage },
  { id: "view3d:wave-angle-max", label: msg("Maximum wave angle shown"), description: msg("Filter track samples and update the blend in both views. These ranges apply in addition to other filters."), keywords: [msg("filters"), msg("samples"), msg("waves")], topic, reveal: onStage },
  { id: "view3d:wave-period-min", label: msg("Minimum wave period shown"), description: msg("Filter track samples and update the blend in both views. These ranges apply in addition to other filters."), keywords: [msg("filters"), msg("samples"), msg("waves")], topic, reveal: onStage },
  { id: "view3d:wave-period-max", label: msg("Maximum wave period shown"), description: msg("Filter track samples and update the blend in both views. These ranges apply in addition to other filters."), keywords: [msg("filters"), msg("samples"), msg("waves")], topic, reveal: onStage },
  { id: "view3d:wave-ranges-reset", label: msg("Reset ranges"), description: msg("Filter track samples and update the blend in both views. These ranges apply in addition to other filters."), keywords: [msg("filters"), msg("samples"), msg("waves")], topic, reveal: onStage },

  { id: "view3d:global-filters", label: msg("Global point filters"), description: msg("Applied after each track's filters, including in the blend."), keywords: [msg("filters"), msg("track")], topic, reveal: onStage },
  { id: "view3d:global-filters-enabled", label: msg("Enable global filters"), description: msg("Applied after each track's filters, including in the blend."), keywords: [msg("filters"), msg("track")], topic, reveal: onStage, landing: "view3d:global-filters" },
  { id: "view3d:layout", label: msg("3D layout"), description: msg("Draw the polar as a tower (angle, radius, height) or on straight axes."),
    keywords: [msg("polar tower"), msg("Cartesian"), msg("axes")], topic, reveal: onStage },
  { id: "view3d:camera-top", label: msg("Top view"), description: msg("Look down the wind-speed axis: the classic polar diagram."),
    keywords: [msg("camera"), msg("preset"), msg("from above")], topic, reveal: onStage },
  { id: "view3d:camera-side", label: msg("Side view"), description: msg("Look across the wind-speed axis, each wind speed a level."),
    keywords: [msg("camera"), msg("preset"), "TWS"], topic, reveal: onStage },
  { id: "view3d:camera-iso", label: msg("Isometric view"), description: msg("The three-quarter view of the whole scene."),
    keywords: [msg("camera"), msg("preset"), msg("reset view")], topic, reveal: onStage },
  { id: "view3d:tool-rotate", label: msg("Rotate tool"), description: msg("Drag to turn the 3D view; click a dot to select it."),
    keywords: [msg("orbit"), msg("pan"), msg("zoom")], topic, reveal: onStage },
  { id: "view3d:tool-lasso", label: msg("Lasso selection"), description: msg("Draw round dots in the 3D view to select them."),
    keywords: [msg("select"), msg("freehand")], topic, reveal: onStage },
  { id: "view3d:tool-box", label: msg("Box selection"), description: msg("Drag a rectangle in the 3D view to select the dots inside."),
    keywords: [msg("select"), msg("rectangle")], topic, reveal: onStage },
  { id: "view3d:wave-split", label: msg("Split Wave Angle"),
    description: msg("In single view, draws one copy of the 3D view per wave direction as seen from the boat, each with only that direction’s samples and an arrow showing the waves against the boat. The copies turn together, and a point hovered in one is marked at the same wind in the others."),
    keywords: [msg("waves"), msg("direction"), msg("copies"), msg("Split the 3D view by wave direction")], topic, reveal: onStage },
  { id: "view3d:wave-split-count", label: msg("Number of wave directions"),
    description: msg("How many wave directions to split into: 4, 8, 16, 18, 24 or 36. One is always centred on the bow, so none begins or ends at 0°"),
    keywords: [msg("waves"), msg("direction")], topic, reveal: onStage, landing: "view3d:wave-split" },
  { id: "view3d:wave-split-sense", label: msg("Waves from or to"),
    description: msg("From: each copy is where the waves come from. To: where they go"),
    keywords: [msg("waves"), msg("direction")], topic, reveal: onStage, landing: "view3d:wave-split" },
  { id: "view3d:show-samples", label: msg("Show samples"), description: msg("Show every track sample as a dot."),
    keywords: [msg("dots"), msg("tracks")], topic, reveal: onStage },
  { id: "view3d:show-nodes", label: msg("Show polar nodes"), description: msg("Show the grid points of every visible polar source."),
    keywords: [msg("dots"), msg("grid points")], topic, reveal: onStage },
  { id: "view3d:show-surfaces", label: msg("Show surfaces"), description: msg("Show each visible polar source as a translucent surface."),
    keywords: [msg("mesh"), msg("polar")], topic, reveal: onStage },
  { id: "view3d:show-filtered", label: msg("Show filtered samples"), description: msg("Show the samples the filters remove, dimmed."),
    keywords: [msg("filters"), msg("dimmed")], topic, reveal: onStage },
  { id: "view3d:show-excluded", label: msg("Show excluded points"), description: msg("Show the samples and polar points you excluded."),
    keywords: [msg("excluded"), msg("outliers")], topic, reveal: onStage },
  { id: "view3d:colour", label: msg("Colour dots by"), description: msg("Colour the dots by source, wave height, period or angle, current speed, UTC time or time of day."),
    keywords: [msg("colour"), "Hs", msg("current"), msg("time"), msg("night"), msg("morning"), msg("time of day")], topic, reveal: onStage },
  { id: "view3d:exclude", label: msg("Exclude from the blend"), description: msg("Remove the selected dots from the blend. Undo puts them back."),
    keywords: [msg("remove"), msg("outlier"), msg("selection")], topic, reveal: onStage },
  { id: "view3d:include", label: msg("Include in the blend"), description: msg("Put excluded dots back into the blend."),
    keywords: [msg("restore"), msg("selection")], topic, reveal: onStage },
  { id: "view3d:show-on-map", label: msg("Show selection on the map"), description: msg("Highlight the selected samples on the map and frame them."),
    keywords: [msg("map"), msg("selection")], topic, reveal: onStage },
  { id: "view3d:clear-selection", label: msg("Clear the selection"), description: msg("Select nothing in the 3D view."),
    keywords: [msg("deselect"), "Esc"], topic, reveal: onStage },
];

export default features;
