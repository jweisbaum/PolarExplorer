import { msg } from "../../i18n";
import type { Feature } from "../features";

/** The title bar, the stage switcher, the dock tabs and the status bar (spec.md 3.2). */
const menu = ["menu:project"];

const features: Feature[] = [
  { id: "shell:project-menu", label: msg("Project"), description: msg("New, open, save and close projects."),
    keywords: [msg("file"), msg("menu")], topic: "projects" },
  { id: "project:new", label: msg("New…"), description: msg("Create a new project, asking first about unsaved changes."),
    keywords: [msg("new project"), msg("create")], topic: "projects", reveal: menu },
  { id: "project:open", label: msg("Open…"), description: msg("Open a saved project file."),
    keywords: [msg("open project"), msg("load")], topic: "projects", reveal: menu },
  { id: "project:open-recent", label: msg("Open Recent"), description: msg("Open one of the ten most recent projects."),
    keywords: [msg("recent projects"), msg("history")], topic: "projects", reveal: menu },
  { id: "project:save", label: msg("Save"), description: msg("Save the project to its file."),
    keywords: [msg("save project"), msg("store")], topic: "projects", reveal: menu },
  { id: "project:save-as", label: msg("Save As…"), description: msg("Save the project to a new file."),
    keywords: [msg("save a copy"), msg("duplicate")], topic: "projects", reveal: menu },
  { id: "project:close", label: msg("Close"), description: msg("Close the project and return to the start screen."),
    keywords: [msg("close project"), msg("start screen")], topic: "projects", reveal: menu },
  { id: "shell:rename", label: msg("Project name"), description: msg("Click the project's name in the title bar to rename it."),
    keywords: [msg("rename"), msg("title")], topic: "projects" },
  { id: "shell:search", label: msg("Search"), description: msg("Find included tracks, polars, controls and help pages as you type."),
    keywords: [msg("find"), msg("feature search")], topic: "search" },
  { id: "shell:help", label: msg("Help"), description: msg("Open the help reference."),
    keywords: [msg("manual"), msg("documentation")], topic: "search" },
  { id: "shell:settings", label: msg("Settings"), description: msg("Language, theme, units, autosave, cache and network."),
    keywords: [msg("preferences"), msg("options"), msg("gear")], topic: "settings" },

  { id: "stage:map", label: msg("Map"), description: msg("Show the world map in the centre."),
    keywords: [msg("stage"), msg("chart"), msg("world")], topic: "map" },
  { id: "stage:3d", label: msg("3D"), description: msg("Show the polar as a 3D surface in the centre."),
    keywords: [msg("stage"), msg("surface"), msg("three dimensions")], topic: "polar-3d" },
  { id: "stage:2d", label: msg("2D"), description: msg("Show the polar plot in the centre: boat speed against wind angle, one wind speed or all."),
    keywords: [msg("stage"), msg("polar diagram"), msg("polar plot"), "TWA", "BSP"], topic: "sources" },
  { id: "stage:compare", label: msg("Compare"), description: msg("Show the difference between two polars in the centre."),
    keywords: [msg("stage"), msg("difference")], topic: "compare" },

  { id: "dock:left", label: msg("Navigation panel"), description: msg("Show or hide the navigation on the left."),
    keywords: [msg("sidebar"), msg("fold"), msg("sources to add")], topic: "workspace" },
  { id: "dock:right", label: msg("Sources and polar plot panel"), description: msg("Show or hide the sources and the polar plot on the right."),
    keywords: [msg("sidebar"), msg("fold")], topic: "workspace" },

  { id: "shell:statusbar", label: msg("Status bar"), description: msg("Shows hints, errors and work in progress."),
    keywords: [msg("hint"), msg("error"), msg("progress")], topic: "workspace" },
  { id: "shell:mcp-badge", label: msg("MCP client badge"),
    description: msg("Shown in the status bar while an AI client is connected through the MCP service, with the last tool it called."),
    keywords: ["MCP", msg("AI"), msg("agent"), msg("connected")], topic: "settings", landing: "shell:statusbar" },
  { id: "shell:cancel-fetch", label: msg("Cancel the fetch"), description: msg("While wind, waves and current are being fetched, stop every fetch; the samples already fetched are kept."),
    keywords: [msg("stop"), msg("reanalysis"), msg("job"), msg("progress")], topic: "environment", landing: "shell:statusbar" },
  { id: "shell:scrape-status", label: msg("Scrape progress"), description: msg("While a scrape you started runs, how far through the races it is."),
    keywords: [msg("scrape"), msg("progress"), msg("library")], topic: "settings", landing: "shell:statusbar" },
  { id: "shell:cancel-scrape", label: msg("Cancel scrape"), description: msg("Cancel the scrape; races already saved stay in the library"),
    keywords: [msg("stop"), msg("scrape"), msg("library")], topic: "settings", landing: "shell:statusbar" },
];

export default features;
