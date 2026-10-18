import { msg } from "../../i18n";
import type { Feature } from "../features";

/**
 * The Settings dialog (spec.md 3.4). `settings:` opens it (App); the second
 * step brings the section into view once the dialog is mounted.
 */
const open = (section: string) => ["settings:", `settings:${section}`];

const features: Feature[] = [
  { id: "settings:mcp", label: msg("AI assistants"), description: msg("Connect once. Assistants can inspect, edit, calculate, download and export within your workflow."), keywords: ["MCP","Claude","Codex"], topic: "settings", reveal: open("mcp") },
  { id: "settings:mcp-clients", label: msg("Permission policy"), description: msg("Routine work automatically; ask before irreversible actions"), keywords: ["MCP",msg("AI")], topic: "settings", reveal: open("mcp") },
  { id: "settings:cache-directory", label: msg("Cache directory"), description: msg("Leave empty to use the application cache directory"),
    keywords: ["Whirlwind", msg("cache")], topic: "settings", reveal: open("cache") },
  { id: "settings:cache-browse", label: msg("Choose cache directory"), description: msg("Choose cache directory"),
    keywords: ["Whirlwind", msg("cache")], topic: "settings", reveal: open("cache") },
  { id: "settings:cache-size", label: msg("Maximum cache size (GB)"), description: msg("Maximum cache size (GB)"),
    keywords: ["Whirlwind", msg("cache")], topic: "settings", reveal: open("cache") },
  { id: "settings:cache-clear", label: msg("Clear cache"), description: msg("Remove cached Whirlwind downloads. Weather saved in projects is kept."),
    keywords: ["Whirlwind", msg("cache")], topic: "settings", reveal: open("cache") },
  { id: "settings:data-source", label: msg("Data sources"),
    description: msg("Choose the archive for historical wind, waves and currents."),
    keywords: ["Whirlwind", "Open Data", "R2", "Tigris", "S3", msg("weather"), msg("download")], topic: "settings", reveal: open("weather") },
  { id: "settings:theme", label: msg("Theme"), description: msg("Choose the colours of the whole application."),
    keywords: [msg("colours"), msg("appearance"), msg("dark mode")], topic: "settings", reveal: open("appearance") },
  { id: "settings:language", label: msg("Language"), description: msg("Choose the language of the interface and help."),
    keywords: [msg("translation"), msg("French"), msg("German"), msg("English"), msg("Spanish"), msg("Italian"), msg("Dutch"), msg("Chinese"), msg("Japanese"), msg("Arabic")], topic: "settings", reveal: open("appearance") },
  { id: "settings:speed-unit", label: msg("Boat and wind speed"), description: msg("The unit speeds are shown in: knots, m/s or km/h."),
    keywords: [msg("units"), msg("knots"), "BSP", "TWS"], topic: "settings", reveal: open("units") },
  { id: "settings:wave-unit", label: msg("Wave height"), description: msg("The unit wave heights are shown in: metres or feet."),
    keywords: [msg("units"), msg("waves"), msg("metres"), msg("feet")], topic: "settings", reveal: open("units") },
  { id: "settings:distance-unit", label: msg("Distance"), description: msg("The unit distances are shown in: nautical miles or kilometres."),
    keywords: [msg("units"), msg("nautical miles"), msg("kilometres")], topic: "settings", reveal: open("units") },
  { id: "settings:plot-band", label: msg("Polar plot dot band"),
    description: msg("How far from the polar plot's wind speed a track sample may be and still be drawn."),
    keywords: [msg("dots"), msg("samples"), "TWS", msg("polar diagram")], topic: "settings", reveal: open("units") },
  { id: "settings:autosave", label: msg("Autosave"), description: msg("What happens to unsaved work between saves."),
    keywords: [msg("recovery"), msg("backup"), msg("crash")], topic: "settings", reveal: open("autosave") },
  { id: "settings:weather-memory", label: msg("Weather kept in memory"),
    description: msg("How much downloaded wind, wave and current data is kept in memory for this session, so other boats of the same race do not download it again."),
    keywords: [msg("cache"), msg("memory"), msg("megabytes"), msg("download")], topic: "settings", reveal: open("weather") },
  { id: "settings:concurrency", label: msg("Concurrent requests"), description: msg("How many downloads run at once."),
    keywords: [msg("network"), msg("parallel"), msg("download")], topic: "settings", reveal: open("network") },
  { id: "settings:timeout", label: msg("Request timeout"), description: msg("How long one download may take before it is abandoned."),
    keywords: [msg("network"), msg("seconds"), msg("download")], topic: "settings", reveal: open("network") },
  { id: "settings:close", label: msg("Close the settings"), description: msg("Close the Settings dialog."),
    keywords: [msg("done")], topic: "settings", reveal: ["settings:"] },
  // The MCP service (spec.md 3.7).
  { id: "settings:mcp-enable", label: msg("MCP service"),
    description: msg("Let an AI client on this computer drive PolarExplorer. Off until you turn it on; nothing can reach it while it is off."),
    keywords: [msg("AI"), msg("agent"), "MCP", "Claude", "Codex", msg("assistant"), msg("automation")], topic: "settings", reveal: open("mcp") },
  { id: "settings:mcp-port", label: msg("MCP service port"), description: msg("The port on this computer the MCP service listens on."),
    keywords: ["MCP", msg("network"), msg("port")], topic: "settings", reveal: open("mcp") },
  { id: "settings:mcp-token", label: msg("Rotate the MCP token"),
    description: msg("Rotate token"),
    keywords: ["MCP", msg("password"), msg("security"), msg("token")], topic: "settings", reveal: open("mcp"), landing: "settings:mcp-enable" },
  { id: "settings:mcp-add", label: msg("Add PolarExplorer to an AI client"),
    description: msg("Add the MCP service to Claude Code or Codex, or open its extension in Claude Desktop."),
    keywords: ["MCP", "Claude", "Claude Code", "Claude Desktop", "Codex", msg("plugin"), msg("extension"), msg("connect")], topic: "settings", reveal: open("mcp"), landing: "settings:mcp-enable" },
  { id: "settings:mcp-snippets", label: msg("MCP configuration for other clients"),
    description: msg("The address and token of the MCP service as text, for a client that has no button."),
    keywords: ["MCP", msg("configuration"), "JSON", "ChatGPT"], topic: "settings", reveal: open("mcp"), landing: "settings:mcp-enable" },
  { id: "settings:mcp-copy", label: msg("Copy an MCP client configuration"),
    description: msg("Copy HTTP client JSON"),
    keywords: ["MCP", msg("copy"), msg("clipboard")], topic: "settings", reveal: open("mcp"), landing: "settings:mcp-enable" },
];

export default features;
