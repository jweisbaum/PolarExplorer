import { fold, language, type Language } from "../i18n";

/**
 * The help reference (spec.md 3.5): one topic per area of the interface, in
 * every language. Copied in shape from VectorEffects.
 *
 * The English pages are here; each translation is a file in `locales/`
 * with the same pages, in the same order, with the same ids and
 * cross-references — only the words differ. `topics.test.ts` holds them to
 * that shape. Pages are prose, not keys: they are translated as whole pages,
 * which reads better than a sentence at a time.
 */

export type Parameter = readonly [name: string, description: string];
export interface HelpTopic {
  id: string; group: string; title: string; paragraphs: string[];
  parameters?: readonly Parameter[]; related?: string[];
}

export const TOPICS: HelpTopic[] = [
  { id: "workspace", group: "Workspace", title: "The project window",
    paragraphs: [
      "PolarExplorer builds a sailing polar for one boat from ORC certificates, polar files and race tracks, and exports it for routing software.",
      "The title bar holds the project’s name, the stage switcher, the search, the Project menu and Settings. The left navigation gathers the sources, the centre stage shows the 3D polar, the 2D polar plot, the map or the comparison, and the right panel lists the sources. The status bar at the bottom shows hints, errors and work in progress.",
    ],
    parameters: [
      ["Hide or show the navigation (◀)", "Folds the whole left navigation away to give the stage more room. Each of its sections also folds on its own. What is folded is remembered for you, not stored in the project."],
      ["Stage: 3D / 2D / Compare / Map", "Chooses what the centre shows. 3D is the default, opening with the polar's origin in the middle of the free space; 2D is the polar plot, its 0° axis down the middle of the stage; Map is available when tracks have been imported."],
      ["Hide or show the right panel (▶)", "Folds the source list away."],
      ["Status bar", "Shows the last hint or error and a line for each job that is running."],
      ["Undo / Redo", "Cmd+Z and Cmd+Shift+Z (Ctrl on Windows and Linux) reverse or reapply the last change to the project."],
    ],
    related: ["projects", "search", "map", "sources"] },
  { id: "projects", group: "Workspace", title: "Projects, saving and recovery",
    paragraphs: [
      "A project is one attempt to build one polar for one boat. It is saved as a .wpsproj file that holds every source exactly as imported, with your changes stored beside it.",
      "When an action would close a project with unsaved changes — New, Open, Open Recent, Close or quitting — PolarExplorer asks first: Save, Don’t save or Cancel. Cancel, Escape and a click outside the question all keep the project open as it is. Saving a project that has never been saved asks where to put it; cancelling that also cancels the action.",
    ],
    parameters: [
      ["New… (Cmd+N)", "Creates a project with the name you give it."],
      ["Open… (Cmd+O)", "Opens a .wpsproj file."],
      ["Open Recent", "Lists the ten most recent projects, newest first."],
      ["Save (Cmd+S) / Save As… (Cmd+Shift+S)", "Writes the project to its file, or to a new one."],
      ["Close (Cmd+W)", "Closes the project and returns to the start screen."],
      ["Project name", "Click the name in the title bar to rename the project. A dot after the name means there are unsaved changes."],
      ["Recent projects", "On the start screen. A file that has moved or been deleted is shown greyed as Not found; click it to remove it from the list. Clear forgets the whole list without deleting any project."],
      ["Recovered work", "If PolarExplorer did not close cleanly, the start screen offers the unsaved work it had kept. Recovering opens it as the project it came from, still unsaved."],
    ],
    related: ["settings", "workspace"] },
  { id: "orc", group: "Sources", title: "ORC polars",
    paragraphs: [
      "The search box searches the ORC and ORR catalogues together, each result badged ORC or ORR. ORR includes offshore and short-course boat-speed tables from RegattaMan; Settings can refresh a certificate year. Repeated ORR downloads and imports do not create duplicates. Under Search by field, Measurements unfolds a Min and a Max for length, beam, draft, displacement and sail areas, in metric units. Missing measurements do not pass a bound. Results load as you scroll: the next page appears when you reach the end of the list.",
      "The ORC polars section searches the catalogue of ORC certificates built into PolarExplorer (about 18,000 boats from jieter/orc-data) and adds the chosen ones to the project as sources. The catalogue is part of the application, and About names the orc-data version it was built from. Settings can add this year's certificates to it from ORC's own service; a certificate found both ways is listed once.",
      "Type in the search box and the results update as you type. Every word must match the start of a word in some field: name, sail number, country, model, builder, designer, year built or certificate year, so farr 40 2023 finds Farr 40s from 2023. Case and accents do not matter, and GBR1124, GBR 1124 and GBR/1124 are the same sail number. An exact sail number comes first, then names and models that start with what you typed, then the other matches, newer certificates first.",
      "Search by field, under the box, unfolds one box per field: boat name, sail number, country, model or type, builder, designer, year built (from and to) and certificate year. Every box you fill must match, and so must the search box: the words of a box must start words of that field only, so a designer box holding farr finds boats designed by Farr but not the Farr 40s of other designers. Case, accents and the sail number's separators do not matter here either, and a certificate year box holding 202 finds the certificates of 2020 to 2029. A boat whose whole field equals what you typed in a box comes first. Collapsed, the heading shows how many boxes are in use; Clear empties them all and keeps the search box.",
      "Add copies the certificate into the project as an ORC polar with the next free colour. Its polar has the ORC angles (52° to 150°) at the certificate's own wind speeds, plus the beat and run angle at each wind speed, where boat speed is the VMG divided by the cosine of the angle. Nothing is filled in between or beyond those angles. Adding a certificate the project already holds asks first.",
    ],
    parameters: [
      ["ORC polars (section)", "Click the heading to fold or unfold the section."],
      ["Search", "Words to find, in any order. Each result shows the name, sail number, model, year built, builder, certificate year and a small polar at light, medium and strong wind."],
      ["Search by field", "Unfolds or folds the per-field boxes; PolarExplorer remembers which. The number in brackets is how many are in use."],
      ["Boat name, Sail number, Model / type, Builder, Designer", "Words that must start words of that field, in any order."],
      ["Country", "Only certificates from one country."],
      ["Built from / Built until", "Only boats built in those years, inclusive. A boat with no year is left out while either is set."],
      ["Certificate year", "The certificate's year, or its first digits."],
      ["Clear", "Empties every per-field box. The search box is kept."],
      ["Add", "Adds that certificate as a source. Added marks one the project already holds; adding it again asks first."],
      ["Added list", "Every ORC polar in the project with its colour, sail number, model and certificate year."],
      ["Remove (✕)", "Removes that ORC polar from the project. Undo puts it back."],
    ],
    related: ["sources", "polar-files", "tracks"] },
  { id: "polar-files", group: "Sources", title: "Polar files",
    paragraphs: [
      "Import… opens a file picker where several polar files can be chosen at once. Each file becomes one source, named after the file and given the next free colour. The format is read from the content, not the extension: Expedition (.txt: comment lines starting with !, then one row per wind speed, TWS followed by TWA and BSP pairs), or a TWA × TWS table as Adrena writes it (.pol, tab-separated) or a spreadsheet saves it (.csv, semicolon- or comma-separated), with TWA\\TWS, TWA/TWS or TWA in the top-left cell.",
      "A file that cannot be read is listed below the button with its line, its column and what is wrong there, and nothing of it is imported; the other files of the same import still are. Speeds above 60 kn and negative values are refused. Angles from 180° to 360° retain the independent port side. An imported polar is kept exactly as read; your edits are stored beside it.",
    ],
    parameters: [
      ["Polar files (section)", "Click the heading to fold or unfold the section."],
      ["Import…", "Chooses one or more polar files to import. One undo takes the whole import back out."],
      ["File list", "Every imported file with its colour, its format and its axes: the TWA and TWS each covers, and how many values it has."],
      ["Remove (✕)", "Removes that polar file from the project. Undo puts it back."],
    ],
    related: ["sources", "orc", "tracks"] },
  { id: "tracks", group: "Sources", title: "Tracks",
    paragraphs: [
      "The filters are grouped. Boat speed holds the BSP and VMG ranges (VMG is BSP × cos TWA, negative downwind). Boat heading holds a heading and a COG range, each a compass sector clockwise from one direction to another (type both; clearing one clears both), the direction-change filter, and Remove tacks and gybes, which leaves out the sample on either side of each change of tack. Wind, waves and current holds TWS, TWD (also a compass sector), TWA and the rest. Where the track provides headings, speeds or wind of its own, each group offers Use: provided by the track or derived; otherwise the value is derived. Start and end fields accept seconds.",
      "The Tracks section imports race tracks of the boat. File… reads GeoJSON and CSV files, several at once; YellowBrick…, Geovoile… and Blue Water… import boats from a race tracker (see Race trackers). Each boat becomes one track source with the next free colour, and one undo takes a whole import back out. Positions are kept exactly as imported; filters and exclusions are stored beside them.",
      "GeoJSON files hold Point features, one position each, or LineString features with a time per position in properties.times or properties.coordTimes. Point properties are read whatever their case: time, timestamp or date (ISO 8601, or epoch seconds or milliseconds); cog, heading, hdg or course; sog, speed, bsp or stw (knots); and boat or name, which groups the points into one track per boat. A CSV needs a header row. The import dialog guesses the time, latitude, longitude, heading, speed and boat columns from the header and the first rows, shows a preview, and lets you correct each one, the time format (ISO 8601, epoch seconds or milliseconds, or your own pattern such as %d/%m/%Y %H:%M) and the speed unit. Times without a zone are UTC. When a file holds several boats, tick the ones to import.",
      "Every position gets a heading and a speed. The track's own heading (COG) and speed (SOG or boat speed) are used where it gives them; elsewhere they are derived from the neighbouring positions: the heading is the great-circle bearing from the previous position to the next, and the speed is the distance through the position over the time between its neighbours. The first and last positions, and those next to a gap longer than the maximum gap (3 hours by default), use the one neighbour they have; a position with none has no derived values. Positions at the same time are merged and positions out of order are sorted; the summary after an import says how many. Whether each value was given or derived is kept, so the filters can use it.",
      "Each sample becomes a dot in the polar plots once it has its wind, which Fetch weather… finds for it (see Wind, waves and current); importing never fetches weather, so until then a track is drawn on the map but has no place in the polar. Filtered samples stay in the project; they are drawn dimmed on the map, and in the plots when Show filtered is on.",
    ],
    parameters: [
      ["Tracks (section)", "Click the heading to fold or unfold the section."],
      ["File…", "Chooses GeoJSON or CSV files to import. The import dialog shows each file's boats, and for a CSV the column mapping."],
      ["YellowBrick…", "Imports boats from a YellowBrick race (see Race trackers)."],
      ["Geovoile…", "Imports boats from a Geovoile race (see Race trackers)."],
      ["Blue Water…", "Imports boats from a Blue Water Tracks race (see Race trackers)."],
      ["Track list", "Every track with its colour, boat, event or file, dates, the samples the blend uses out of all it has, and the weather status: not fetched, queued, fetching with how far it is, ready, partial or failed."],
      ["Show on map (⌖)", "Switches to the map and frames the track."],
      ["Filters (▸)", "Unfolds the track's sample filters and its heading and speed derivation. Every change is one undo."],
      ["From / To (UTC)", "The time window: samples outside it are filtered out, such as motoring before the start or after the finish."],
      ["Minimum / Maximum BSP", "Samples slower or faster than these are filtered out. The minimum is 1 kn by default; empty means no bound."],
      ["Manoeuvre threshold", "Samples whose heading changes more than this from a neighbour are filtered out (30° by default): tacks and gybes are not polar sailing. Empty keeps them."],
      ["Heading / Speed: given or derived", "Keep only samples whose value the track gave, or only derived ones, or either."],
      ["Wind, waves and current", "Ranges of TWS, TWD, TWA, wave height and current speed; the wave direction by sector or angle off the bow, or by compass direction; and leaving out currents without tide. A sample without the value a filter reads is left out."],
      ["Use: provided / derived", "Shown in a group only when the track provides that value: its own heading, speed or wind, or one derived from its positions (for wind, the downloaded weather)."],
      ["Select (tick box)", "Ticks the track for Fetch weather for selected tracks…."],
      ["Fetch weather… / Cancel fetch", "On the track's row: fetches the wind, waves and current the samples do not have yet, or all of them again once the track is ready (see Wind, waves and current); while it runs, Cancel fetch stops it."],
      ["Fetch weather for selected tracks…", "Starts fetching weather for every ticked track immediately, one after another."],
      ["Remove (✕)", "Removes the track from the project. Undo puts it back."],
    ],
    related: ["trackers", "environment", "map", "sources", "polar-3d"] },
  { id: "trackers", group: "Sources", title: "Race trackers",
    paragraphs: [
      "Paste a YellowBrick race link or key, such as yb.tl/fastnet2025, and choose Open. YellowBrick and Geovoile load only the boat list at this point; no track positions are downloaded until you click Import tracks.",
      "Search by boat name, sail number, model or division and tick the boats you want. Import tracks downloads the positions, then adds the chosen boats as tracks in one undoable change. Progress and Cancel remain available during the download. If it fails, your search and choices stay for Retry. Importing fetches no weather; fetch it later from the track list.",
      "YellowBrick gives positions only, so each boat’s heading and speed are derived from its positions. A boat’s time window starts at its start and ends at its finish when the tracker gives them (otherwise the race’s start and end), so motoring before the start and after the finish is filtered out; change it in the track’s filters. The track keeps the event’s address and title and the boat’s tracker id, sail number, model and division.",
      "Geovoile… reads the races Geovoile follows from about 2016 on. Paste the race’s viewer address, such as vendeeglobe.geovoile.com/2016/tracker/ (or …/viewer/). A race sailed in legs is imported one leg at a time: add ?leg=2 to the address for the second leg, or choose the leg in the dialog once one is open. Geovoile’s official heading and speed are used for the positions its reports cover, and the rest are derived; the division is the boat’s class, and a boat’s time window ends at its official arrival. Older Geovoile trackers (Flash, or from 2012–2015) are not supported, and the dialog says so.",
      "Blue Water… reads a race’s public data from its own API. Paste the race’s page address, such as race.bluewatertracks.com/2025-melbourne-hobart-westcoaster, or its race key alone. Course and speed over ground are given for every position and used as they are, so nothing is derived; the division is the boat’s handicap class, and a boat’s time window ends at its own official finish, or otherwise the race’s tracked end. Blue Water Tracks sends boat details and positions together. Open resolves the address without downloading them. Click Import tracks to download the event, then choose boats and click Import tracks again to add them.",
      "Downloaded events stay in memory until the app closes, up to two million positions, with the oldest removed first. Reopening a cached event downloads nothing and shows position counts and a map preview. Refresh boat list reloads only metadata; Import tracks then downloads newer positions. YellowBrick falls back to KML when its binary positions cannot be read.",
    ],
    parameters: [
      ["Event address", "A yb.tl, cf.yb.tl or app.yb.tl link, or the race key alone; for Geovoile, the race’s viewer address; for Blue Water Tracks, its race.bluewatertracks.com link or race key."],
      ["Open", "Loads the boat list, or opens tracks already downloaded this session. Blue Water Tracks waits for Import tracks before downloading its combined response."],
      ["Cancel", "Stops any download under way and closes the dialog; nothing is imported."],
      ["Retry", "After a failure, asks the tracker again."],
      ["Refresh boat list", "Reloads the boat list. Newer positions download when you click Import tracks."],
      ["Search", "Shows only the boats whose name, sail number, model or division contains every word typed; case and accents do not matter."],
      ["Boats", "Tick the boats to import. A boat the tracker has no positions for cannot be ticked. The box in the heading ticks or unticks every boat shown."],
      ["Leg", "Chooses a Geovoile leg and loads its boat list."],
      ["Import tracks", "Downloads positions and adds selected boats as tracks, as one undo, without weather. For Blue Water Tracks, the first click downloads the boat picker; choose boats and click again to add them."],
    ],
    related: ["tracks", "environment"] },
  { id: "environment", group: "Sources", title: "Wind, waves and current",
    paragraphs: [
      "Fetch weather on a track, or Fetch weather for selected tracks, starts downloading wind, waves and current immediately. Importing never fetches weather. Only chunks needed for the route are downloaded. Open Data reuses downloads in memory for the session; Whirlwind also caches chunks on disk across routes and restarts. Set the cache directory and size in Settings. Progress appears in the status bar and track list. Cancelling keeps completed samples; a later fetch resumes the rest.",
      "Open Data: Wind (10 m, u and v) comes from WeatherBench2's ERA5 until 10 January 2023 and from ARCO-ERA5 after it; waves (significant height and mean direction) from ARCO-ERA5. Values are interpolated bilinearly between the four surrounding grid points and linearly between hours; wind interpolates its components and the wave direction is interpolated as a unit vector. Grid points on land are left out, and a place with land all round has no waves or current. The current comes from the first source that has it: the NW European Shelf or Iberia–Biscay–Ireland reanalyses (tidal, from 1993), the global merged current (circulation plus tide, from November 2020), or GlobCurrent (geostrophic, Ekman and tide from FES2022, from 1993).",
      "Wind and current are both over the ground; a polar is through the water. Where a current was found, the boat's velocity through the water is its ground velocity minus the current (leeway ignored), and the wind over the water is the wind minus the current. Both are stored, and Correct for current chooses which feed the polar. The wave angle is measured off the bow: 0° head seas, 180° following. Each sample records the dataset and version that supplied it. Wind and current speeds are kept to 0.01 kn, directions to 0.1° and wave heights to a centimetre; the angles and corrected values are worked out from them again whenever the project is opened. Changing a track's heading and speed derivation recomputes the angles from the stored environment without fetching again.",
    ],
    parameters: [
      ["Status bar", "The track being fetched, how far it is and how many wait, with Cancel fetch, which stops every fetch."],
      ["Fetch weather… / Cancel fetch", "On a track's row: fetches what is missing, or every sample again once the track is ready or the interval changes; while it runs, Cancel fetch stops that track's fetch."],
      ["Correct for current", "Feeds the polar from boat speed, TWS and TWA through the water where a current was found (on by default). One undo."],
      ["Leave out currents without tide", "A track filter: leaves out the samples whose current came from a source marked without tide. Every current source read today includes the tide, so it leaves nothing out unless such a source is added."],
      ["Replacing the project", "New, Open or Close while a fetch runs asks to cancel it first."],
    ],
    related: ["tracks", "settings", "map"] },
  { id: "sources", group: "Sources", title: "Source list and polar plot",
    paragraphs: [
      "The right panel lists every source of the project — ORC polars, polar files and tracks — each with its colour, a show or hide switch and a blend weight. A hidden source is left out of the blend and of every plot.",
      "The 2D stage draws boat speed against true wind angle: a curve per visible polar source at one true wind speed, or one curve per wind speed a source has when the slider is set to All, each in its source's colour, with the blend drawn thicker in its own colour. Track samples with their wind are dots in their track's colour when their wind speed is within a knot of the slice (the band is a setting); excluded samples are hidden, and hollow while Excluded is ticked, and samples selected on the map or in the 3D view are ringed. Hovering a curve shows its source, TWA, TWS and BSP; a dot shows its speed over ground (SOG) instead, or through the water (STW) where it was corrected for current; and hovering a cell of the blend also names each source behind it with its speed and its share of the weight; the speeds on the rings and in the hover are in your speed unit (Settings).",
    ],
    parameters: [
      ["Sources (section)", "Click the heading to fold or unfold the list."],
      ["Blend", "The top entry stands for the blended polar: its colour, a show or hide switch, how many cells have direct evidence and how many were filled, Blend settings and Export (see The blend and export)."],
      ["Colour", "Click the swatch for the palette of sixteen colours or a custom colour. A new source takes the first palette colour no source uses."],
      ["Show or hide", "The checkbox. A hidden source is left out of the blend and of every plot."],
      ["Name", "Click a source’s name to rename it; Enter keeps the new name, Esc cancels."],
      ["Kind and count", "The symbol says whether the source is an ORC polar, a polar file or a track. The count is the cells a polar holds, or the samples a track uses out of all it has."],
      ["Weight", "How much the source counts in the blend, from 0 to 1 (1 by default). One drag of the slider is one undo."],
      ["Edit / Compare", "Edit opens the source in the 3D view to edit its polar (see The 3D polar); Compare opens the Compare stage with the source as A and the blend as B. A ✎ after Edit means the source holds edits."],
      ["Remove", "Removes the source from the project. Undo puts it back."],
      ["Reorder (⠿)", "Drag a source by its handle, or focus the handle and press the up and down arrow keys. The order is only how the list is shown."],
      ["All / wind speed slider", "All draws one curve per wind speed each visible source has; the slider picks one true wind speed instead."],
      ["Filtered", "Also draws the samples the track filters take out, dimmed. Offered when the project has a visible track."],
      ["Excluded", "Also draws the samples you excluded, hollow; the rings then reach them too. Offered when a sample is excluded."],
      ["Dot colour", "Colours the dots by their track, or by the time of day they were sailed at: night (21:00–05:00), morning (05:00–12:00), afternoon (12:00–17:00) or evening (17:00–21:00), by local solar time at the boat’s position. Offered when the project has a visible track."],
      ["Measure", "Turns the pointer into a ruler: the plot lists every curve’s boat speed at the pointer’s wind angle, with each one’s difference from the curve you point at, in your speed unit and in percent. Click to pin point A and read the difference from it to the pointer; click again to move A, press Esc to let it go."],
      ["2D", "Shows the polar plot as a stage of its own, between the panels, with the polar's 0° axis down the middle."],
    ],
    related: ["workspace", "blend", "polar-3d", "compare"] },
  { id: "blend", group: "Sources", title: "The blend and export",
    paragraphs: [
      "Asymmetric polar keeps starboard at 0–180° and port at 180–360°, with independent speeds throughout the blend, both plots and exports. Half-circle inputs seed both sides; symmetric mode averages opposite original nodes. Choose linear or monotone spline interpolation, and grid steps of 1, 2, 5 or 10 degrees and knots. Correct blend opens a cell table: changes are saved as overlays after blending, can be undone, and Reset restores the calculated value. The 0° and 360° rows remain zero.",
      "The blend is the one polar PolarExplorer makes from the visible sources, and the polar it exports. It is made on the project’s output grid: each polar source is read onto that grid between its own points, never beyond them, with its edits in and every cell read from an excluded point left out; each track counts through its polar segment, binned on the same grid.",
      "With the default Mean setting, each cell is the weighted mean of the sources with a value there. A polar source counts by its weight; a track cell by its weight times its samples over the samples for full confidence (30 by default), at most fully, and a cell you edited counts fully. Cells no source reaches are then filled between known values, first along the true wind angle at the same wind speed, then along the wind speed. Nothing is extrapolated, so a cell beyond every source stays empty. The 0° row is 0 kn. Smoothing, off by default, evens the filled grid.",
      "The Blend entry at the top of the source list shows how many cells have direct evidence and how many were filled. The blend is drawn thicker in the polar plot and opaque in the 3D view, in its own colour. Hiding it hides it from the plots only; export always writes it.",
      "Export writes the blend as an Expedition (.txt), Adrena (.pol) or CSV (.csv) polar, recomputed from the sources at that moment. Axis values are written with at most two decimals and boat speeds with two, so the same project always gives the same bytes on every computer. The dialog previews the grid first; custom axes write the blend read onto them, without extrapolating.",
    ],
    parameters: [
      ["Polar blend statistic", "min, median, mean, max, p90. Combine visible polars per cell. Mean uses source weights; median and p90 interpolate sorted speeds. Track settings do not affect polar-only views. The polar result carries the combined polar weight when blended with tracks. Track cells count by weight and sample confidence. Empty cells are filled between known values; the 0° row is 0 kn."],
      ["Blend colour", "Click the swatch to change the colour the blend is drawn in."],
      ["Show or hide the blend", "Draws the blend in the plots or not. Export is not affected."],
      ["Coverage", "Cells with direct evidence from a source, and cells filled between them. The tooltip also counts the cells left empty."],
      ["Blend settings", "The output grid, the statistic new tracks start with, the samples a track cell needs, the samples for full confidence, current correction and smoothing. Apply makes them one change, undone with Undo."],
      ["Output grid", "True wind angles (0 to 180) and wind speeds (0 to 70 kn), in increasing order, with at most two decimals and so at least 0.01 apart: every export then reads back. Default grid puts back the grid a new project starts with."],
      ["Statistic for new tracks", "The statistic of a cell’s boat speeds a newly imported track starts with: the 90th percentile by default. Each track keeps its own, changed in its edit panel."],
      ["Samples a cell needs", "A track cell with fewer samples has no value (5 by default)."],
      ["Samples for full confidence", "A track cell counts in the blend by its samples over this number, at most fully (30 by default)."],
      ["Export…", "Opens the export dialog: the format, the grid and a preview, then where to save."],
      ["Format", "Expedition: one row per wind speed holding only the angles that have a value. Adrena: a tab-separated table. CSV: the same table with semicolons."],
      ["Grid", "The project’s output grid, or custom axes the blend is read onto. A grid two of whose values would be written the same with two decimals is refused, and the message names both values."],
    ],
    related: ["sources", "polar-3d", "tracks"] },
  { id: "map", group: "Views", title: "The world map",
    paragraphs: [
      "The map is available when the project has imported tracks. It draws the land and coastlines of the world from data built into the application; no map tiles are ever downloaded.",
      "Every visible track is drawn in its source colour; positions the filters take out are drawn faint, and excluded ones paler. A track that crosses the 180° meridian is drawn as one line across it. Hovering a position shows its time, speed over ground (SOG) and heading (given or derived), and its TWS, TWA, wave height and current once the environment has been fetched.",
      "Shift-drag a box to select the positions inside it. The selection is shared with the polar views: the same samples are selected in the 3D view and ringed in the polar plot, and Show on map in the 3D view highlights its selected samples here and frames them.",
    ],
    parameters: [
      ["Projection: Equirectangular / Orthographic", "Equirectangular draws longitude and latitude as a flat grid. Orthographic draws a globe as seen from space. The choice is remembered for you."],
      ["Drag", "Pans the flat map, or turns the globe."],
      ["Scroll or pinch", "Zooms in and out around the pointer."],
      ["Fit the world", "Shows the whole world again."],
      ["Fit the tracks", "Frames every visible track."],
      ["Shift-drag", "Selects the track positions inside the box, here and in the polar views. Escape or Clear selects none."],
    ],
    related: ["workspace", "tracks"] },
  { id: "polar-3d", group: "Views", title: "The 3D polar",
    paragraphs: [
      "With tracks present, Global point filters apply after each track’s own filters, without start/end dates. Priority filter groups are tried in order for each TWA/TWS cell across visible tracks: the first group with enough samples supplies the cell. Add a less restrictive group last to fill sparse cells; if no group qualifies, there is no track evidence in that cell. Colour options include wave period, wave angle to the bow and wave angle to the wind. Time colour shows UTC date-time endpoints.",
      "The 3D stage shows every visible polar source as a translucent surface over its own grid, with its grid points as dots, and every track sample that has its wind as a dot in its track's colour. A sample without wind (before the environment is fetched) has no place in the polar and is not drawn. Nothing is resampled or extrapolated: an empty cell of a source is a hole in its surface.",
      "In the polar tower (the default) the angle round the vertical axis is TWA, the distance from it is BSP and the height is TWS, so each wind speed is a classic polar curve and the stack is a surface. The Cartesian layout puts TWA, TWS and BSP on three straight axes. The axes are labelled in your speed unit.",
      "Select dots by clicking one, Shift-clicking to add, or drawing a lasso or a box round many. Only dots that are drawn are counted and acted on. The selection panel shows how many are selected, their mean TWA, TWS and BSP, and how many come from each source. Exclude removes the selection from the blend: a grid point is then drawn as a cross, and that cell is empty for that source when the blend is made; a sample is drawn as a ring and left out of its track's polar segment. Include puts them back. Both are ordinary changes, undone with Undo. Nothing in the source itself changes. The samples selected here are selected on the map too, and a box on the map selects them here.",
      "Edit on a source in the source list opens it here in edit mode. The source being edited is drawn opaque and the others fade (Hide other sources hides them). What is edited is the source’s own polar: the imported grid of a polar file, the VPP grid of an ORC certificate, or, for a track, its polar segment — the samples that pass the filters and are not excluded, binned onto the project’s output grid with port and starboard separate in asymmetric mode; a cell holds the track’s statistic of their boat speeds (the 90th percentile by default) once it has at least five samples, and keeps its sample count and spread for the table’s tooltips. Every edit is stored beside the source, never in it: an edited node is drawn as a square and highlighted in the table, every view updates at once, and Reset all edits gives the source back exactly as imported. Surfaces, nodes and the 2D curves show each source as edited; the 2D curves also leave out excluded nodes, as the blend does.",
    ],
    parameters: [
      ["Layout: Polar tower / Cartesian", "How TWA, TWS and BSP are placed in the scene."],
      ["Top / Side / Isometric", "Preset cameras: straight down the wind-speed axis (the classic polar diagram), across it, or the three-quarter view."],
      ["Rotate / Lasso / Box", "With Rotate, drag to turn the view, right-drag to pan and scroll to zoom. With Lasso or Box, dragging selects instead. A click selects the nearest dot in every tool; Shift adds to the selection; Escape clears it."],
      ["Split Wave Angle", "In single view, draws the 3D view once per wave direction as seen from the boat (0° on the bow, 90° on the starboard beam, 180° astern, 270° on the port beam). The slider chooses 4, 8, 16, 18, 24 or 36 directions; one is always centred on the bow, so no direction begins or ends at 0°. From puts a sample in the copy its waves came from, To in the copy they went to, and each copy’s arrow shows that against the boat. A copy holds only its direction’s samples, after every filter and the wave ranges; a sample with no wave direction is in none (the controls count them). Each copy draws its own blend, made with every track binned from that copy’s samples alone and every polar file or certificate as it is, so a copy with no samples shows the reference polars’ blend; round the boat an arc marks the directions the copy holds. The copies share one camera; a dot hovered in one is marked at the same wind in the others, a hovered blend cell is marked in each other copy with that copy’s own speed, and a lasso or box selects in the copy it starts in."],
      ["Show: Samples / Polar nodes / Surfaces / Filtered samples / Excluded points", "What is drawn. Filtered samples are those the track filters remove, drawn dimmed; excluded points are hidden unless Excluded points is ticked. The axes reach what is drawn, so hiding an outlier brings them back to the rest."],
      ["Colour", "Colours the dots by source, or by wave height, current speed, time or time of day (night, morning, afternoon or evening, by local solar time at the boat’s position). Wave height and current need track samples with their environment and are offered once those exist."],
      ["Blend surface", "Hover the blend’s surface, away from any dot, to see the cell under the pointer: its TWA, TWS and boat speed, whether the value comes from sources, was filled in between neighbouring cells or was corrected by hand, and each source behind it with its own speed and its share of the weight."],
      ["Exclude / Include", "Remove the selection from the blend, or put it back. Excluded points are then hidden; tick Excluded points to see them again (grid points as crosses, samples as rings) and include them."],
      ["Show on map", "Switches to the map, highlights the selected samples there and frames them."],
      ["Edit (source list)", "Opens the source in the 3D view in edit mode, with its table. Done leaves edit mode; the edits stay."],
      ["Drag", "In edit mode, drag a node of the source being edited to change its boat speed. Shift snaps to 0.05 kn. One drag is one undo."],
      ["Table", "The source’s polar in your speed unit, TWA down and TWS across. Type a value and press Enter to edit a cell; empty the cell to reset it. Each typed value is one undo. Clicking a cell selects it, Shift-click adds, and edited cells are highlighted."],
      ["Scale / Smooth / Reset", "Act on the selected cells: scale them by the percentage, smooth each over its neighbours on the grid (a 3 × 3 kernel), or put them back to the source’s values. Each is one undo."],
      ["Reset all edits", "Clears every edit of the source being edited; undo puts them back."],
      ["Statistic", "For a track: how a cell of its polar segment sums up its samples’ boat speeds — the 90th or 75th percentile, the median or the mean."],
      ["Hide other sources", "While a source is edited, hides the other sources instead of fading them."],
    ],
    related: ["sources", "compare"] },
  { id: "compare", group: "Views", title: "Compare",
    paragraphs: [
      "The Compare stage shows the difference between two polars, A and B, cell by cell on the project’s output grid. Each is any source’s polar, any track’s polar segment, or the current blend, read as the other views read it: with its edits, without its excluded nodes, and never extrapolated. Compare on a source opens the stage with that source as A and the blend as B.",
      "In 3D, A and B are translucent surfaces in their colours, and the difference surface lies midway between them, coloured by ΔBSP = A − B: orange where A is faster, blue where B is faster, grey where they agree. The scale is centred on zero and its range is in the legend. Only cells where both have a value are compared; a cell only one of them covers is drawn grey — hatched in the heat map and where several lie together in 3D, and marked with a cross in 3D — and counted in the summary. The 0° row is 0 kn by definition and is never compared.",
      "The summary gives the number of cells compared, the mean and largest difference, and for each wind speed the angles where A is faster and where B is faster by more than the threshold. The heat map shows the same differences flat, TWA down and TWS across; hover a cell for its values.",
      "What is compared, the percentage switch and the threshold are not saved and cannot be undone: they are kept for each project while the application runs.",
    ],
    parameters: [
      ["A / B", "The two operands, each chosen by its colour and name: the blend, a source’s polar, or a track’s polar segment. A hidden source can be compared too."],
      ["⇅", "Swaps A and B, which changes the sign of every difference."],
      ["Surface A / Surface B / Difference surface", "Show or hide each surface in the 3D view."],
      ["Δ as percent of B", "Shows each difference as a percentage of B’s speed instead of a speed. A cell where B is under 0.1 kn is not comparable in %: it is drawn plain grey and left out of the percentage statistics."],
      ["Threshold", "A difference must be larger than this (0.05 kn to begin with) for a cell to count as A faster or B faster in the summary."],
      ["Layout / Top / Side / Isometric", "As in the 3D polar: the tower or straight axes, and the preset cameras."],
    ],
    related: ["polar-3d", "sources", "blend"] },
  { id: "settings", group: "Settings and help", title: "Settings",
    paragraphs: [
      "ORC polars downloads every country's valid certificates of the current year from ORC's own service, data.orc.org: about 60 MB in a minute or two. Scrape ORC polars starts it, with the countries done and Cancel download; cancelling leaves the catalogue as it was. A certificate the catalogue already holds is updated, never stored twice; one that ORC no longer lists is removed; and another year's certificate of the same boat is kept beside it. A boat with two valid certificates in a year, a crewed and a double-handed one for instance, is listed twice, each result with its certificate number. Download automatically, on both catalogues, chooses when a download runs by itself: Manually only, On startup or On shutdown. An automatic download is skipped when that catalogue was downloaded less than a day ago; on shutdown, quitting waits for it, and quitting again stops it and quits at once.",
      "ORR polars downloads the public RegattaMan catalogue for a selected certificate year. Scrape ORR polars starts the download, with progress and Cancel download. Cancellation preserves the previous catalogue. Repeated certificates are updated without duplicates; copies already in projects stay unchanged. Download details lists certificates that could not be read.",
      "Settings apply to the whole application and every project, and are never stored in a project. Open them with the gear in the title bar, the Settings button on the start screen, or Cmd+, (Ctrl+, on Windows and Linux).",
    ],
    parameters: [
      ["Data sources", "Open Data uses the public archives. Whirlwind offers Amazon S3, Cloudflare R2 and Tigris. S3 requires no credentials. All Whirlwind sources read hourly wind, waves and current from shared chunks and reuse the disk cache."],
      ["Language", "English, French, German, Spanish, Italian, Dutch, Chinese, Japanese or Arabic (laid out right to left). Everything changes at once, including the menu bar. Also on the start screen."],
      ["Theme", "The colours of the application. Harbour is the default."],
      ["Boat and wind speed / Wave height / Distance", "The units values are shown in: the polar plot, the 3D axes and edit table, Compare, the track filters and the map's hover. Stored values do not change; polar files and the output grid stay in knots."],
      ["Polar plot dot band", "How far from the polar plot's wind speed a track sample may be and still be drawn as a dot, from ±0.25 to ±5 kn (±1 kn by default)."],
      ["Autosave", "Keep a recovery copy of unsaved work (the default), save into the project file itself, or leave everything until you save."],
      ["Keep downloaded weather in memory", "Downloaded weather kept in memory during the session: default 256 MB, range 16–4096 MB. Other routes reuse it. Project weather is saved independently; Whirlwind also has the disk cache below."],
      ["Whirlwind cache", "Whirlwind keeps downloaded chunks on disk for reuse across routes and app restarts. The least recently used chunks are removed when the cache is full. Remove cached Whirlwind downloads. Weather saved in projects is kept."],
      ["Concurrent requests / Request timeout", "How many downloads run at once (8 by default) and how long one may take before it is abandoned."],
      ["MCP service", "Lets an AI client on this computer (Claude Code, Codex, Claude Desktop) drive PolarExplorer through the same commands the interface uses: every change it makes is one you can undo, the views follow it, and the status bar shows MCP and the last tool while a client is connected. It is off until you turn it on. Turning it on opens a port on this computer only and issues a token a client must present; turning it off closes the port and forgets the token. Rotate token issues a new one."],
      ["Add to Claude Code / Codex / Claude Desktop", "Writes the service into that client’s own configuration, or opens PolarExplorer’s extension in Claude Desktop for you to confirm there. Configuration for other clients gives the address and token as text. ChatGPT is not offered: it reaches MCP servers only over the public internet, and this service answers only on this computer."],
    ],
    related: ["projects", "search"] },
  { id: "search", group: "Settings and help", title: "Search and help",
    paragraphs: [
      "The search box in the title bar finds any control by its name or by words it is known by, in the language on screen, with or without accents. Results appear as you type. Choose one and PolarExplorer opens whatever hides it — a panel, a section, a stage, a menu or Settings — and outlines it in orange for a moment.",
      "Help pages are listed below the controls in the results. This reference opens with F1, the ? button or the Help menu, and has its own search.",
      "Data credits: the ORC catalogue comes from jieter/orc-data (MIT); ERA5 wind and waves from ECMWF / Copernicus Climate Change Service, via WeatherBench2 and ARCO-ERA5; currents from E.U. Copernicus Marine Service Information; and the embedded basemap from Natural Earth (public domain). About shows the catalogue’s version. The application’s documentation folder includes the user guide and full data source notices.",
    ],
    parameters: [
      ["Search (Cmd+F / Ctrl+F)", "Moves to the search box from anywhere."],
      ["? (F1)", "Opens this reference."],
      ["Arrow keys and Enter", "Choose a result without the mouse. Escape closes the list."],
    ],
    related: ["workspace", "settings"] },
];

/**
 * The reference in each language. Every translation has the same pages, in
 * the same order, with the same ids and cross-references — only the words
 * differ; `topics.test.ts` holds them to that shape.
 */
const translations = import.meta.glob<{ default: HelpTopic[] }>("./locales/*.ts", { eager: true });

export function topicsFor(language: Language): HelpTopic[] {
  if (language === "en") return TOPICS;
  return translations[`./locales/${language}.ts`]?.default ?? TOPICS;
}

/** Topics whose text holds every word of the query, in the given language. */
export function searchTopics(query: string, topics: HelpTopic[] = topicsFor(language())): HelpTopic[] {
  const words = fold(query).trim().split(/\s+/).filter(Boolean);
  return topics.filter((topic) => {
    const text = fold(JSON.stringify(topic));
    return words.every((word) => text.includes(word));
  });
}
