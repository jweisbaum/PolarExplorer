# PolarExplorer user guide

PolarExplorer builds sailing polars in independent boat tabs from ORC and ORR certificates,
polar files and race tracks. A polar describes boat speed at different true
wind speeds and angles. The resulting blend can be exported for routing software.

## Boat tabs and race projects

A new project starts with one boat. **Add Polar**, to the right of the project
name in the top bar, opens another tab (named Polar 2, Polar 3, …) with its own
sources, filters, edits, undo history and views. It is also available in projects opened from a tracker.
Double-click a tab or press F2 to edit its name in place; Enter keeps it and
Escape cancels. The **×** on a tab closes it: PolarExplorer asks first, then
removes that tab and its sources. **Undo Delete Polar**, at the end of the tab
row, restores it while the project stays open. The last tab cannot be closed. Saving includes every remaining tab. Each starts with 0° at the top.
With more than one, **Export all…** at the bottom right of the window, beside
the version, writes one polar per tab to a folder, reporting tabs with no
exportable data and avoiding overwrites.

The three icons beside Add Polar choose the layout: **Single view**, **Split
view** (two boats) and **Four-way view** (four). In the comparison layouts choose
a boat in each pane's dropdown; the tab row and the left and right panel
toggles are hidden. These views use 3D only. Rotation, pan, zoom and camera
presets are linked. Hovering a dot shows the nearest visible point in the
other panes within 5° TWA and 1 knot TWS; a pane without a corresponding point
says so. Hovering the blended surface shows each pane's own blend cell at the
same wind, so the boats' speeds can be read side by side. Each pane retains its own sources and filters. Use **Filters and
display** to open the comparison pane’s filter controls; they start collapsed
to leave room for the polar. Returning to Single view restores its panel layout.

Choose **Open project from tracker…** on the start screen or Project menu and
paste a YellowBrick, Geovoile or Blue Water race link (a Geovoile race in legs is
built from the leg the link shows). The app downloads the event natively,
creates one tab per boat, imports its race track, and searches the embedded ORC
catalogue, downloaded ORR catalogue and local boat metadata for identical models.
Historical tracks come from the GeoJSON directory configured in Settings.
When the race has several classes, the first click downloads it and offers
**Classes**: each group the tracker puts boats in (IRC Overall, IRC Class 2,
Line Honours…) is listed on its own, and a boat in several ticked groups still
gets one tab. Every class starts ticked; untick the ones you do not want, or
use **All classes** to clear them and tick only those you do.
Choose **Exact boat only** to limit both polars and historical tracks to that
individual boat. This requires matching MMSI or a sail number corroborated by
builder and length; the name or model alone is insufficient. Conflicting
identifiers or specifications reject a match. The boat's track from the supplied
race is included with either option.
The report shows available original boat details (MMSI, model, class, type,
builder, length, sail number and other supplied fields), the matched model,
source counts and unavailable tracks.
Choose **Open project** to open these boats, or **Cancel** (also Escape) to
close the list and keep your current project and unsaved changes.
Imported tracks offer the usual weather download and polar extraction controls.

Names and team names alone never prove identity or a model match. Model, class,
type, design, make and builder fields are checked for specific model evidence.
Broad classes such as IRC, IMOCA 60 or Class 40 and a builder alone are insufficient.
Conflicting builders or known lengths reject matches. A corroborated vessel
identifier can supply an otherwise missing, unambiguous model; a name cannot.
Conflicting MMSIs prevent that identity recovery even when sail numbers match.
No fuzzy model-number matching is used. Boats without verified models still
receive their own race track, with no guessed historical sources.

## Install and find help

Choose the package for your computer:

| Computer | Package |
| --- | --- |
| Intel Mac | `_x64.dmg` |
| Apple Silicon Mac | `_aarch64.dmg` |
| Windows x64 | `_x64_en-US.msi` or `_x64-setup.exe` |
| Windows ARM64 | `_arm64-setup.exe` |
| Linux x64 | `.AppImage`, `.deb` or `.rpm` |

On macOS, open the disk image and drag PolarExplorer to Applications. On
Windows, run one installer. On Linux, use your distribution's package installer,
or make the AppImage executable and launch it. Windows installers can download
WebView2 if it is missing. Builds made without signing credentials may be
blocked or identified as unverified by the operating system; the release notes
should state their signing status.

Press **F1**, use **?**, or choose **PolarExplorer Help** for the offline reference.
Settings changes the interface and help language. The title-bar search finds
controls by name and highlights them when selected. This guide uses English labels;
the built-in reference provides the same instructions in French, German, Spanish,
Italian, Dutch, Chinese, Japanese and Arabic. Arabic is laid out right to left.

This guide and the data notices are also installed in `documentation`: on
macOS inside `PolarExplorer.app/Contents/Resources`, on Windows beside the
application, and on Linux in the application's resource directory. Release
downloads include the same files in `PolarExplorer-documentation.zip`.

## Create and save a project

Choose **New**, enter a project name, optionally name the boat and add notes,
then create it. The left panel imports sources; the centre switches between
3D (the default), 2D (the polar plot), Compare and Map; Map appears only when
tracks are imported. The right panel holds the source list, each source with
its **Weight** slider. The Project menu sits between the search bar and
Settings. The 3D view opens with the polar's origin in the middle of the space
the panels leave free, all of the polar in sight; the 2D view runs the polar's 0°
axis down the middle of the stage.

The left and right panels overlay the view. Use their edge arrows to hide or
show them without resizing or reframing the map or polar. View controls remain
accessible beside the open panels.

Save with **Project → Save** or Cmd+S (Ctrl+S on Windows/Linux). A `.wpsproj`
contains the original sources, your edits and fetched values at each track
position. Keep that file to continue editing; a polar export is only the result.
**Save As** creates another project file. A dot by the project name means
unsaved changes. Closing or replacing an unsaved project offers Save, Don't save
or Cancel. Settings controls autosave and recovery; save explicitly before
sharing a project. After an unclean exit, the start screen offers recovered work.

## Add sources

**ORC polars.** Search the embedded catalogue by name, model, sail number or
other certificate fields. **Search by field** narrows individual fields; all
filled filters must match. Choose **Add** on a result. About shows the catalogue's
source commit and dates. Catalogue searches work offline.
Settings → **ORC polars** downloads this year's valid certificates from ORC's
own service (data.orc.org), about 60 MB in a minute or two, and adds
them to the catalogue. A certificate is never stored twice: downloading again
updates what is there, removes what ORC no longer lists, and a certificate
the built-in catalogue already has is listed once. A boat's certificate of
another year is a different certificate and is listed beside it. So are two
valid certificates of one boat in the same year (a crewed and a double-handed
one, for instance): each result shows its certificate number.

**Keeping the catalogues current.** Both Settings sections have **Download
automatically**: *Manually only* (the default), *On startup* or *On
shutdown*. An automatic download is skipped when that catalogue was downloaded
less than a day ago. On shutdown PolarExplorer waits for the download before
it quits and says so in the status bar; quit again to stop the download and
quit at once.

**ORR polars.** The search box searches the ORC and ORR catalogues together;
each result carries an **ORC** or **ORR** badge. ORR results are the bundled
RegattaMan certificates. Offshore and short-course tables are separate variants.
Settings → **ORR polars** downloads a chosen certificate year, with progress
and cancellation. Repeated scrapes update existing catalogue entries; repeated
imports do not duplicate a source or change copies already in your projects.
Scrapes include the complete public certificate fields and ratings: hull, rig,
sails, stability, trim, performance metrics, custom/general and wind-range
ratings, PCS ratings and both time-allowance tables. Their original values are
kept in the catalogue and copied into the project when you add a polar. Builder
and build-year filters use the certificate data. Older polar-only project
sources stay as imported; remove and add a source again to use a refreshed copy.

Under **Search by field**, **Measurements** unfolds bounds for length, beam,
draft, displacement and sail areas: a **Min** and a **Max** box
for each. Bounds use metric units and omit boats without the required
measurement. Results load as you scroll: when you reach the end of the list,
the next page appears. The track library search works the same way.

**Polar files.** Import an Expedition `.txt`, Adrena `.pol` or supported CSV
polar. Review its preview and any parser error before continuing. Original
grids stay intact; missing cells remain missing.

**Tracks from files.** Import GeoJSON or CSV from the Tracks section. Check
times and positions in the preview and map. Boat speed and course can be derived
from consecutive positions; implausible fixes can be filtered later.

**Tracks from a race.** Paste a supported YellowBrick, Geovoile or Blue Water
Tracks event URL into the tracker import dialog. Select boats as the list
arrives, then import once their positions are available. This imports tracks
only. Weather is a separate, explicit action.

**Tracks from a SYRF track library.** In Settings → **Track library**, choose
the folder of individual-track GeoJSON files and the folder holding
`boat-metadata.json` (empty for the application's own), then **Save library
settings** or just close Settings. Nothing is ever written to a database.

To fill the metadata from a SYRF PostgreSQL database, enter its host, port,
name, user and password under **SYRF database (read only)**. Press **Test
connection**, then **Download boat metadata**. The download only reads,
runs only when you press the button, and keeps the races you scraped.

To fill the library, press **Scrape tracks now**, or set **Run track scraper**
to *On startup* or *On shutdown*. Only finished YellowBrick, Geovoile and Blue
Water races are saved; ongoing and future races are skipped. List race links,
one per line, to scrape only those; leave the list empty to rescrape the
library's races and discover new ones (a YellowBrick user key and device ID
add its catalogue). A race already in the library is never downloaded again,
even when listed; the status counts it as "already in the library". The
scraper paces itself and waits out a busy tracker, so a long scrape is slow
on purpose. A manual scrape shows its progress in the status bar,
where **Cancel scrape** stops it; races already saved stay. The
metadata covers vessels from YellowBrick, Geovoile, Blue Water, old Geovoile,
Regadata and America's Cup.

Use **Search tracks by vessel details** in the Tracks panel. Search by any
vessel field: name, model, class, make, builder, sail number, measurements or
other saved values. Combine terms from different fields to narrow results;
all terms must match the same vessel. Case and accents are ignored.
Results show the race,
date, source and whether its GeoJSON file is available. Import a result, then
use its ordinary **Fetch weather…** button to prepare polar samples.
Your query, remaining results and current page stay open after each import,
so you can add several tracks from the same search. Successfully imported
tracks disappear from that search's results; failed imports stay available
to retry.

## Fetch weather for tracks

When a track provides its own wind, its filters offer **Use** under **Wind,
waves and current**: the wind the track provides (where it gives both speed
and direction), or downloaded weather. When it provides headings or speeds,
**Boat heading** and **Boat speed** offer the same choice between the track's
values and ones derived from its positions; a value the track doesn't
provide is always derived, and no choice is shown. CSV import can map
TWS/TWD columns and a separate wind-speed unit. GeoJSON and SYRF files also
retain supplied wind. Changing this choice preserves both data sources and
needs no fresh download.

Use **Fetch weather…** on a track, or select tracks and use **Fetch weather for
selected tracks…**. **Select all**, above the imported tracks list, ticks every
track at once; then use the weather button to download their weather together.
Choose wind, waves and current as needed, review the estimate
and start the download. Jobs report progress and can be cancelled.

PolarExplorer samples the archives in space and time at each position. It saves
those sampled values and their dataset provenance in the project. Downloaded
blocks stay in memory only; other tracks from the same race can reuse them
during that session. **Weather kept in memory** in Settings limits that reuse.

Coverage differs between datasets, dates and locations. A missing value stays
missing, including sea data unavailable on land. A track needs wind to appear
in the polar views and contribute polar samples. Reanalysis is a reconstruction
of past conditions, with finite spatial and temporal resolution; inspect the
track's context before treating a fast sample as representative performance.

## Inspect, filter and edit

The map shows the tracks; **Fit the world** and track framing help find them.
Switch between the flat map and globe. A map box selects the same samples in
the polar views. Escape clears a selection.

In 3D, the polar tower places true wind angle around the axis, boat speed
outward and true wind speed vertically. Cartesian layout uses three straight
axes. Rotate the view, or choose Lasso or Box to select samples and grid nodes.
In asymmetric mode, angle labels run 0–180° on both halves of the 2D and 3D
polars. Port and starboard retain their independent speeds.
The 2D plot shows a wind-speed slice; its dot band controls which nearby track
samples appear.

Hover over a 3D dot to see its source, TWA, TWS and BSP. Track dots also show UTC
time, waves and current.

In either 2D or 3D, choose **Display → Points and PDF** to show dots over the
probability density gradient. Dot colours, hover, click selection, Lasso,
Box and Exclude/Include remain available. PDFs update with the current filters,
exclusions, source weights and blend settings. In 3D, dots remain visible over
the opaque PDF volumes. With the Rotate tool active, click between dots to
expand a PDF cell and see its percentiles; **Show all cells** or Escape returns
to the full view. Choose **Probability density (PDF)** to show PDFs alone.
**Points and polars** uses the same point styling as **Points and PDF**.
Selecting a PDF cell in 3D dims points outside its box; clearing the cell
selection restores their visibility. Weight adjustments update the PDF in
place without blanking the view or resetting your 3D camera.

Use **Wave range filters** at the bottom centre of the 3D screen to set lower
and upper limits for wave height, angle off the bow, and period. Each row has one
slider with two round handles: drag the start or end handle, or focus it and use
the arrow keys. Drag the highlighted middle section to move both limits together
without changing their spacing; it stops at either end of the scale. You can also
focus the middle section and use the arrow keys, or Home/End to move to either
end. The values beside the slider show both limits. The dots update while you
drag, and the blend and both plots follow. These saved, undoable ranges apply
in addition to the other filters. Missing measurements fail an active range.
**Reset ranges** clears these sliders without clearing other filters.

**Split Wave Angle**, beside the Box tool in single view, draws the 3D view
once for each wave direction as seen from the boat: 0° is waves on the bow, 90°
on the starboard beam, 180° from astern and 270° on the port beam. Use the
slider to choose 4, 8, 16, 18, 24 or 36 directions. One direction is always
centred on the bow, so 0° is never a dividing line. Choose **From** to sort
samples by where their waves came from, or **To** by where they were going.
Each copy contains only the samples for its direction, shows how many there are,
and has a large arrow showing the waves against the boat, with an arc round the
boat marking the directions the copy covers. Each copy also draws its own blend,
made from its direction's samples together with your polar files and
certificates as they are, so hovering the blend in a copy shows what the boat did
in those waves and which sources stand behind it. All other filters,
including the wave range filters, still apply. Samples with no wave direction
appear in no copy; the toolbar says how many. The copies rotate and zoom
together, and hovering a sample in one copy marks the sample at the same wind in
the others. Split and four-way view do not offer it.

Track filters remove unsuitable samples from the track's polar segment.
Their limits are shown in the units selected in Settings. **Exclude** removes
selected samples or nodes from the blend; **Include** restores them. Filtered
or excluded samples can still be inspected. Neither action deletes raw fixes.

Filter edits update while you type. **Direction change**, **AWA change**, and
wind speed/direction change thresholds compare each point only with the
immediately previous and next points. A change over the threshold on either side
excludes the point. Missing neighbours and gaps are not skipped or bridged.
AWA is calculated from boat motion and the selected true wind.
These controls are available for individual tracks, global filters and priority
groups. Empty thresholds disable their filter; Undo restores an edit.

Track filters are grouped. **Boat speed** holds the BSP range and a **VMG**
range (speed made good to windward, negative downwind). **Boat heading**
holds a **heading** and a **COG** range, each a compass sector from one
direction clockwise to another (type both; clearing one clears both), the
direction-change filter, and **Remove tacks and gybes**, which leaves out
the sample on either side of each change of tack. **Wind, waves and
current** holds TWS, **TWD** (also a compass sector), TWA, the change
filters, and the wave and current filters, including unknown wave/current
removal; wave direction can be compared with COG. Track start/end times
accept seconds.

With tracks present, the 3D view offers **Global point filters**. These layer
over individual filters and affect the blend and every view; they have no
start/end time. **Priority filter groups** select the first group with enough
samples in each TWA/TWS cell, pooled across visible tracks with positive weight.
Put stricter criteria first and a looser fallback last. If none qualifies, the
cell has no track evidence. This pooled minimum replaces the per-track minimum
while priorities are enabled. Other sources can still supply the cell.

Colour dots by wave period, wave angle to the bow or wave angle to the wind.
Time colouring displays the UTC date and time at both ends of the scale.

Choose **Edit** on a source to open its table and 3D editing tools. Change a
cell, drag a node, scale selected cells or smooth them. **Reset** clears those
edits; **Reset all edits** restores the source. **Undo** and **Redo** apply to
project edits, including exclusions. A track's grid is derived from its eligible
samples: by default each bin needs five samples and uses their 90th percentile
boat speed. The statistic can be changed in edit mode.

Every change is an overlay beside the original source. Clearing overlays gives
back the imported data. Speed display units can be knots, m/s or km/h; stored
polars and polar export formats use knots.

## Blend, compare and export

Hover the blend in the 3D view or on the polar plot to see, for the cell under
the pointer, each source behind its value with that source's own speed and its
share of the weight. On the polar plot, **Measure** lists every curve's boat
speed at the pointer's wind angle with the differences between them; click to
pin a point and measure from it. **Colour** can show the time of day each track
position was sailed at (night, morning, afternoon, evening by local solar time)
instead of its track, in both views.

Use the source list to control visibility, colour and weight. Visible sources
with positive weight contribute to the blend on the project's output grid.
**Blend settings…** edits that grid. Weights run from 0 to 1 and are relative: halving
every weight does not change the result. A project saved with a weight above
1 by an earlier version opens with that weight at 1. The blend uses each source with its edits and
exclusions; it does not extrapolate beyond a source's coverage.

In the top bar, **Asymmetric polar** keeps independent starboard
(0–180°) and port (180–360°) values in both plots, blending, corrections and
export. Half-circle inputs seed both sides. Symmetric mode averages opposite
original nodes without rewriting imported data. The toggle applies immediately
and supports Undo/Redo; leave it unchecked for symmetric polars. In Blend
settings, choose linear or monotone spline interpolation and TWA steps of
1/2/5/10° or TWS steps of 1/2/5/10 kn.
Neither interpolation mode extrapolates beyond supported values.

**Correct blend** opens a cell table for manual changes after blending.
Corrections are saved as overlays, support Undo/Redo, and can be reset to the
current calculated blend. Head-to-wind rows at 0° and 360° remain zero.

In **Compare**, choose A and B from sources or the blend. The difference is
**A − B**: positive values mean A is faster. Empty coverage is distinguished
from zero difference. Use the heatmap or 3D view to inspect disagreements.

Choose **Export…**, select a format, and use the project grid
or a custom grid. Inspect the preview, then choose a destination. The blend is
recomputed for every export; exporting does not save the project or replace
its sources. Given the same project and options, export bytes are reproducible.

For **WhirlwindMatrixPolar (.wmpolar)**, the **Matrix data modes** section
starts collapsed in both export dialogs. Expand it to adjust the settings.
All three data modes start enabled:
boat speed (SOG), histogram (sampled PDF), and Gaussian PDF. The default wave
buckets are:

- Height: 0–<1 m, 1–<3 m, 3–<5 m, and ≥5 m.
- Direction clockwise from the bow: −45–<45°, 45–<135°, 135–<225°, and
  225–<315°. The first bucket includes the bow and wraps through 0°.
- Period: 0–5 seconds and >5 seconds.

That gives 32 wave combinations. No configuration is required. To change them,
open **Customize wave buckets (optional)** and edit the comma-separated
height/period cutoffs or the direction boundaries. Add or remove cutoffs to
change the number of buckets; leave height or period blank for a single
all-values bucket. Directions must cover a full 360° turn. **Reset wave
buckets** restores the defaults. Uncheck **Split by wave conditions** for a
wave-independent polar, or uncheck any unwanted data modes. Histogram mode
also lets you set the number of speed bins over 0–60 kn. The same defaults
and controls appear in **Export all tabs**.

Each wave combination has its own wind matrices and distributions, computed
using the source weights, filters, exclusions and blend settings. Track
samples without complete wave data are omitted
from wave-conditioned exports. Ordinary polars provide a baseline for every
wave combination. A histogram retains the admitted speed distribution, shifted
to follow the chosen statistics and corrections; Gaussian mode stores its
weighted mean and standard deviation. PDF-only files can be exported, but
PolarExplorer import requires SOG mode.

## Let an AI client drive PolarExplorer

**Settings → MCP service** lets an AI client on the same computer work in
PolarExplorer for you: open a project, add certificates, polar files and
tracks, fetch weather, set weights and filters, read what stands behind a
blend cell, compare and export. It uses the same commands the interface
does, so every change it makes is one you can undo, and you watch the views
follow it. While a client is connected the status bar shows **MCP** and the
last tool it called. It does not replace a file that is already there (a
project saved under a new name, an exported polar) unless it was told to.

The service is off until you turn it on. Turning it on opens a port on this
computer only (47392 unless you change it) and issues a token a client must
present; turning it off closes the port and forgets the token. **Rotate
token** issues a new one, after which a client added before must be added
again.

- **Add to Claude Code** and **Add to Codex** write the service into that
  client's own configuration. Restart a session that is already running.
- **Add to Claude Desktop** (macOS and Windows) opens PolarExplorer's
  extension in Claude Desktop, where you confirm the install. It needs
  installing once.
- **Configuration for other clients** gives the address and token as text.
- **ChatGPT** is not offered: it reaches MCP servers only over the public
  internet, and this service answers only on this computer.

A client cannot read or change Settings, set the track library's folders or quit the
application. It can start the downloads you can start (a tracker's race,
weather for a track, a year of the ORR catalogue); ask it to say so first
if that matters to you. It can also read the files you could open here: a
track or polar file it is pointed at is read wherever it is.

## When something is missing

- **Track absent in a polar view:** fetch wind and check filters and exclusions.
- **Blank blend cells:** check visibility, positive weights, source coverage and
  the minimum number of samples per track bin. The app does not fill unsupported data.
- **Weather gaps:** check dataset coverage and whether positions fall on land.
- **Slow downloads:** fetch only the tracks and variables needed; reuse the
  session's memory cache for boats in the same race.
- **A moved recent project:** open its new location; removing an entry from
  Recent does not delete the file.

The [data source notices](DATA-SOURCES.md) identify the archives and their terms.
