import { msg } from "../../i18n";
import type { Feature } from "../features";
const tabs = msg("Each boat has independent sources, filters, edits and views. Add a boat beside the project name. Double-click a tab or press F2 to edit its name.");
const deletion = msg("The × on a tab deletes that polar and its sources, after asking. Keep at least one polar; Undo Delete Polar, at the end of the tab row, restores removed tabs while this project stays open.");
const compare = msg("Compare two or four boats in 3D with linked rotation and hover at corresponding wind conditions.");
const exporting = msg("Export all tabs writes one polar file per tab. Choose Read-only project to export every tab together as a .pelocked project that can be viewed but not edited; this does not lock the tabs in the current project.");
const petab = msg("Export Tab saves this tab as a portable .petab archive with every polar, filter, track fix and weather value. Import Tab or drop a .petab file into the project to add it as a locked, read-only tab.");
const tracker = msg("Open a YellowBrick, Geovoile or Blue Water link as one tab per boat. Match polars and tracks by identical model or exact vessel identity, and review the original boat details. Names alone are insufficient.");
const features: Feature[] = [
  { id: "boats:tab", label: msg("Boats"), description: tabs, keywords: ["boat", "tabs"], topic: "projects", reveal: ["boats:tabs"] },
  { id: "boats:add", label: msg("Add Tab"), description: tabs, keywords: ["boat", "tabs"], topic: "projects" },
  { id: "boats:import-tab", label: msg("Import Tab"), description: petab, keywords: ["boat", "tabs", "petab", "read only", "import"], topic: "projects", landing: "boats:add" },
  { id: "boats:export-tab", label: msg("Export Tab"), description: petab, keywords: ["boat", "tabs", "petab", "export"], topic: "projects", landing: "boats:add" },
  { id: "boats:read-only", label: msg("Read only"), description: petab, keywords: ["boat", "tabs", "locked"], topic: "projects", reveal: ["boats:tabs"] },
  { id: "boats:name", label: msg("Boat name"), description: tabs, keywords: ["boat", "tabs", "rename"], topic: "projects", reveal: ["boats:tabs"], landing: "boats:tab" },
  { id: "boats:delete", label: msg("Delete Polar"), description: deletion, keywords: ["boat", "remove", "delete", "close"], topic: "projects", reveal: ["boats:tabs"] },
  { id: "boats:restore", label: msg("Undo Delete Polar"), description: deletion, keywords: ["boat", "restore", "undo"], topic: "projects", reveal: ["boats:tabs"], landing: "boats:delete" },
  { id: "boats:single", label: msg("Single view"), description: compare, keywords: ["boat", "compare"], topic: "workspace" },
  { id: "boats:split", label: msg("Split view"), description: compare, keywords: ["boat", "compare"], topic: "workspace" },
  { id: "boats:four", label: msg("Four-way view"), description: compare, keywords: ["boat", "compare"], topic: "workspace" },
  { id: "boats:comparison-controls", label: msg("Filters and display"), description: compare, keywords: ["boat", "compare"], topic: "workspace", landing: "boats:split" },
  { id: "boats:pane", label: msg("Boat in comparison pane"), description: compare, keywords: ["boat", "compare"], topic: "workspace", landing: "boats:split" },
  { id: "boats:add-empty", label: msg("Add another polar to compare."), description: compare, keywords: ["boat", "compare"], topic: "workspace", landing: "boats:add" },
  { id: "boats:export-all", label: msg("Export all tabs"), description: exporting, keywords: ["boat", "export"], topic: "blend", landing: "boats:add" },
  { id: "boats:export-format", label: msg("Export format"), description: exporting, keywords: ["boat", "export"], topic: "blend", landing: "boats:add" },
  { id: "boats:export-close", label: msg("Close"), description: exporting, keywords: ["boat", "export"], topic: "blend", landing: "boats:add" },
  { id: "boats:export-confirm", label: msg("Choose export folder…"), description: exporting, keywords: ["boat", "export"], topic: "blend", landing: "boats:add" },
  { id: "start:tracker", label: msg("Open project from tracker…"), description: tracker, keywords: ["YellowBrick", "Blue Water"], topic: "projects" },
  { id: "project:tracker", label: msg("Open project from tracker…"), description: tracker, keywords: ["YellowBrick", "Blue Water"], topic: "projects", reveal: ["menu:project"] },
  ...["provider", "url", "match", "details", "class", "classes-all", "open", "cancel", "close", "warnings"].map(id => ({ id: `boats:tracker-${id}`, label: msg("Open tracker project"), description: tracker, keywords: ["YellowBrick", "Blue Water", "class", "division"], topic: "projects", reveal: ["menu:project"], landing: "project:tracker" })),
];
export default features;
