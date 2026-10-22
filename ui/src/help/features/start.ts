import { msg } from "../../i18n";
import type { Feature } from "../features";

/** The start screen (spec.md 3.1) and the new-project form it shares with the dialog. */
const features: Feature[] = [
  { id: "start:language", label: msg("Language"), description: msg("Choose the language of the interface and help."),
    keywords: [msg("translation"), msg("French"), msg("German"), msg("English"), msg("Spanish"), msg("Italian"), msg("Dutch"), msg("Chinese"), msg("Japanese"), msg("Arabic")], topic: "settings" },
  { id: "start:help", label: msg("Help"), description: msg("Open the help reference."),
    keywords: [msg("manual"), msg("documentation")], topic: "search" },
  { id: "start:settings", label: msg("Settings"), description: msg("Language, theme, units, autosave and reanalysis."),
    keywords: [msg("preferences"), msg("options")], topic: "settings" },
  { id: "start:new", label: msg("New project"), description: msg("Name the project, then create it."),
    keywords: [msg("create"), msg("start")], topic: "projects" },
  { id: "new:name", label: msg("Project name"), description: msg("The new project's name."),
    keywords: [msg("title")], topic: "projects" },
  { id: "new:create", label: msg("Create project"), description: msg("Create the project and open it."),
    keywords: [msg("new project"), msg("start")], topic: "projects" },
  { id: "start:browse", label: msg("Open…"), description: msg("Open a saved project file."),
    keywords: [msg("open project"), msg("load"), msg("browse")], topic: "projects" },
  { id: "start:recent", label: msg("Recent projects"), description: msg("The ten projects opened or saved most recently."),
    keywords: [msg("history"), msg("open recent")], topic: "projects" },
  { id: "start:clear-recent", label: msg("Clear"), description: msg("Forget the recent projects. The projects themselves are not deleted."),
    keywords: [msg("recent projects"), msg("forget")], topic: "projects" },
  { id: "start:recover", label: msg("Recovered work"), description: msg("Unsaved work kept when PolarExplorer did not close cleanly."),
    keywords: [msg("crash"), msg("autosave"), msg("restore")], topic: "projects" },
];

export default features;
