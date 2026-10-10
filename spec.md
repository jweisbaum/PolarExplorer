# PolarExplorer — Specification

**Status:** Draft v0.1 · **Date:** 2026-09-27

This document says what PolarExplorer does and the rules it keeps. `plan.md`
says in what order it gets built. `CLAUDE.md` is the operational guide for
working in the code. Decisions are cited as D## and milestones as M## (both
defined in `plan.md`).

The application was named PolarEffects until 2026-10-01 (D30). The rename is
of the name a person reads: the bundle identifier, the settings folder
(§3.4), the `.wpsproj` extension and the crates' `pe-` prefix are unchanged,
so nothing a person had moved.

---

## 1. Purpose and scope

### 1.1 Purpose

PolarExplorer builds sailing polars in independent boat tabs within a project. The user gathers
evidence about how the boat sails:

- ORC velocity predictions for the boat and its sister ships,
- existing polars in Expedition or Adrena format,
- historical race tracks of the boat, from YellowBrick, Geovoile, Blue Water
  Tracks, or GeoJSON/CSV files.

The app puts every track position against reanalysis wind, waves and current.
It turns each track into a polar segment, lets the user inspect, filter and
edit every source in 3D, blends the enabled sources into one polar, and
exports it for routing software.

### 1.2 Users

Navigators, routers and performance analysts who already know what a polar
is and use Expedition, Adrena or a similar router. The app does not teach
sailing. It does explain every control through tooltips, help topics and the
feature search.

### 1.3 Platforms

One code base, built for:

| Platform | Rust target |
|---|---|
| macOS Apple silicon | `aarch64-apple-darwin` |
| macOS Intel | `x86_64-apple-darwin` |
| Linux x64 | `x86_64-unknown-linux-gnu` |
| Windows x64 | `x86_64-pc-windows-msvc` |
| Windows ARM64 | `aarch64-pc-windows-msvc` |

Windows ARM64 is a target that VectorEffects does not ship. It rules out any
C dependency that does not cross-compile cleanly to it (D6).

### 1.4 Architecture

The app copies VectorEffects' architecture (D1):

- Tauri 2 shell. Rust owns the whole domain. React 19 + TypeScript + Vite is
  a view layer only.
- **The back end is Rust only.** No Python, Node or browser sidecars, and no
  headless browser for scraping (D2).
- IPC types are generated with ts-rs into `ui/src/generated/`. All `invoke`
  calls go through `ui/src/ipc.ts`.
- Errors cross IPC as `{ kind, message }`, with messages built as "could not
  <action> <subject>: <cause>".
- One plain stylesheet with theme role colours applied as CSS variables. The
  system font stack; no web fonts.
- Undo/redo is a command history in `pe-core`.

The crate layout is in `CLAUDE.md`.

### 1.5 Non-negotiable invariants

These are repeated from `CLAUDE.md`, which is authoritative:

1. Sources are immutable; edits are overlays.
2. The blend is derived, never stored as truth.
3. Fetched environment samples are project data; rendered views are not.
4. Nothing is fetched that the user did not ask for, and nothing reaches in.
   A WebDriver endpoint on loopback for agent-driven UI tests is compiled in
   only with `pe-app`'s `webdriver` feature, which no shipped build enables
   (D25). **One inbound exception (D29):** the MCP service of §3.7 listens
   on `127.0.0.1` while — and only while — the person has switched it on in
   Settings, and answers only to the token that switch issued.
5. Export is deterministic and byte-reproducible.
6. Interaction stays fast; imports and fetches may be slow.
7. Every string is translatable and every control is findable.

### 1.6 Out of scope for v1

Live tracking, routing, VPP computation from hull measurements, sail
crossover charts, polars per sail, instrument log import (NMEA/Expedition
logs: see §14).

---

## 2. Vocabulary

| Term | Meaning |
|---|---|
| **Project** | One or more independent boat tabs, saved together as `.wpsproj`. |
| **Boat tab** | One boat's sources, blend, filters, undo history and view state. |
| **Source** | Anything that contributes to the polar: an ORC polar, an imported polar file, or a track. |
| **Polar** | Boat speed (BSP) as a function of true wind angle (TWA) and true wind speed (TWS), on a TWA × TWS grid. |
| **Sample** | One track position with its derived heading and speed and the wind, wave and current found for it. A dot in the plots. |
| **Polar segment** | The partial polar derived from one track's samples. It covers only the TWA/TWS cells the track visited. |
| **Overlay** | A user change stored beside a source: exclusions, cell overrides, filters, colour, weight, visibility. |
| **Blend** | The polar computed from all enabled sources with their overlays. The thing that is exported. |
| **Event** | A race on a tracker (one YellowBrick race key, one Geovoile race, one Blue Water race). One event can supply several tracks. |

---

## 3. Application shell

### 3.0 Boat tabs and fleet comparison

A new project starts with one boat tab. **Add Polar**, beside the project name
in the top bar, creates another empty tab (named *Polar N*) in any project,
including tracker projects; there is no fixed tab limit. A tab is one boat's
polar, and its controls name it so. Double-click a boat tab (or press F2) to
edit its name in place, Enter/blur to keep it and Escape to cancel. There is no
separate rename button. The **×** on a tab closes it: after an "are you sure"
that names the tab, it removes that boat and cancels its weather jobs. The
last tab cannot be closed. **Undo Delete Polar**, at the end of the tab row
once a tab has been closed, restores removed
tabs, sources and local edit histories while the project remains open. Deleting
the first boat preserves the project title and save location. Every boat starts
in the top camera view with 0° upwards, including empty boats and tracker tabs.
Every boat has the same left/right overlay panels and centre view selector,
with isolated sources, filters and undo history. Save, recovery and reopen retain
all boats; older single-boat projects open as one tab. Boat-specific IPC requests
carry the boat identity, so switching tabs cannot redirect in-flight work.

**Export all…** appears at the bottom right of the window, beside the version,
when there are multiple boats and exports each boat's blend in the chosen
existing polar format, with collision-free file names. The layout switch is
three icons in the top bar beside Add Polar (one pane, two, four), each named
in its tooltip: a split layout compares two selected boats; a four-pane layout
compares up to four. These layouts only show 3D and hide the tab row and the
left and right panel toggles; each pane's boat dropdown replaces tab
navigation. Rotation is synchronized; hovering a point shows each boat's nearby
point at the corresponding TWA/TWS, or no matching point when that boat has
none. Hovering the blend's surface shows, in every pane, that boat's own blend
cell at the same wind (the cell of its grid nearest the hovered one, within 5°
and 1 kn), each tooltip beside its cell; a boat with no cell there shows none.
Each pane retains independent sources and filters.
Comparison filter/display overlays start collapsed behind **Filters and display**;
single-view panel states are retained when switching layouts.

**Open tracker link** accepts YellowBrick, Geovoile (from 2026-10-03; a race in
legs is built from the leg its link shows, and Geovoile gives only names and sail
numbers, so few certificates match by model) and Blue Water event links and creates
one tab per boat. It imports the event track and all available local polars and
library tracks confidently identified as the same model. Both historical tracks
and polars may come from other boats of that identical model (user clarification,
2026-10-01). Preserve the original tracker details and URLs. Use all available
details including MMSI, sail number, boat/team aliases, length, builder, model,
class and type to resolve the model; model-like values may occur under any of
those descriptive labels. Generic racing classes, a shared builder alone, or a
name alone must not establish an identical model. Conflicting model numbers,
builders or materially different lengths reject an automatic association.
Conflicting MMSIs also reject an individual-vessel identity match used to fill
missing model information; different vessels can still share the same model.
Discovery/import reports progress, supports cancellation, avoids duplicate
sources and reports unavailable files or unresolved model information.
When the race has more than one class (the tracker's division), the dialog
first downloads it, kept for the session, and offers **Classes**: a checkbox
per class with its boat count, all ticked, and *All classes* to tick or clear
them together (asked 2026-10-04; several at once 2026-10-05). Each of a
boat's groups is a class of its own: YellowBrick and Blue Water Tracks join a
boat's groups ("IRC Overall, IRC Class 2"), Geovoile names one class. The
ticked classes build tabs for their boats only, one tab per boat however many
of its classes are ticked; with every class ticked the whole
race opens, boats without a class included, and with none Open is disabled.
A class no boat sails in is refused. MCP's `race_project` takes the same
list as `classes`.
The opening dialog offers **Identical models** (default) or **Exact boat only**
for both polars and historical tracks. Exact matching requires a valid matching
MMSI or a matching qualified sail number corroborated by builder and known
length. Names and model specifications alone do not establish identity;
contradictory MMSIs, models, builders or lengths reject an association. The race
track itself is included in either mode. The boat report displays the original
available metadata, including model/class/type, builder, length, MMSI, sail number,
and provider-specific fields and URLs; none of those original values is replaced
by the normalized model used for matching.
The boat list is a preview: **Open project** commits it. **Cancel** or Escape
discards it and closes the dialog, preserving the previous project, unsaved
edits, undo history and recovery snapshot (or returning to the start screen).
Changes to the current project while reviewing the list prevent replacement.

### 3.1 Start screen

Shown when no project is open. Same layout and component structure as
VectorEffects' start screen (`StartScreen.tsx`): a centred panel with

- **New project**: opens the new-project form (§4.2) inline.
- **Open…**: native file picker filtered to `*.wpsproj`.
- **Recent projects**: the ten most recent, name and path, newest first,
  with **Clear**. A missing file is shown greyed with "Not found" and removed
  on click after a confirmation.
- **Recovered work**: autosaves left by a crash (§4.5), if any.
- Header controls: language picker, Help, Settings.

**Styling** is VectorEffects' structure with a different default palette
(D8). The default theme is **Harbour**:

| Role | Value |
|---|---|
| `--bg` | `#2e3f55` |
| `--surface` | `#253447` |
| `--panel` | `rgba(37, 52, 71, 0.96)` |
| `--inset` | `#1f2c3c` |
| `--raised` | `#2e3f55` |
| `--hover`, `--active` | `#36597a` |
| `--border` | `#5b7fa3` |
| `--border-subtle` | `#3a506a` |
| `--text` | `#d6e6f5` |
| `--muted` | `#b3c9de` |
| `--accent` | `#8fb8de` |
| `--highlight` | `#e8f1fa` |
| `--warning` | `#edbd8d` |
| `--error` | `#e9a7a3` |
| `--selected-bg` | `#8fb8de` |
| `--selected-ink` | `#1f2c3c` |
| `--flash` | `#ff8a1f` (feature-search highlight, same in every theme) |

The other bundled themes are ported from VectorEffects (Midnight, Ocean,
Plum, Ember, Paper) with the same token set. A Custom theme editor is
deferred (§14).

### 3.2 Project window

```
┌────────────────────────────────────────────────────────────────────────┐
│ Fastnet polar •   [3D | Map | Compare]   [? search]  [Project ▾]  ⚙    │
├──────────────┬───────────────────────────────────────────┬─────────────┤
│ ◀ ORC polars │                                           │ Sources     │
│   search…    │                                           │ ■ ORC Bxx   │
│   added list │         centre stage:                     │ ■ Exp file  │
│ ▸ Polar files│         3D polar (default),               │ ■ Track 1   │
│   import…    │         world map, or compare             │ □ Track 2   │
│ ▸ Tracks     │                                           │─────────────│
│   + YB / GV  │                                           │ Polar plot  │
│   + BWT / file                                           │ (2D, dots)  │
│   track list │                                           │ TWS [12] kn │
├──────────────┴───────────────────────────────────────────┴─────────────┤
│ status / hints / job progress                                          │
└────────────────────────────────────────────────────────────────────────┘
```

- **Title bar**: the project name (click to rename), a dirty dot, the stage
  switcher, the **Asymmetric polar** checkbox, the help search (a search
  box and a "?" button that opens the help window), the Project menu and
  settings (Cmd/Ctrl-,). The Project menu sits
  between search and settings; its popup opens towards the left.
- **Project menu**: New…, Open…, Open Recent ▸, Save, Save As…, Close, each
  with `data-feature="project:*"`. Standard shortcuts (Cmd/Ctrl-N, O, S,
  Shift-S, W). Cmd/Ctrl-Z and Shift-Z undo and redo outside text fields. The
  native menu has no Close Window item, so Cmd/Ctrl-W always means Close
  project.
- **Left navigation**: collapsible as a whole (◀) and per section. Three
  sections, in this order: ORC polars (§5), Polar files (§6), Tracks (§7).
  Collapse state is remembered per user, not per project.
- **Centre stage**: 3D (default, §10), 2D (§9.2, since 2026-10-02),
  Compare (§11), then Map (§9.1) last (asked 2026-10-03). Map appears only when the project contains an
  imported track (including hidden tracks). Removing the last track while on
  Map returns to 3D. Opening or creating a project starts in 3D.
  A stage once shown stays as it was left while another is on show:
  coming back to it finds the same camera, toggles, tool, selection,
  slider and plot settings, without loading it again (asked 2026-10-04).
  A hidden stage does no work for edits made meanwhile; when shown again
  it reads the project once. (Switching boat tabs still opens a boat's
  stages afresh.)
- **Right panel**: the source list (§8). Collapsible. (The 2D polar plot
  was a section of it until 2026-10-02; it is a stage now.)
- Both side panels overlay the centre stage. Opening or closing either panel
  leaves the stage viewport, canvas dimensions and camera framing unchanged.
  Floating view controls stay in the uncovered area; panel dialogs open
  above the side panels.
- **Status bar**: hints, errors, and a progress line for running jobs, with
  Cancel.

### 3.3 Unsaved changes

The VectorEffects save guard is copied unchanged (D9):

- Dirty state lives in Rust. Every mutation bumps a revision and sets
  `dirty`; save clears it. A new project starts dirty.
- New, Open, Open Recent, Close and quitting the app go through
  `mayReplaceProject`. It asks **Save / Don't save / Cancel**. Cancel is the
  default and is taken by Escape and by clicking outside.
- Choosing Save on a never-saved project opens Save As. Cancelling that file
  dialog aborts the whole operation.
- The back end refuses to drop a dirty project unless the call passes
  `discard_unsaved = true`, so no caller can lose work by forgetting to ask.
- Quitting (the Quit item, Cmd/Ctrl-Q, the platform's own quit) and the
  window's close button are stopped in Rust while the project is dirty; Rust
  emits `app://quit-requested`, the frontend runs the same guard, and exits
  through `quit_app(discard_unsaved)`, which Rust checks like any other
  discard. "Don't save" also drops the recovery snapshot.
- A running track job (tracker download or reanalysis fetch) belongs to the
  project. Replacing the project asks to cancel the job first.

### 3.4 Settings (global)

Stored in `settings.json` in the platform config directory
(`directories::ProjectDirs::from("com", "PolarEffects", "PolarEffects")`),
beside the recent list. Every field has a default; a file that fails to parse
falls back to defaults, and it is read field by field, so one unreadable or
out-of-range value costs only that preference. Settings are not stored in
projects. Each change is validated and saved by Rust at once; a refused value
or a failed write leaves the previous setting in place.

- **Language** (§3.5). Also shown on the start screen.
- **Theme**.
- **Units**: boat and wind speed (kn default, m/s, km/h), wave height (m,
  ft), distance (nm, km).
- **Autosave**: recovery (default), save, off.
- **Data sources**: **Open Data (Slow)** (default) keeps the existing public
  archives and current-source fallback chain. **Whirlwind (Fast) S3** reads
  `s3://whirlwind-hindsight/hindsight`, in `us-east-1`, for wind, waves and
  current. **Whirlwind (Fast) R2** reads Cloudflare R2 at the configured
  account endpoint; **Whirlwind (Fast) Tigris** reads Tigris at
  `fly.storage.tigris.dev`. Both use `whirlwind-hindsight/hindsight` and
  SigV4 region `auto`. The choice is global and captured when a fetch is queued. Existing
  project values stay unchanged until a fetch is requested; resuming a track
  from a different source replaces its weather, so a track never mixes sources.
  S3 reads are anonymous, including metadata and byte ranges: no AWS
  credential file or environment variables are read, and no authorization or
  signing headers are sent. At the user's explicit request on
  2026-10-07, the supplied R2 and Tigris S3 credentials are built into the Rust
  backend, redacted from Debug and excluded from settings, IPC, projects and
  URLs. The R2 management API token is not needed or bundled. Signed requests
  never follow redirects. Each storage source has its own provenance and
  endpoint-scoped memory/disk cache keys.
- **Downloaded weather**: "Keep downloaded weather in memory for this
  session (MB)" (default 256, 16–4096). The blocks a fetch downloads (§7.5)
  are kept in memory, least recently used first to go, so other boats of the
  same race reuse them. Open Data downloads stay in memory and quitting
  forgets them (D27). Whirlwind's compressed inner chunks also use the disk
  cache below (asked 2026-10-07). An earlier version's on-disk chunk cache (the
  `chunks` folder in its settings' `chunk_cache.location`, or in the platform
  cache directory) is announced on the status line the first time a project
  window opens, then removed in the background — only that folder, and only
  when it holds nothing but the dataset folders the cache wrote; its
  settings are no longer read or written.
- **Whirlwind cache**: cache directory (empty for the platform cache folder),
  maximum size in decimal GB (default 10, range 1–4096), and **Clear cache**.
  A dedicated `whirlwind-hindsight-v1` child holds only validated compressed
  inner chunks; metadata and shard indexes are read anew each provider session.
  Chunks are reused across routes and app restarts. Cache keys include the
  archive identity, shard ETag and byte range; changed shard versions cannot
  reuse older data. Files have integrity checks and atomic publication;
  damaged or incomplete entries are downloaded again. Least recently used
  entries are evicted before writes to stay under the limit, and lowering the
  limit prunes immediately. Clearing removes owned cache entries only and
  invalidates in-flight writes and memory chunk reuse. Weather saved in projects
  is unchanged. Changing directory applies to later fetches; older cache files
  remain in their original directory. Open Data does not use the disk cache.
- **Network**: request concurrency (default 8, 1–32), timeout (default 60 s,
  5–600 s).
- **Map projection** (§9.1), remembered here rather than in the project.
- **Polar plot dot band** (§9.2): how far from the plot's wind speed a
  sample may be and still be drawn, ±0.25 to ±5 kn (default ±1 kn). A display
  preference; it never changes the blend.
- **MCP service** (§3.7): off by default; a port (default 47392) and, while
  on, a token. The settings file is written owner-only.
- **Catalogue schedules** (§5.4): when the ORC and the ORR catalogue are
  downloaded by themselves, each *Manually only* (default), *On startup* or
  *On shutdown*.

### 3.4.1 SYRF track library

The track library is two folders the person chooses in Settings → **Track
library** and saves with **Save library settings**, or by closing Settings
(asked 2026-10-06; a draft Rust refuses keeps the dialog open with the
reason): a folder of
individual-track GeoJSON files, and the folder holding `boat-metadata.json`
(empty for the application's config directory, `boat-metadata/`). What it
scrapes goes only into these two folders (asked 2026-10-04). An older
settings file's `database` section still gives the folders, the scraper
fields and the connection.

**Downloading metadata from the SYRF database** (restored read-only
2026-10-06, D34). Under **SYRF database (read only)** the person gives the
PostgreSQL host, port, database, user, password and whether TLS is required
with a verified certificate (default `localhost:5432`, `syrfbackendprod`,
`postgres`, no TLS). **Test connection** signs in, checks that the session
is read-only and that every table below can be read, and reads no rows.
**Download boat metadata** saves the settings and runs only when pressed.
It has no schedule, cannot run while a scrape does, and can be cancelled.
The session is opened with `default_transaction_read_only` on, and all
reads happen in one `READ ONLY, REPEATABLE READ` transaction, so the server
refuses any write, including a temporary table. The subsets are therefore
common table expressions. Rows come from `Vessels` (not deleted),
`VesselParticipants`, `CalendarEvents`, `CompetitionUnits`,
`VesselParticipantGroups`, `VesselParticipantEvents`,
`VesselParticipantTrackJsons`, `Courses` and
`CourseUnsequencedUntimedGeometries`. Each table is limited to the
supported sources below, read as `row_to_json` and ordered by id. One
search record is built per boat in each race's group, carrying the track
file's storage key when the database names one. The result is **merged**
into `boat-metadata.json`, which is written once at the end; a cancelled
or failed download leaves the file as it was. The database's rows and
records replace those of the previous download. What scraping saved stays
(told apart by its derived event ids), except a scraped race that the
database also holds, which is removed with its files, as at a scrape's
start. Progress (`library://metadata`) shows in Settings with the
database's track count and the scraped tracks kept. The password stays in
the local settings file and is never logged; MCP clients cannot reach these
commands.

**Scraping** (restored 2026-10-04) fills the library from YellowBrick, Geovoile
and Blue Water Tracks through `pe-trackers`. It runs when the person presses
**Scrape tracks now**, or on startup or shutdown once they choose that schedule
(*Only on demand* is the default). The race URLs field, one per line, limits a
scrape to those races; empty, it rescrapes the races the library knows and
whatever each tracker lists (YellowBrick's catalogue through the optional user
key and device ID, kept only in local settings and never logged). **Only
finished races are saved**: every boat must have a terminal status and no
position may lie in the future; an ongoing, future or unverifiable race is
counted as skipped, also when listed explicitly. **A race the library
already holds is never fetched again** (asked 2026-10-05), listed or not:
any search record whose race address resolves to the race's key holds it,
whatever its files (a snapshot boat that never had positions has none). A
Geovoile address without a leg holds the leg its record's name gives ("Leg
2", "(2/2)"), and any leg holds the race's own address. A race the scraper
saved beside a copy the library already held is removed, records and files,
at the next scrape's start. Races download Blue Water Tracks first, then YellowBrick, then Geovoile
(asked 2026-10-05), discovery in the same order. Requests start at least 1 s apart, 2 s pass
between races, and a busy answer (5xx, 429, a dropped connection) is tried
again after 5, 10, 20, 40, 80 and 120 s, longer when the server's
`Retry-After` asks; races still turned away are tried once more after a
60 s pause at the end. Errors name the tracker. Each boat's track is written to
`individual-tracks/<race id>/vessel/provided/<participant id>.geojson`, and the
race's `CalendarEvents`, `CompetitionUnits` (with `approximateStartLocation`
and `approximateEndLocation`), `Vessels` rows and search records are upserted
into `boat-metadata.json` under ids derived from the race and boat, so a
second scrape replaces its own records. The metadata is written atomically
every ten races and at the end, so a cancelled scrape keeps what it saved.
Progress (`library://scrape`) shows in Settings and, for a manual scrape, in
the status bar with **Cancel scrape**; a shutdown scrape keeps the app open
until it finishes or is cancelled.

The GeoJSON directory is the root of the metadata's relative storage keys. On
the development machine the verified root is
`/Volumes/Disk_Three/s3/syrf-tracks-individual-production`, containing
`individual-tracks/<competition GUID>/vessel/provided/<participant GUID>.geojson`.
The earlier `syrftracksgeojson` directory contains race collections from a
nonmatching database snapshot; it is also supported when GUIDs match. Storage
keys cannot escape the selected directory through traversal or symlinks.

When the configured metadata file is absent, the Tracks section hides the
vessel-search heading, input and results entirely (asked 2026-10-08).
A lightweight file-presence check runs on mount, metadata-directory changes,
window focus, and metadata download or scrape completion. It does not parse
or index the metadata merely to decide whether to show search.

`boat-metadata.json` (version 1) holds SYRF table rows and one search record
per track, limited to YellowBrick, Geovoile, Blue Water, old Geovoile, Regadata
and America's Cup (including 2021). A file of another version is refused with a
message.

The Tracks panel searches values in every field of the full local Vessels rows,
including name, model, class, make, builder, sail number, IDs, measurements and
custom fields. Text, numbers, booleans and nested JSON values are searchable;
nulls and field names are not treated as values. Matching ignores case, accents
and punctuation. Every query word must match the same vessel, but words may
match different fields. Matches containing the complete query in one field
(ignoring spaces and punctuation) precede matches scattered across fields:
"Cal 40" boats come before a "Pascal" whose timestamp contains "40".
Catalogue order is retained within each tier, including across pages.
Folded text is indexed once per vessel, shared by its tracks. Search loads
only vessel rows and track summaries, and retains a disposable search index
under the application's cache directory across restarts. The source path,
file size and modification time invalidate that index; missing or corrupt
indexes are rebuilt from the original metadata, which search never rewrites.
Local indexing warms on a background thread at startup and after changing
the metadata folder. Each query prepares its substring matchers once and
matches each vessel once; pagination reuses the matching row IDs in catalogue
order. File availability is checked in parallel for the visible page, with
the directory resolved once, and import revalidates every path. Typing waits
50 ms to coalesce keystrokes; an older response never replaces newer results.
Results come in pages of 100 tracks, the next loading on its own as the
list is scrolled to its end (asked 2026-10-02). Each result shows
boat, event, provider, model, sail number, date, original URL and file availability.
Importing a result preserves the query and the pages loaded so several
tracks can be added without repeating the search. A successfully added track
is removed from that search's results and count. Failed imports remain
available to retry. These removals are temporary search state; the next
page's offset is the number of rows received, so no track is skipped.
Import selects the participant GUID, reads SYRF LineStrings with millisecond
timestamps and optional speed/heading columns (including `properties.detail`),
and creates an ordinary immutable track source with undo and project persistence.
Project identity and duplicates are checked under the import commit lock.
Weather remains a separate per-track or selected-tracks action (§7.5).

### 3.5 Language, help and tooltips

The VectorEffects i18n system is copied (D10):

- English text is the key: `t("Import {count} tracks", {count})` via
  `useT()`, `msg("…")` for tables built at module load.
  A key may carry an `@@context` suffix for words whose translation depends
  on their role (the Compare stage and the Compare action); the suffix is
  never displayed.
- Catalogues in `ui/src/i18n/locales/<lang>/<area>.ts`. **English, French,
  German, Spanish, Italian, Dutch, Simplified Chinese, Japanese and Arabic**
  (the last six added 2026-10-03, machine-drafted, each with its own
  `GLOSSARY.<lang>.md`, all awaiting a native sailor's review). Arabic sets
  `dir="rtl"` on the page and the layout mirrors; scales — the wave range
  sliders, numbers with units drawn over the 3D view, the comparison and the
  split view's marks — stay left to right, as charts do in right-to-left
  software. The language picker is in Settings and on the start
  screen; changing it relabels everything immediately, including the native
  menu, which Rust rebuilds from a translated table (`menu.rs`) on every
  language change.
  Existing status messages and their translated parameters follow the new
  language too; shortcut modifiers use its keyboard names.
- `coverage.test.ts` fails on a missing, unused or untranslated key, and on
  JSX text, `title` or `aria-label` that skips `t`.
- A glossary per language fixes sailing terms (TWA, TWS, BSP, VMG, polar,
  "abattée", "Wende"…) so the same concept is always the same word. The
  French and German were drafted by the implementer (M17 preflight: no
  native-speaking sailor was available); every glossary entry carries the
  review status "machine-drafted, needs a native sailor", and
  `docs/TRANSLATION-REVIEW.md` is the reviewers' checklist.
- **Tooltips** on every control, translated. Each tooltip shows the shortcut
  if there is one.
- **Help** is a reference window of translated topics, one per area. Every
  topic exists in every language (`topics.test.ts`).
- Strings that come from Rust (error kinds, job names, history labels) are
  keyed and translated in the UI, as in VectorEffects.

### 3.6 Feature search

Copied from VectorEffects (spec §5.8 there), with the highlight in orange:

- Cmd/Ctrl-F or the search box in the title bar. Results appear **as the user
  types**, ranked by: translated label, keywords, description, help topics.
  Matching is case- and accent-insensitive in the **current language**.
- Every control has `data-feature="<area>:<name>"` and a registry entry
  (`ui/src/help/features/<area>.ts`) with label, description, keywords, help
  topic and reveal steps (`panel:left`, `section:tracks`, `stage:3d`,
  `dialog:settings` …).
- Choosing a result runs the reveal steps (opening the panel, section, stage
  or dialog that hides the control), then highlights the control with a
  flashing **orange** outline (`--flash`, `#ff8a1f`). The outline follows the
  control for 2.4 s or until the next pointer down. Motion is reduced under
  `prefers-reduced-motion`.
- `features.test.ts` fails if a tag has no registry entry or an entry has no
  tag, and if any feature is not found by its own translated label in every
  language.
- The start screen has no search box (as in VectorEffects); its Help button
  opens the help window. The help window's own controls (search, topic list,
  related pages, Close) are registered, revealed by `help:open`.
- Exempt from the registry, and only these: dynamic per-item list rows — one
  recent or recovered project, its Discard button, one Open Recent entry —
  whose containers carry the id instead; and the answer buttons of transient
  dialogs (Save / Don't save / Cancel, confirmations), since nothing can
  reveal a question that has not been asked.
- Errors are shown translated by their `kind` (§1.4); Rust's English message
  is kept only as the tooltip, and an unknown kind shows a translated generic
  line.

### 3.7 MCP service

An MCP (Model Context Protocol) server inside the application (D29), so an
AI client on the same computer can drive it. Follows VectorEffects' service;
the design is `docs/superpowers/specs/2026-10-01-mcp-service-design.md`.

- **Off unless switched on.** Settings → MCP service turns it on, which
  issues a token (32 random bytes, base64url) and opens a listener on
  `127.0.0.1` at the port in Settings (47392 by default; a taken port is
  shown as an error and the setting stays on, so the next launch tries
  again). Turning it off closes the port and clears the token. While off the
  module creates no socket, thread or task. *Rotate token* issues a new one.
  A settings file that says on without a token, or holds a token while off,
  loads as off.
- **Guards.** Streamable HTTP at `/mcp` only. Every request must carry
  `Authorization: Bearer <token>`, compared in constant time, or is answered
  `401` before any MCP handling. `Host` and `Origin` are held to the
  loopback names, so a web page cannot reach it through a rebound name.
- **Tools call the interface's own commands**, so an agent's edit is
  validated, undoable and autosaved exactly as a person's, and sources stay
  immutable behind overlays (invariant 1). A refusal reaches the client as a
  tool result with the text the interface would show, and — where the
  interface would ask — what to do instead (`discard_unsaved`, a `path`).
  Units are the domain's (knots, degrees, "from"/"toward", UTC seconds),
  never the display units. Every tool that reads or edits a boat takes an
  optional `boat` id. A structured parameter (filters, settings, an edit)
  is an object holding only what changes, also accepted as a string of JSON;
  a key that is not one of its fields is refused with the fields listed,
  never dropped. `project_save` to a new path and `export_polar` refuse to
  replace a file that is already there unless the call passes
  `overwrite: true`, as the interface's save dialog asks.

  | Group | Tools |
  |---|---|
  | Guide | `polarexplorer_guide` |
  | Project | `project_status`, `project_new`, `project_open`, `race_project`, `project_save`, `project_close`, `recent_projects` |
  | Boats | `boats_list`, `boat_add`, `boat_rename`, `boat_remove`, `boat_restore` |
  | Sources | `sources_list`, `source_set`, `source_move`, `source_remove`, `orc_search`, `orc_add`, `orr_search`, `orr_add`, `orc_refresh`, `orr_refresh`, `polar_files_import` |
  | Tracks | `track_files_inspect`, `track_files_import`, `tracker_event`, `tracker_import`, `library_search`, `library_import`, `track_set`, `track_samples`, `sample_get`, `samples_exclude` |
  | Weather | `weather_estimate`, `weather_fetch`, `weather_cancel`, `weather_jobs` |
  | Blend | `blend_status`, `blend_set`, `blend_filters_set`, `polar_read`, `blend_cell`, `polar_edit`, `compare` |
  | Export | `export_preview`, `export_polar`, `export_all` |
  | View | `screenshot`, `view_stage`, `view_boat`, `selection_set` |
  | History | `undo`, `redo` |
  | Escape hatch | `invoke` |

- **A whole race in one call** (asked 2026-10-03). `race_project` runs the
  interface's Open project from tracker… (YellowBrick, Geovoile or Blue
  Water Tracks; same model or exact boat matching), confirms its preview and
  answers the per-boat report. The guide's "A whole race" then has the
  agent search again per boat — `orc_search`, `orr_search`,
  `library_search`, `tracker_event` — with what it knows of each boat from
  the race's own pages (design, builder, sail number, other races), since
  the application matches only on the tracker's data, and tells it to
  carry a job through, stopping only for a large download, the user's
  unsaved work or a choice nothing settles. `library_search` and
  `library_import` search and import the user's track library (asked
  2026-10-03); setting the library up stays the person's, in Settings.
- **Cells are named by value.** `blend_cell` and `polar_edit` take TWA and
  TWS values; a value not on the grid is refused with the axis listed.
- **Long tools.** `weather_fetch` waits for the given tracks, reports MCP
  progress, and cancels its fetch when the client cancels the request or
  goes away (invariant 6); what was already fetched is kept. Source ids are
  each boat's own, so a fetch or a cancel without `boat` is the first
  boat's and leaves the others' alone. `tracker_event` downloads through
  `pe-trackers` as the dialog does, and `orc_refresh` and `orr_refresh`
  start the catalogues' scrapes as Settings does (§5.4). These are
  the person's agent asking, on the allow-listed hosts only (invariant 4).
- **`invoke`** runs any other IPC command by name, with unknown arguments
  refused. Not reachable through it: the service's own commands, every
  settings command (each answers the whole settings file, which holds the
  service's token), the track library's commands (choosing its folders is the
  person's; its search and import are the curated tools above), quitting, the commands that answer packed
  bytes, those that need the application handle (their tools carry them),
  and `save_project_as` and `export_polar` (their tools hold the overwrite
  rule). A test holds the table and the exclusions equal to the registered
  commands.
- **The view follows.** The service — and only the service — emits
  `document://changed` after every tool that wrote or opened, saved or closed
  a project (whether it succeeded or refused part-way), and the shell applies
  it as it applies its own results: an edit replaces the summary, a
  different project is entered. A boat a client removed leaves no pane
  behind, and one it restored is shown again. `view://stage`, `view://boat`
  and `view://selection` move what is on screen; they are not edits. A
  stage is a boat's own, so `view://stage` names its boat (the first when
  the tool was given none): that boat's stage is set and its tab shown.
  While a client is connected the status bar shows **MCP** and the last
  tool called. A client is connected from its initialisation until it ends
  its session; one that only drops its connection is counted until the
  session's idle timeout (five minutes, `rmcp`'s default), so the badge can
  outlast such a client by that long. A client on the 2026-07-28 protocol
  keeps no session: it is counted from each tool call, for the same five
  minutes after its last. Switching the service off, or rotating its token,
  leaves nobody counted at once.
- **`screenshot`** returns a PNG of the stage on show (the 3D view, the map,
  Compare, or the full-size plot): its canvas, redrawn and read in one task,
  without the panels around it. Refused at once with no project open; a
  frontend with no frame says why. Nothing is written to disk.
- **Clients.** *Add to Claude Code* runs `claude mcp add --scope user
  --transport http polarexplorer …` and writes a skill to
  `~/.claude/skills/polarexplorer/`. *Add to Codex* edits
  `~/.codex/config.toml` in place, keeping every other line, owner-only.
  *Add to Claude Desktop* (macOS and Windows) writes `PolarExplorer.mcpb`
  beside the settings file and opens it there; the bundle holds no secret
  (its bridge reads the port and token from the settings file per request)
  and follows the application through restarts. *Configuration for other
  clients* gives the address and token as text. **ChatGPT is not offered**:
  it reaches MCP servers only over public HTTPS or a tunnel, never
  loopback, and the application neither exposes itself nor runs a tunnel.
- The server's instructions open with today's date and fit the 2,048
  characters Claude Code keeps; the full manual is the `polarexplorer_guide`
  tool and the skill's body. A test holds both to the tools that exist.

---

## 4. Project model

### 4.1 Contents

```
Project
  id, name, created, schema_version
  boat: { name, notes }                        // free text
  grid: { twa: [deg], tws: [kn] }               // output grid, §12.2
  blend: BlendSettings                          // §12 (sample counts clamped
                                                //  to 1..=1,000,000 on load):
                                                //  min_samples, n_full,
                                                //  smoothing, use_corrected,
                                                //  include_stokes_drift, colour,
                                                //  visible, default_statistic,
                                                //  asymmetric, interpolation, corrections,
                                                //  global_filters, priority_groups,
                                                //  priority_min_samples
  sources: [Source]                             // ordered as the user sees them
  next_id                                       // ids never exceed 2^53 − 1
Source
  id, kind, label, colour "#rrggbb", visible, weight (0..=1, default 1)
  kind = Orc { record: OrcRecord }                     // copied from the catalogue
       | Orr { record: OrrRecord }                     // immutable certificate variant
       | PolarFile { format, file_name, polar: Polar } // parsed at import
       | Track { track: Track }
  overlay: Overlay
Overlay
  cell_overrides: [(twa, tws, bsp)]             // polar edits, §10.4
  excluded_cells: [(twa, tws)]                  // polar nodes removed, §10.3
  excluded_samples: [SampleId]                  // track dots removed, §10.3
  filters: SampleFilters                        // track sources only, §7.6
Track
  origin: { tracker, event_url, event_title, boat_id, boat_name, sail_no,
            model?, division?, race_start?, race_finish? } | File { name }
  fixes: [Fix { t, lat, lon, cog?, sog? }]      // as imported
  derivation: DerivationSettings                // §7.4
  statistic: median | mean | p75 | p90          // §12.1
  samples: [Sample]                             // §7.5, derived + fetched env
  env_meta: { datasets: [(name, version, fetched_at)] }
```

### 4.2 New project

The form has one field, **Project name**, required. The project starts with
one boat tab, named on the tab afterwards. The grid and blend settings start from defaults (§12.2) and are
changed later from the Blend section of the source list. A new project opens
straight into the project window with the 3D stage.

### 4.3 File format: `.wpsproj`

Same container rules as VectorEffects' `.veproj` (D11):

- A ZIP (deflate) with:
  - `META-INF/version`: the schema version in plain text, so a newer file can
    be refused before parsing.
  - `project.json`: canonical JSON, stable key order, everything except bulk
    track data.
  - `tracks/<track id>.json`: one entry per track, holding its fixes and
    its samples by column, only what is not derived (schema 2, §7.5; schema
    1 wrote one object per sample and is migrated on open). Kept separate so
    `project.json` stays small and diffable.
  - `boats/<boat project id>/tracks/<track id>.json`: additional boats’ bulk
    data (schema 6). The first boat retains the original track paths.
    `project.json` stores additional boat documents under `boat_tabs`; tabs
    cannot be nested. Each boat owns its own sources, grid, IDs and overlays.
    Schemas 1–5 open as one boat without changing imported data.
  - Schema 7 holds source weights in 0–1 (§8). A file of schema 6 or
    earlier with a weight above 1 opens with that weight at 1, in every
    boat; nothing else in it changes.
- Every entry has a fixed timestamp, so saving an unchanged project twice
  gives identical bytes.
- No rendered images and no blend results (invariant 2). A test fails on any
  unexpected entry, and opening refuses an archive that holds one (or lacks a
  track's entry) rather than dropping it silently on the next save. The one
  exception is operating-system litter from re-zipping or browsing the file
  (`__MACOSX/…`, `.DS_Store`, `Thumbs.db`), which is ignored on open and
  never written back.
- Floats go through the canonical helpers.
- Saves are atomic: write `<name>.wpsproj.tmp`, then rename. The same
  helper writes `settings.json` and autosave manifests.
- The document is validated as it will be read back (after canonical
  rounding) before anything is written, so a project that could not be
  reopened is refused at save time.
- Save As appends `.wpsproj` unless the name already ends in exactly that
  (`Race.v2` → `Race.v2.wpsproj`).
- Migrations: `MIGRATIONS: &[(u32, fn(&mut Value) -> Result<()>)]`, keyed by
  the version each step migrates from. Newer files are refused with a message
  naming both versions.

### 4.4 Recent projects

The ten most recent, stored in `settings.json`, updated on open and save.

### 4.5 Autosave and recovery

Copied from VectorEffects: in recovery mode the dirty project is written to
`<data_dir>/autosave/<project id>.wpsproj` every 60 s or every 50 history
entries. A clean save or close deletes it. After a crash the start screen
offers it under Recovered work; it opens dirty at its original path. The
snapshot is written beside its final name and renamed into place under the
session lock, after checking that the project is still open, unsaved and
not on its way out, so a Save, Close or "Don't save" during the write never
leaves stale work to be offered back. A running environment fetch dirties
the project as it writes, so its results are snapshotted too.

### 4.6 Undo and redo

Every document change is a `Command` with `apply` and `undo`, in a history of
200 entries. Drags (3D node edits, weight sliders) coalesce into one entry.
Selection, camera and stage changes are not recorded. Imports are undoable
(undo removes the source). Fetched environment data survives undo of a
filter change because it lives on the track, not in the command.

---

## 5. ORC polars

### 5.1 Catalogue

The ORC catalogue is **embedded in the app** (D3):

- Built by `tools/orc-catalogue-builder` from jieter/orc-data's per-boat files
  (`site/data/<country>/<sail>.json`, about 18,000 boats across all years,
  MIT licence). `ALL2025.json` is not used as a source of records: it is
  Python `repr`, not JSON, and holds 34 boats.
- The builder reads the files **from the checkout's `HEAD` commit** through
  `git`, not from the working tree: orc-data has file names that differ only
  in case (`FIN/FIN71.json`, `FIN/Fin71.json`), which a macOS or Windows
  checkout collapses into one.
- The builder keeps: sail number, country, name, type/model, builder,
  designer, year, the certificate year, the size fields (LOA, beam, draft,
  displacement, sail areas, crew), GPH/OSN, and the VPP (`angles`, `speeds`,
  BSP per angle per TWS, beat and run angles and VMGs). Speed axes differ by
  year (6–20 kn up to 2023, 6–24 kn in 2024, 4–24 kn from 2025); all are kept
  as given. Sizes of 0 (orc-data's "no spinnaker") are kept as absent.
- The sail number is shown once with its country: orc-data's `GBR/GBR1124`
  is `GBR 1124`; its stand-in for a missing number (`GBR/_3`) is empty.
- **Certificate year.** The per-boat files do not state it. The builder reads
  it from the VPP's wind-speed axis (4–24 kn: the latest yearly list from
  2025 holding the boat, else the year the checkout's Makefile fetches; 6–24
  kn: 2024; 6–20 kn: the latest of the yearly lists `ALL2019`–`ALL2023`
  holding the boat's sail number, of which only the sail numbers are read).
  When none of this identifies a year it is left empty rather than guessed
  (about 700 boats, all certificates older than 2019).
- **Dropped records.** The builder reports every file it leaves out and why
  (not JSON, VPP shape or axis wrong, a speed that is negative or above
  60 kn, values finer than 0.01, a file identical to another). At orc-data
  `c2ca870c` it read 18,141 files, kept 18,135 and dropped 6: four with
  negative boat speeds and two exact duplicates.
- Output: a compact binary (`catalogue.bin`, postcard records in one LZ4
  block, speeds and angles as whole hundredths, which is exact for
  orc-data's two decimals) in `crates/pe-orc/data/`, about 5.7 MB. It is
  embedded in the binary and decoded and indexed on first use (about 0.2 s).
  Its small uncompressed header holds the orc-data commit, the commit's date
  and the build date (the commit's date unless `SOURCE_DATE_EPOCH` is set,
  so rebuilding one commit is byte-identical); About shows them (the native About panel), and the
  ORC polars section's footer shows the certificate count and commit date.
  About and the searchable help also credit jieter/orc-data (MIT), ECMWF /
  Copernicus ERA5, WeatherBench2, ARCO-ERA5, Copernicus Marine and Natural Earth
  in all three interface languages. Every installed bundle includes the user
  guide, detailed data notices and ORC MIT notice in its documentation resources.
- Rebuilding the embedded catalogue is a developer task and a new release.
  Between releases, a scrape of ORC's own service adds the current
  certificates beside it (§5.4); searching and adding never fetch.

### 5.2 Search

- One search box that matches **across all fields**: boat name, sail number,
  country, model/type, builder, designer, year built and certificate year.
  Tokens are ANDed, and each must match the **start of a word** of some field,
  so `farr 40 2023` finds Farr 40s from 2023 and `arr` finds nothing. The name
  is also indexed without its punctuation, so `oneil` finds O'Neil.
- **Search by field** (D26). Under the box, a disclosure — folded by
  default, open or folded remembered per user in `localStorage` like the
  panels — holds one box per field: boat name, sail number, country (a list
  of the catalogue's codes), model / type, builder, designer, year built
  (from, to) and certificate year. Every box that holds a word must match,
  and so must the main box. Within a box, tokens are ANDed and each must
  start a word of **that field only**, with the same folding and compact
  forms as the main box (the sail number box takes `GBR1124`, `GBR 1124`,
  `GBR/1124` or the number alone; the name box takes `oneil`). The
  certificate year box matches the year from its start, so `202` is 2020–2029.
  Folded, the disclosure's heading shows how many fields are in use
  ("Search by field (2)"; year built counts once); **Clear** empties every
  field and keeps the main box. The field boxes are not saved.
- Results update **as the user types**, within 30 ms of each keystroke, best
  first. Ranking: a field box equal to its whole field (folded, separators
  removed; for the sail number, with or without its country), then exact
  sail number of the main box (with or without its country), then name
  prefix, then model prefix, then other token matches; within each, newer
  certificates first, then by name.
- Case- and accent-insensitive (decomposed accents and `İ` included); accepts `GBR1124`, `GBR 1124`, `GBR/1124`.
- Each result shows name, sail number, model, year, builder, the certificate
  year and a small polar thumbnail (light, medium and strong wind: the
  certificate's wind speeds nearest 6, 12 and 20 kn). Year built (from, to)
  leaves out a boat without a year while either is set. An empty main box
  with every field empty lists nothing; otherwise it lists what the fields
  admit. **Both catalogues at once** (asked 2026-10-02): the ORC and the ORR
  catalogue (§5.5) answer the one search, each hit badged ORC or ORR, the
  totals added; the catalogue picker is gone. The two lists are interleaved,
  each in its own rank — the best ORC hit, the best ORR hit, the next of
  each — so neither is buried under the other's pages (asked 2026-10-03,
  when ORR hits sat below every ORC page loaded). Fifty results of each
  catalogue make a page, and the next page loads on its own as the list is
  scrolled to its end (an IntersectionObserver on the list's last row; a
  More button where there is none), appended until both are exhausted. The
  **Measurements** bounds under Search by field are folded until asked for,
  each row the measurement's name, then its Min and Max boxes under those
  headings. Crew weight is not offered as a bound (asked 2026-10-03); the
  search command still takes it, for the MCP service.
- Measured in M5 on the development machine (Intel i9, debug build): 188
  keystrokes over the full catalogue, median 1.2 ms, p99 2.4 ms. In M14d,
  with the per-field keys and 470 more keystrokes typed into single fields
  (alone, beside the main box and beside a second field): 658 keystrokes,
  median 1.6–1.7 ms, p99 2.5–2.6 ms; building the index on first use went
  from about 0.18–0.24 s to about 0.27 s.

### 5.3 Adding and removing

- **Add** puts a copy of the record in the project as an ORC source with the
  next palette colour, labelled with the boat's name (else its model, else
  its sail number); one undo takes it back out. Any number can be added;
  adding the same certificate twice asks first. orc-data gives no
  certificate number, so "the same certificate" is the same country, sail
  number, name, model, year built and certificate year. A result the project
  already holds is marked **Added**.
- The added list sits under the search. Each entry has **Remove**
  (undoable) and shows its colour, sail number, model and certificate year.
- The ORC VPP converts to a polar on the ORC angles (52°–150°) plus the
  beat and run angles per TWS, on the certificate's own wind speeds. At a
  beat or run angle only its own wind speed has a value, the VMG divided by
  the cosine of the angle (by the cosine of 180° less the angle for a run);
  where it falls on a table angle, the table's value stays. Nothing is
  invented outside the angles ORC gives; the blend (§12) handles gaps.

### 5.4 Scraping ORC certificates

The embedded catalogue is as old as the build. **Settings → ORC polars**
adds the current certificates to it from ORC's own service (D31).

- **Source.** `https://data.orc.org/public/WPub.dll?action=DownRMS`, ORC
  family, one JSON document per country for the current VPP year: the
  service the embedded catalogue comes from by way of jieter/orc-data. It
  serves only the current year (another year answers no certificates, in
  which case last year's is asked for once, for the first days of January).
  Countries are the ones the service itself lists. Only `pe-trackers`
  contacts `data.orc.org`; a scrape is about 60 MB in a minute or two
  (7,730 certificates from 33 countries in 84 s, October 2026), one country
  at a time, and nothing downloaded is kept.
- **A certificate is read as the embedded ones were**: the sail number in
  the catalogue's one form, speeds as `3600 / allowance` to two decimals,
  beat and run angles and VMG per wind speed, sizes and ratings, plus the
  certificate's reference number (`RefNo`), which orc-data does not carry.
  A certificate that cannot be a polar (a table with a row short, an
  allowance of zero, no reference) is left out and named in the details; a
  country that does not answer is named and the others go on.
- **No duplicates.** Scraped certificates are kept in
  `orc-catalogue.bin` beside the settings, in the catalogue's own format.
  **A certificate is its reference number**: scraped again, it replaces the
  stored one in its place, so scraping again stores nothing twice and
  changes nothing. Two valid certificates of one boat in one year are two
  certificates (ORC issues a crewed and a double-handed one, or two sail
  configurations: 14 boats in October 2026); both are listed, each with its
  number in the results. A stored certificate of the scraped year that its
  country's list no longer names was issued again under a new number or
  revoked, and is **withdrawn**: out of the catalogue, counted as removed.
  A country that did not answer, or answered nothing, withdraws nothing.
  In search, a scraped certificate that is the same certificate as an
  embedded entry (country, sail number, name, model, year built and
  certificate year) takes that entry's place, the scraped data standing
  for both. **Another year's certificate of the same boat is another
  certificate** and is listed beside it, as for ORR; other years are never
  touched by a scrape. Sources already in a project are copies and are
  never changed.
- **Ids stay put.** Embedded entries keep their catalogue ids with or
  without a scrape, scraped ones are only appended, and a withdrawn one
  keeps its place (it is found by no search and cannot be added), so a
  result on screen or held by an MCP client still names the same
  certificate after a scrape. The search refreshes when one finishes.
- **A scrape is stored whole or not at all**: cancelling, or a failure of
  the whole job, leaves the catalogue as it was. A store that cannot be
  read costs the scraped certificates until the next scrape, never the
  embedded catalogue.
- **Settings.** *Scrape ORC polars* and *Cancel download*, with the
  countries done and the certificates read, then what was added, updated,
  removed and left out. It continues while Settings is closed and is independent
  of the open project. Opening Settings fetches nothing.
- **Schedules.** Each catalogue, ORC and ORR, has *Download automatically*:
  **Manually only** (the default), **On startup** or **On shutdown**. A
  scheduled scrape is the person's standing request (invariant 4). It asks
  for the current year, and is skipped when that catalogue's store was
  written less than 24 hours ago. On startup it runs in the background. On
  shutdown, after the unsaved-changes guard, quitting waits for it and the
  status bar says so; asking to quit again cancels the scrape and quits as
  soon as it has stopped, the catalogue as it was. If the project was
  edited meanwhile, the guard asks again.
- The MCP service's `orc_refresh` starts the same scrape (§3.7).

---

## 6. Polar files

- **Import…** opens a native picker allowing **multiple files**. Each file
  becomes one source, labelled with its file name.
- Formats, detected from content (not only the extension):
  - **Expedition** (`.txt`): `!` comment lines; each row is `TWS` followed by
    `TWA BSP` pairs, whitespace-separated; rows may have different lengths. A
    leading row made only of words such as `twa`, `bsp`, `tws`, `pol`, `up`
    or `dn` — joined (`TwaUp`, `UpBsp`) and/or followed by digits (`twa0`,
    `Bsp1`), case-insensitively — names the columns instead of holding a wind
    speed's data, and is skipped.
  - **Adrena / grid** (`.pol` tab-separated, `.csv` semicolon- or
    comma-separated): top-left cell `TWA\TWS`, `TWA/TWS` or `TWA`; first row
    TWS values; first column TWA values; cells BSP in knots.
- A parse error names the file, line and column and imports nothing from that
  file. Other files in the same batch still import. Everything a batch
  imports is one undo entry.
- Boat speeds (BSP) above 60 kn, wind speeds (TWS) above 70 kn, or negative
  values of either, are refused as a format error. A TWS of 0 (a column of
  zero boat speed) and a TWA of 0 (a row of zero boat speed) are ordinary
  zeros, not refused.
- Reading details:
  - Text is UTF-8 (with or without a BOM), UTF-16 with a BOM, or else
    Latin-1; CRLF, CR and LF line endings are all accepted. Blank lines and
    `!` comment lines are skipped in both layouts.
  - Expedition rows keep their own angles: the polar's TWA axis is the union
    of every row's angles, and a row that has no point at an angle leaves that
    cell empty. Rows and pairs may come in any order.
  - Table cells may be quoted, trailing separators are ignored, an empty cell
    or a short row is an empty cell, and a row longer than the header is an
    error. In tab- and semicolon-separated files (and Expedition), a comma
    inside a number is its decimal point (`5,25`).
  - Angles in (180°, 360°] preserve the independent port side. Symmetric
    display mode averages opposite original nodes as a derived operation;
    full-circle display and exports retain both sides.
    Angles below 0° or above 360° are refused. The same TWS twice, or the
    same written TWA twice in a row or table, is refused.
  - Files over 4 MB, or with more than 512 distinct angles or wind speeds,
    are refused as not being polars.
- Writing (used by export, §12, and pinned by golden files): `\n` line
  endings, axis values with at most two decimals and no trailing zeros,
  boat speeds with exactly two decimals. Expedition: a `!` comment line, then
  one tab-separated row per TWS holding only the cells that have a value.
  Adrena: `TWA\TWS` top-left, tab-separated, empty cells left blank. CSV: the
  same with semicolons.
- Imported files are listed with colour, format and axes. **Remove** is
  undoable.

---

## 7. Tracks

### 7.1 Track list

The Tracks section lists every track source: colour, boat name, event title,
date range, sample count, and a weather status (not fetched, queued,
fetching n %, ready, no points to plot, partial, failed). A completed fetch
with no samples that have enough wind, heading and speed data to place on the
polar says **no points to plot**, rather than ready; it can still be refetched.
Weather status has its own wrapping line so a narrow sidebar cannot hide it.
Each has a tick box, **Fetch
weather…** (Cancel fetch while it runs), Show on map and Remove.

Buttons above the list: **YellowBrick…**, **Geovoile…**, **Blue Water…**,
**File…**; and, once there are tracks, **Select all** and **Fetch weather for
selected tracks…**, over the ticked tracks. Select all ticks every imported
track and is disabled when all are already ticked. It does not start a download;
the weather button starts fetching immediately, leaving queued/running tracks out.
Several events and several files can be
imported into one project, and several boats from one event. Importing never
fetches weather (D24): an imported track shows "Weather: not fetched" until
the user asks for it.

### 7.2 Tracker import

All three trackers share one dialog flow (D4, updated 2026-10-08):

1. **Open loads only the boat list**, with title, dates and boat details,
   without downloading track positions. YellowBrick reads RaceSetup;
   Geovoile reads the viewer page, versions and config, never tracks or reports.
2. Search by name, sail number, model or division and tick boats. Unknown
   position counts show a dash. **Import tracks starts the position download**,
   then adds the chosen boats as track sources in one undoable change.
3. Blue Water Tracks combines boat details and positions in one response.
   Open resolves its address without a request. The first **Import tracks**
   downloads the event and shows the boat picker; after selecting boats,
   **Import tracks** adds them without another download.
4. Complete events stay in memory for the session, so reopening one downloads
   nothing and shows position counts and a map preview. **Refresh boat list**
   reloads metadata only; the next Import tracks downloads fresh positions.
5. No weather is fetched. The user starts it per track, or for the ticked
   tracks, from the track list (§7.1, §7.5).

In detail (M10):

- Each tracker is a `TrackerClient` in `pe-trackers`. Resolving an address
  needs no network. `list` reads only metadata, or returns `None` for a
  provider with a combined response. The app returns a `positions: false`
  view and does not cache incomplete events. `fetch_listed` still fetches
  the whole event for an explicit download, the library and project imports.
  Independent full-download requests run concurrently (YellowBrick's
  RaceSetup and AllPositions3; Geovoile's config, tracks and reports after
  page and versions), each decoded on its own thread, with gzip support
  under the same 256 MB cap. Early `tracker://listed` updates carry the
  dialog's download key, and updates for other downloads are ignored.
- The picker searches name, sail number, model and division. Every word
  must match; case, accents, spaces and slashes in sail numbers do not
  matter. A heading checkbox ticks all boats shown. Before downloading,
  every listed boat can be selected; on a complete event, boats with no
  positions are disabled. Missing boats or positions discovered during
  import are reported by the normal per-boat import failures.
  A complete event has a map preview with at most 64 points per boat,
  rounded to 1e-4°, over the basemap, with selected boats highlighted.
- The download runs as a job (§7.7), with progress and Cancel in the dialog.
  Cancel stops the download and closes the dialog, and a late answer cannot
  start an import. While importing, selection, event, leg and refresh controls
  are disabled. A position failure retains the boat list, search and choices;
  Retry downloads that event and imports those choices, even if the address
  field was edited. Open starts a new list with fresh choices.
  Only complete events enter the session cache. A 5xx/429 response, timeout
  or dropped connection is retried three times (0.5, 1, 2 s); other failures
  stop at once. Redirects stay on the same host or an allow-listed HTTPS
  tracker host. Failure text is translated, and an unsupported address has
  no Retry.
- The session keeps up to two million positions (about 110 MB), removing
  the oldest events first and always retaining the latest.
- Import adds one track per chosen boat, labelled by boat name, as one undo
  entry. The origin records tracker, canonical event address, title, boat id,
  name, sail number, model, division, start and finish. Its time window uses
  the boat's start/finish when available, otherwise the event's (§7.6).

**YellowBrick** (host `yb.tl`, CDN `cf.yb.tl`):

- Race key = the first path segment of `https://yb.tl/<key>` (also accepts
  `cf.yb.tl`, `app.yb.tl` viewer links, and a bare key).
- `GET /JSON/<key>/RaceSetup` (ISO-8859-1): title, start, stop, `teams[]`
  (id, name, sail, model, type, tags, status, and the team's own `start`
  and `finishedAt`), `tags[]` (id, name, sort, show, and a `start` for the
  divisions that start separately). A boat's **division** is the names of
  its tags that have their own start (e.g. "IRC 2"), else of all its shown
  tags, in tag order. Its status is FINISHED when it has a `finishedAt`,
  else the team's status.
- `GET /BIN/<key>/AllPositions3`: the full history, decoded in Rust. Big-endian.
  `u8 flags` (bit0 altitude, bit1 DTF, bit2 lap, bit3 percent), `u32 refTime`,
  then per team `u16 id`, `u16 count` and `count` moments newest first. A
  moment whose first byte has the high bit clear is absolute
  (`u32 t, i32 lat, i32 lon` in 1e-5 degrees, then the optional fields);
  otherwise it is a delta from the previous (newer) moment
  (`u16 dt & 0x7FFF, i16 dlat, i16 dlon`, then optional deltas).
- YellowBrick gives no speed or course; both are derived (§7.4).
- Once RaceSetup loads, fallback if the binary fails to decode, is refused,
  stays unavailable after retries, or is a web page (which is what an unknown
  key's `AllPositions3` answers with status 200):
  `GET https://yb.tl/<key>.kml` (the CDN answers 504 for it). YellowBrick
  builds it on request, so it has its own 10-minute timeout (Middle Sea
  Race 2024: 23 MB in 17–73 s; Fastnet 2025: 99 MB, over two minutes).
  Each placemark (named after the team) holds a `gx:Track` of `when` and
  `gx:coord` (`lon,lat,alt`) pairs; coordinates are rounded back to
  YellowBrick's 1e-5° grid and placemarks are matched to teams by name. The
  KML leaves out the binary's few reports at a repeated time. A cancellation
  or a failed RaceSetup does not fall back; a working boat list with an
  unavailable binary does, since the site's KML can still be available.
- Some keys return 5xx (an unknown key's `RaceSetup` answers 500); the
  dialog says the tracker is not answering and offers Retry. A `RaceSetup`
  that is a web page is "no public event at this address".
- Direct track import does not need app credentials. Catalogue discovery may
  use the person's configured credentials and free-race associations (§3.4.1).

**Geovoile** (hosts `*.geovoile.com`):

- The user pastes the viewer URL (`https://<sub>.geovoile.com/<root>/tracker/`
  or `/viewer/`, with an optional leg). The app reads the viewer HTML for
  `rooturl`, `resourcesurl` and the four 24-bit hwx seeds.
- `GET <root>tracker/resources/versions/v` (a JS object literal), then
  `resources/[leg<N>/]<type>/v<ver>` for `config` (XML: boats, names, sail
  numbers, colours, legs) and `tracks` (JSON). If `resourcesurl` is set, the
  path is `<resourcesurl>[leg<N>_]tracker_<type>.hwx?v=<ver>`.
- The hwx decoder (xorshift keystream from the seeds + LZSS) is written in
  Rust from the verified algorithm in `plan.md` Appendix A. Seeds are always
  parsed per site.
- Tracks: `loc` holds `[t, lat·1e5, lon·1e5]` then `[dt, dlat, dlon]`
  deltas. `reports` supplies official heading and speed where present; they
  are used as `cog`/`sog` when non-zero.
- Only the modern `tracker/` generation (about 2016 on) is supported. Flash
  (`.hwz`) and 2012–2015 HTML trackers are refused with a clear message.

In detail (M11):

- The address is `https://<sub>.geovoile.com/<root>/tracker/` or `/viewer/`
  (scheme optional, `tracker/` optional, a trailing page name ignored), the
  root one or more plain path segments, and `?leg=<n>` (1–99) choosing a
  leg. The host is compared exactly (a subdomain of `geovoile.com`, no
  user name, no port), so look-alikes are refused before any request. The
  event's key is `<host><root>[?leg=<n>]` and its canonical address the
  viewer page, `https://<host><root>tracker/[?leg=<n>]`.
- One download is five requests: the viewer page (with `?leg=<n>` when
  asked), the versions, the config, the tracks and the reports. The page
  gives `rooturl`, `resourcesurl`, `versionsurl`, `nblegs`, `numleg` and the
  seeds, which may be spread over several `data:image/png` sources (Route
  du Rhum 2018). Every resource address is resolved against the page and
  must be HTTPS on a Geovoile host, so a `resourcesurl` or `versionsurl`
  pointing elsewhere is refused before it is requested. The versions file
  is read as the viewer reads it (`…/versions/v<now rounded to 5 s>`, or
  `versionsurl`); a version it does not name, or an empty versions file
  (Route du Rhum 2018 and 2022), is 0 — the number is only a cache-buster.
- A race in legs: a pasted address without a leg opens the leg the page
  shows (its current one), and the event records that leg. The dialog then
  offers every leg (Leg n of N); choosing one downloads it. A leg beyond
  `nblegs` is "no public event". The title is the config's name, with
  `(n/N)` for a leg.
- The config gives each boat's name, sail number and class (`boatclass`,
  shown as its division) and each class's run, whose start is the boat's
  start (else the config's `date`). The model is not given.
- The reports (`columns` then `history[].lines`, fields found by column
  name because their order differs between editions) give each boat's
  latest status (RAC and STA racing, ARV finished, RET retired; others as
  written), its official arrival (`arrivals`) or the time it was hidden
  (`hidden`), which ends its time window (else the event's end, the last
  position of any boat), and per report its heading and speed. Each report
  describes the fix nearest to it within 60 s; a fix takes the nearest
  report, and its heading and speed are used as `cog` and `sog` when
  non-zero (heading 0–360, speed under 100 kn). A reports file that does
  not load or decode leaves these to be derived; a cancel or a tracker
  that keeps failing still ends the download.
- Refusals: a page with no `rooturl` that is a web page is an older
  tracker ("Flash, or Geovoile before about 2016"), as is an address
  naming a `.hwz`/`.swf` file; a 404 or a page that is not HTML (the
  Vendée Globe 2024 answers "Not available") is "no public event"; a
  resource that does not decode with the page's seeds to the expected XML
  or JSON, or whose first or any fix is not a plausible time (2000–2100)
  and place, is "unsupported Geovoile version". None of these offers Retry.
- The viewer's `live` resource (the newest positions of a race under way)
  is not read: the tracks file is what a finished race needs.

**Blue Water Tracks** (host `api.bluewatertracks.com`):

- The user pastes `https://race.bluewatertracks.com/<slug>` (the slug is the
  last path component), an `api.bluewatertracks.com/api/race/<slug>` link, or
  a bare slug.
- `GET https://api.bluewatertracks.com/api/race/<slug>`: `race` (raceName,
  raceStartTime, trackTimeFinish, and `boats[]`: boat_id, boatName, sailNo,
  design, handicaps, each a rating system's name, rating and division) and
  `positions[]` as GeoJSON `Point` Features with `properties.{boat_id, date,
  sog, cog}` (the third coordinate, when given, is altitude and is ignored).
  A boat's division is the distinct division values across its handicaps,
  joined.
- An unknown slug is not a 404: the API answers 200 with `race` an empty
  array instead of an object (`{"positions":[],"race":[]}`), which is read
  the same as no public event; an ordinary 404 is treated the same way.
- SOG and COG are given for every position and used as they are (unlike
  Geovoile's official reports, a value of exactly 0 is not treated as
  absent). The event's start is `raceStartTime`; a boat's time window ends
  at its own `finishTime` when given, else at `trackTimeFinish`.
- Positions are not guaranteed sorted or free of duplicate timestamps per
  boat; they are sorted and merged per boat exactly as a file import's are
  (spec.md 7.4) before the dialog ever sees them.

### 7.3 File import

- **GeoJSON**: a FeatureCollection of Point features, or LineString features
  with per-vertex times in `properties.times` / `properties.coordTimes`.
  Point properties recognised (case-insensitive): `time`/`timestamp`/`date`
  (ISO 8601 or epoch seconds/ms), `cog`/`heading`/`hdg`/`course`,
  `sog`/`speed`/`bsp`/`stw`, `boat`/`name`. Features are grouped into one
  track per boat name.
- **CSV**: header row required. A column-mapping step guesses time, lat, lon,
  heading, speed and boat columns from the header and a preview, and lets the
  user correct them. Time formats: ISO 8601, epoch seconds or ms, or a user
  format string (`%Y %y %m %d %H %M %S %f %b %z`). Speed unit selectable
  (kn default, m/s, km/h, mph). The separator (`,` `;` or tab) is taken from
  the header, quoted fields follow RFC 4180, and a decimal comma is read when
  the separator is not a comma. The file is re-read as the mapping changes,
  so the dialog shows the boats (or the error) the mapping gives.
- Times without a zone are UTC. A zone offset must be within ±14:00;
  24:00:00 is read as the next day's midnight, and no later time of day is.
  Longitudes written 0–360 are accepted and
  folded to [−180, 180); in-range values are stored bit for bit as read.
  GeoJSON MultiLineString features with one list of times per line (as GPX
  converters write) are read too. A file that is not UTF-8 is read as
  Latin-1.
- A file that cannot be read is reported with its line and column (CSV and
  JSON syntax) or its feature number (a GeoJSON feature that is not a usable
  position), and nothing of it is imported; the other files still import.
- Several files can be chosen at once. A file with several boats offers the
  same boat picker as a tracker event. All the tracks of one import are one
  undo entry. The import summary gives, per track, the positions kept, how
  many were out of order and sorted, how many duplicate times were merged,
  and how many headings and speeds were given or derived.

#### Supplied true wind

CSV mapping includes optional TWS and TWD columns and a separate wind speed unit.
GeoJSON accepts `tws`/`twd` (also true_wind_speed/true_wind_direction) on Points
and matching per-vertex arrays on LineString/MultiLineString. SYRF detail maps
may identify TWS/TWD coordinate columns. Directions are true degrees **from**
north; speeds are converted to knots. A complete supplied pair takes precedence
by default, falling back to downloaded weather for other points. Track details
can choose downloaded weather only, undoably. Both sources survive weather
refetch, rederivation, save and reopen. Schema 5 adds optional fields; older
projects keep their previous result.

### 7.4 Deriving heading and speed

Every track is reduced to timestamped fixes. Then, per fix:

- **Heading.** If the track supplies heading or COG, use it. Otherwise use
  the heading *at* the fix (central difference): the circular mean of the
  great-circle bearing arriving from the previous fix (its final bearing)
  and the one leaving for the next (its initial bearing). The first fix
  uses the initial bearing to its one neighbour, the last the final bearing
  from it, so on a long leg each end has its own heading (along 60°N from
  0° to 10°E: 85.7° leaving, 94.3° arriving). Where the positions used are
  the same place (a stationary boat), or the two bearings point opposite
  ways, there is no heading.
- **Speed.** If the track supplies SOG or boat speed, use it. Otherwise
  (distance(prev, this) + distance(this, next)) / (t_next − t_prev).
- Duplicate timestamps are merged, and out-of-order fixes are sorted, before
  derivation. Both are reported in the import summary. Of several fixes at
  one time the first in the file is kept; a heading or speed only a later
  one gives is kept with it.
- The track records per fix whether heading and speed were **given** or
  **derived**, so the user can filter on it.

Derivation settings (per track, editable later, undoable): maximum gap
between neighbours used for a central difference (default 3 h, 1 s–24 h). A
neighbour further away in time than the gap is not used: a fix with one
usable neighbour uses that one (as the first and last fixes do), and a fix
with none gets no derived values. And whether to prefer the given or the
derived value — **for the heading and for the speed separately** (asked
2026-10-02: a file may give a sound course and a poor speed); with
"derived", a fix with nothing to derive from keeps its given value.
Changing any of them re-derives every sample's heading and speed, as one
undo entry that restores the previous values exactly. The interface offers
each preference only where the track gives that quantity (§7.6).

### 7.5 Environment for each sample

Every track sample can be matched against reanalysis, as a background job
(§7.7), **when the user asks for it** (D24): a track's **Fetch weather…**, or
**Fetch weather for selected tracks…** over the ticked tracks, starts the
hourly background fetch immediately, without a download-size calculation
or confirmation dialog (requested 2026-10-07).
A ticked track already queued or fetching is left out, and the ticks are
cleared once the fetch starts.
Importing a track (from a file or a tracker) never starts it and makes no
reanalysis request. Repeated clicks are disabled while the start request is
pending; a failed start reports its error and keeps the selected tracks for
retry. Progress and cancellation remain in the status bar and track list.

With **Whirlwind (Fast) S3** selected, the route reader uses the archive's Zarr v3
metadata, coordinate axes and parameter names. All three Whirlwind sources
use the combined layout published on R2: `data` contains `u10`, `v10`,
`ucur`, `vcur`, `wave_direction`, `wave_height` and `wave_period`, mapped by
the `param` coordinate names. There are no separate wave-array requests. Values
are float16, stored in Zstandard-compressed inner chunks inside rectilinear
shards. The currently published inner chunks are 72 hours × all parameters ×
40 × 40 cells (10° × 10°). Only inner chunks intersecting a requested sample's
spatial and temporal interpolation stencil are downloaded. It reads each
required shard's checksummed index using a suffix Range request, then each
required inner chunk using its exact byte range. It never downloads a complete
shard or scans the bucket. Duplicate chunk requests are coalesced across
positions and selected tracks, and the bounded session memory cache reuses
indexes, compressed chunks and decoded float16 chunks across routes, including
when a later request needs different parameters
from the same chunk. Whirlwind additionally keeps validated compressed chunks
in the bounded disk cache in Settings (§3.4), including across restarts.
S3 reads use an async runtime with up to 64 requests in flight across tracks
and parameters. Each shard's chunks can download as soon as its index arrives,
without waiting for unrelated indexes. Decoding uses at most eight CPU workers;
disk-cache reads, checksums and writes run concurrently under the cache limit.
Up to 128 queued tracks using the same source share time-ordered batches of up
to 72 hours per track and 10,000 total samples, planning and decoding each required chunk
once per batch. Different Stokes choices are sampled separately. Progress and
cancellation remain per track, and project updates are coalesced per batch.
Cancelling one track leaves shared work running for the other tracks; cancelling
all tracks interrupts pending HTTP reads and retry waits. Completed batches
stay saved and resume skips their samples. A failed shared read is retried in
per-track subsets to isolate the failure. Missing objects and
inner chunks are missing data, while authentication errors and corrupt data
fail the fetch. The optional MCP estimate counts actual compressed byte lengths from the
needed indexes, counting each combined chunk once. Whirlwind provenance is
`whirlwind-hindsight`, `whirlwind-hindsight-r2` or `whirlwind-hindsight-tigris`,
version `hindsight-v3`; its current is the HistorySyncer GlobCurrent tidal current.
Public archive network concurrency remains the existing setting.

With **Open Data (Slow)** selected:

| Quantity | Dataset | Variable |
|---|---|---|
| 10 m wind u, v | **WeatherBench2** ERA5, hourly 0.25° (D12) | `10m_u_component_of_wind`, `10m_v_component_of_wind` |
| Significant wave height | **ARCO-ERA5**, hourly 0.25° | `significant_height_of_combined_wind_waves_and_swell` |
| Mean wave direction (from) | ARCO-ERA5 | `mean_wave_direction` |
| Surface current, total incl. tides | see §7.5.1 | u, v |

- Values are interpolated bilinearly in space and linearly in time. Wind and
  current interpolate their u and v; wave direction interpolates as a unit
  vector. Land cells (NaN) are ignored in the stencil; if all four are NaN the
  value is missing.
- **WeatherBench2 ends on 2023-01-10.** For times after its last hour, wind
  comes from ARCO-ERA5 (same variables, same grid, same model). Each sample
  records which dataset supplied it. See D12.
- From these, each sample stores: TWS (kn), TWD (from), TWA (0–180),
  tack side, Hs (m), wave direction (from), wave angle relative to the bow,
  current speed and direction (toward), and dataset ids. Units are
  converted once, on ingest (1 m/s = 3600/1852 kn). The track records each
  dataset's name, version (the store's dated name) and fetch time, and
  whether its current has tides; each sample records which supplied its
  wind, waves and current, and whether the fetch has answered for it
  (what Fetch weather… resumes from).
- **What is stored** (schema 2, D27): per sample only what is not derived —
  its id and fix, its motion, TWS, TWD, Hs, wave direction, current speed
  and direction, the three dataset indices and the fetched flag — by column
  in `tracks/<id>.json`. The environment is rounded on ingest to 0.01 kn,
  0.1° and 0.01 m (ERA5 resolves nothing finer), so memory equals what a save
  and load give back. TWA, tack, the corrected values and the wave angle, and
  the sample's time and place (its fix's), are recomputed on load. Measured
  on Fastnet 2025 boats: 7.5–8.8 bytes per sample of environment, deflated.
- **How it is downloaded** (D27): every ERA5 store is chunked one global
  field per hour (≈ 3.3 MB), a blosc container of eight blocks that decode
  independently (131,072 values, ≈ 91 rows, each). A fetch reads each needed
  chunk's first 64 bytes (header and block offsets) by HTTP Range, then only
  the blocks holding its positions' stencil rows (adjacent blocks in one
  request), and decodes those. A stencil across two blocks, the poles, the
  0/360 seam and an uncompressed (memcpyed) container are handled; a server
  that answers a Range with 200 and the whole chunk has it used whole (up to
  the 64 MB body cap) and is then read whole. Values are read from the bytes
  just fetched, never back through the size-bounded memory; a chunk whose
  header was served but whose blocks then 404 fails the batch (resumable),
  never "fetched, no data". A fetch first reads the heads of every ERA5
  chunk the track needs, side by side, so each batch waits for one round
  trip per chunk. The values relate() derives are rounded to canonical
  precision, so libm differences between platforms cannot reach an export. The current geoChunks are read
  the same way (a block is 1,024–8,192 hours of a box). About an eighth of
  the whole-chunk bytes: a 5-day race hourly ≈ 150–160 MB, not ≈ 1.3 GB.
- TWA is the angle between the heading and where the wind comes from; the
  wind over the starboard side is starboard tack, and head to wind or dead
  downwind is neither. The wave angle is measured off the bow (0° head
  seas, 180° following), the bow pointing along the heading through the
  water where there is a current and the ground heading otherwise.
- The environment is stored as found; everything that relates it to the
  boat's motion (TWA, tack, the corrected values, the wave angle) is
  recomputed from the stored values whenever the motion changes, so a
  change of derivation settings (§7.4) needs no fetch and undoes exactly.
- **Current correction** (D13). Wind and current are both ground-relative;
  a polar is water-relative. When a current is available:
  - boat velocity through the water = ground velocity − current;
    BSP = its magnitude and heading = its direction (leeway ignored);
  - wind over the water = wind − current; TWS and TWA use it.
  Both raw (ground) and corrected values are stored. A project-level toggle,
  **Correct for current** above the track list, chooses which feed the
  polar (default: corrected where current exists); it is one undo. The
  **Include Stokes drift** option is gone (asked 2026-10-02): the document
  keeps the setting for the fetch's sake, a loaded project has it off
  (migration 7 → 8), and no control sets it.

#### 7.5.1 Current source

Every tier below includes the tide (settled from the stores' own metadata
on 2026-09-28, `plan.md` §6, Q7); they differ in resolution and in how the
tide is modelled. The app takes each sample from the first source in this
chain that covers its time and place (D20):

1. **Regional tidal reanalyses**, 1993 to a few months ago. They are tidally
   forced models, so `uo`/`vo` is already the total current:
   - NW European Shelf, `NWSHELF_MULTIYEAR_PHY_004_009`,
     `cmems_mod_nws_phy-uv_my_7km-2D_PT1H-i` (7 km, hourly, 40–65N,
     20W–13E; int16 with `scale_factor`).
   - Iberia–Biscay–Ireland, `IBI_MULTIYEAR_PHY_005_002`,
     `cmems_mod_ibi_phy-cur_my_0.027deg_PT1H-m` (1/36°, hourly means,
     26–56N, 19W–5E).
2. **Global merged surface current**, 2020-11-01 onward:
   `GLOBAL_ANALYSISFORECAST_PHY_001_024`,
   `cmems_mod_glo_phy_anfc_merged-uv_PT1H-i` (1/12°, hourly). It carries
   `uo` (circulation), `utide` (FES2014 tide), `vsdx` (Stokes drift) and
   `utotal = uo + utide + vsdx`. The app uses `uo + utide`; the option to
   include Stokes drift was removed 2026-10-02.
3. **GlobCurrent** (`MULTIOBS_GLO_PHY_MYNRT_015_003`, the store VectorEffects
   reads), 1993 onward, 0.25°, **with tide (FES2022)**: in version 202411
   both the multi-year and near-real-time stores describe `uo` as "absolute
   geostrophic velocity + depth Ekman + tide velocity". The multi-year
   series (`cmems_obs-mob_glo_phy-cur_my_0.25deg_PT1H-i`) is read first and
   the near-real-time one (`…_nrt_…`) after it ends; `uo`/`vo` are read at
   the level nearest the surface (0 m of 0 and −15 m).

Each dataset record says whether its current includes the tide; every
tier above does, so no sample is marked "no tide" today. The "leave out
currents without tide" filter stays for a tier without one should it ever
be added.

A position is looked for in a tier only if it lies inside that tier's grid
and time axis (the regional tiers only inside their boxes, so a race
elsewhere never opens those stores). A tier whose value is missing there —
land, fill, or a chunk the archive does not have — passes the position to
the next tier. A tier whose store will not open (the archive down, a
version withdrawn) is left out for ten minutes, its positions going on to
the next tier; the fetch goes on and reports "left out a current source
that would not open" on the status line with the reason. The stores read (versions as recorded on each sample):
`cmems_mod_nws_phy-uv_my_7km-2D_PT1H-i_202112`,
`cmems_mod_ibi_phy-cur_my_0.027deg_PT1H-m_202511`,
`cmems_mod_glo_phy_anfc_merged-uv_PT1H-i_202211`,
`cmems_obs-mob_glo_phy-cur_{my,nrt}_0.25deg_PT1H-i_202411`. Integer-packed
arrays (the NW Shelf and GlobCurrent `int16`) are unpacked with their own
`scale_factor`/`add_offset`, and their fill value is missing.

All are Copernicus Marine ARCO zarr v2 on `s3.waw3-1.cloudferro.com`, read
anonymously with the same blosc/LZ4 codec as ERA5. For sampling along a
track the **geoChunked** stores are used (one chunk covers months at one
place). Their fill value is about 9.97e36, and a missing chunk comes back as
HTTP 403 or 404; both mean "no data".


### 7.6 Sample filters

**Live analysis extension.** Valid number edits apply after a 120 ms typing
pause, with blur flushing immediately. Writes are serialized and newer drafts
survive older responses. Track, global and priority filter changes invalidate
derived points, segments and blend in both views. Main wave sliders apply an
additional saved constraint to track samples and the blend, without replacing
track/global/priority filters.

Direction, apparent wind angle (AWA), true wind speed and true wind direction
can each have a maximum change threshold. Compare each observation only with
the immediately previous and next points, as confirmed by the user. Never skip
an unknown neighbour to compare farther away, or bridge a gap exceeding the track's
maximum gap. Thresholds are inclusive: only a strictly greater change excludes
points. Missing observations cannot establish a change. Angles use the shortest
circular separation; AWA is the bearing of air velocity relative to the moving
boat, retaining port/starboard sign. All comparisons use original observations,
so another filter cannot hide a change. Wind speed/direction and heading follow
the current-correction selection; apparent air velocity is frame invariant and
AWA uses the selected bow heading.

**M19 extension (requested 2026-09-30).** All filters remain reversible
overlays. Optional switches exclude samples with unknown wave data (height
or direction missing) and unknown current (speed or direction missing).
Zero height/speed is known, not missing. Wave direction can also be compared
with COG, independently of current correction (0° ahead, 180° astern).

**Consolidated filters (asked 2026-10-02).** The filters are grouped by
what they read, and each of the boat's and the wind's quantities has a
range:

- **Boat speed**: BSP from–to, and **VMG** from–to (BSP × cos TWA, so
  positive to windward and negative downwind, in the speed unit).
- **Boat heading**: **heading** from–to and **COG** from–to, each a compass
  sector clockwise from its start to its end, wrapping north, both bounds or
  neither (clearing one clears the other). The heading is through the water
  where the current is corrected for, the course over the ground always; the
  direction-change filter stays with them.
- **Remove tacks and gybes**: on or off (settled with the user 2026-10-02:
  no window). A tack or gybe is where the tack — the side the wind is on —
  differs between a sample and the last one that had a tack; the sample on
  either side of it is left out and no more. Head to wind or dead downwind
  in between may bridge the change; missing wind, or a gap longer than the
  track's maximum derivation gap, cannot.
- **Wind, waves and current**: TWS from–to, **TWD** from–to (a compass
  sector, through the water where corrected), TWA from–to, the change
  filters, and the wave and current filters as before.

The tack/gybe window, the stop speed and window, the timestamp interval and
its unit, and the given-or-derived origin filters are gone (a project saved
with any of them loads without them, migration 7 → 8). A sample without the
value an active range reads is out, as everywhere in this section.

**Provided or derived (asked 2026-10-02).** Where a track *provides* a
quantity — headings (COG), speeds (SOG) or wind (both speed and direction)
— its filters offer **Use: provided by the track / derived** for that
quantity, in its group: the heading and the speed are chosen apart (§7.4),
and the wind's choice is the supplied-or-downloaded one (§7.5). A quantity
the track does not provide is derived and offers no choice. The separate
"Heading and speed" section is gone, and with it the maximum-gap control
(the setting stays at its default, 3 h, and the MCP `track_set` can still
change it). Track start/end inputs accept seconds.

The 3D view exposes **Global point filters** only when the project has track
data. These offer the same settings without start/end times, apply after
each track's own filters, and affect the derived segments, blend and every
view consistently. They start disabled; enabling an empty global layer
adds no exclusions. All changes are saved and undoable (schema 3); schema 2
projects migrate with every new option disabled.

The user-requested ORR catalogue source is [RegattaMan's public ORR valid
list](https://www.regattaman.com/valid_list_ora.php?crule=ORR&sdir=true&ssdir=true&sort=3&ssort=0).
The app includes a captured catalogue and Settings offers a year-specific
refresh with progress and cancellation, started by hand or by the schedule
chosen there (§5.4). Only `pe-trackers` contacts
`www.regattaman.com`; source lookup and project imports work offline. The two
boat-speed tables (offshore and short-course) remain separate variants; time
allowances are never read as boat speeds. Physical measurements come from
metric original inputs, not converted display spans. Each SKU/variant is stored
once, repeat scrapes update that record, and repeat project imports are no-ops.
Failures name their certificates; cancellation leaves the prior catalogue intact.
Complete public certificate data is cached: every named valid-list column
(including effective/expiry dates, IR and BM-PHRF spin/non-spin ratings), all
Data and performance-metric fields, public ownership, comments, and line-drawing
parameters. Field identifiers, labels, original metric values and enum display
labels are retained, including blanks, zeros and negative values. Original
offshore/short-course speed and time-allowance tables keep their wind columns,
row labels, units and footnotes. All embedded rating blocks are decoded directly
from `data-ratingjson`, including custom/general TCF, GPH, wind-band TCF and PCS
ratings, their course/wind/system keys, and both spin and non-spin values.
Rating precision is preserved as published; missing values stay missing.
Missing/malformed ratings or mismatched certificate identity fail explicitly.
Raw HTML, scripts, login/session state and generic glossary tables are not saved.
Imported records are immutable copies with ordinary overlays. Schema 4 adds an
optional certificate payload; older imports retain their original polar-only
data. An old polar-only catalogue cache cannot mask a complete bundled record.
Builder and build year searches use the newly captured certificate fields.

The top bar's **Asymmetric polar** checkbox selects full-circle
asymmetric mode in every stage; it is not in Blend settings. Starboard occupies
0–180°, port 180–360°. Half-circle sources seed both sides; full-circle imports
retain their independent values. Symmetric mode averages opposite source cells
without modifying the import. Exports preserve full-circle axes and read back
without folding. Switching the mode immediately changes the output grid in one
undoable step. Unchecked selects symmetric mode. Blend settings edits the grid
using the selected mode.
Grid presets offer 1/2/5/10° TWA and 1/2/5/10 kn TWS spacing; changing
the TWS step rounds the existing upper limit up to the next step.

Interpolation is selectable: linear (the old default) or shape-preserving
monotone cubic Hermite interpolation (PCHIP), first along TWA then TWS.
The slope rule follows the [SciPy PCHIP reference](https://docs.scipy.org/doc/scipy/reference/generated/scipy.interpolate.PchipInterpolator.html).
Both modes preserve source coverage and exclusions and never extrapolate.
The chosen rule applies to source resampling, blend filling, plots and export.
The 0° and 360° rows stay zero and never anchor interpolation. Manual blend
corrections are saved as cell overlays after blending; resetting them restores
the current derived result. No computed blend is persisted.

Optional priority groups are evaluated after individual/global filters and
manual sample exclusions. For each TWA/TWS cell, pool eligible observations
from visible tracks with positive weight; select the first group with at least
the configurable minimum. Only that group's matching samples contribute,
once each even if groups overlap. If no group qualifies the cell has no track
evidence. A group's filters are independent of other groups, and may loosen
criteria in later groups. With priorities enabled, the pooled group minimum
replaces each individual track's minimum; each contributing track still uses
its own statistic and the unchanged sample-confidence blend weight. Ordered
groups, their minimum and edits are undoable and saved.

Per track source, editable any time, undoable:

- **Wave height** range (m) and **wave direction**: relative to the bow
  (head, bow, beam, quarter, following sectors, or a custom range) or
  absolute (from-direction range).
- Current speed range.
- TWS range, TWA range.
- Time window (defaults to the race start and finish when the tracker gives
  them, so pre-start and post-finish motoring is out).
- Minimum BSP (default 1 kn) and maximum BSP.
- Manoeuvres: exclude fixes where heading changes more than N° (default 30°)
  between neighbours.
- Given versus derived heading/speed.

Filtered-out samples stay in the project and appear dimmed in the plots when
"Show filtered" is on.

- A sample lacking the value an active filter reads (no wind under a TWS
  range, no speed under the minimum BSP) is filtered out: nothing shows it
  passes. With no filter on a quantity, a missing value does not matter.
- The manoeuvre filter compares a sample's heading with each neighbour's and
  takes the larger change; a neighbour without a heading, or further away
  in time than the track's maximum gap, is ignored.
- Wave sectors, off the bow (0° head seas): head below 30°, bow 30–60°, beam
  60–120°, quarter 120–150°, following from 150°.
- BSP, TWS and TWA are the water-relative (current-corrected) values where
  they exist and the project uses them (§7.5), the ground values otherwise.
- Every filter is edited in a track's details in the Tracks section: time
  window, BSP range, manoeuvres and given versus derived (M8), and TWS,
  TWA, wave height and current speed ranges, the wave direction (sectors,
  an angle off the bow, or a compass range from–to clockwise) and "leave
  out currents without tide" (M9). Each change is one undo.
- The BSP, TWS and current boxes are in the display speed unit and the
  wave height boxes in the display wave height unit (§3.4), converted where
  they are shown and typed; the filter is stored in knots and metres, and a
  box left as it was commits nothing (M17b).

### 7.7 Jobs

Reanalysis fetches run on a worker pool, show progress in the status bar
and track list, and can be cancelled. Tracker downloads show progress and
Cancel in their import dialog; the boat list can be searched while downloading.
ORC and ORR catalogue refreshes show progress and cancellation in Settings,
continue while Settings is closed, and are independent of the open project.
A tracker download never includes weather
(D24). A cancelled or failed fetch keeps whatever samples completed
(status "partial") and can be resumed with Fetch weather…. Jobs for tracks from the
same event share the session's in-memory blocks, so the second boat of a
race downloads little.

The environment fetch in detail (M9):

- One runner takes the queued tracks one after another, so a second boat
  reads the chunks the first one just cached instead of downloading them at
  the same moment. Within a track, samples go in batches of at most three
  hours of track time and 400
  samples; each batch's chunk reads run on a pool of the network
  concurrency setting (§3.4), each chunk fetched and decoded once.
- Each finished batch is written into the project at once. Cancel stops at
  the next chunk read, keeping every finished batch. Each sample's values
  depend only on its own time and place, so a resumed fetch ends exactly
  where an uninterrupted one would.
- Fetch weather… fetches the samples still missing; on a ready track, or at a
  different interval or Stokes-drift choice than the last fetch (both
  recorded on the track), it starts over. Starting over first clears every
  sample's wind, waves and current (raw, corrected and which datasets) and
  the track's dataset records, so a restart that is cancelled leaves a
  track partly fetched at the new settings, never a mix of two fetches.
- Within a batch, the wind's and waves' chunks share one pool, and the
  currents (Copernicus Marine) are read beside them (Google Cloud), each
  host with at most the concurrency setting in flight.
- The status bar shows the running track, its progress and how many wait,
  with Cancel fetch (all); a track's row has Cancel fetch for it. A
  failure is reported on the status line and leaves "partial" (or "failed"
  when nothing finished).
- A job belongs to the project it was started for: New, Open, Close and
  opening recovered work first ask to cancel a running fetch, and a result
  for a track that was removed or changed meanwhile is dropped.
- Reads: a transient failure (a 5xx or 429 answer, a timeout, a dropped
  connection) is retried three times, pausing 0.5, 1 and 2 s; anything else
  fails at once. A retry pause ends at once on Cancel. A body over 64 MB is
  refused. A redirect is followed only to the same host or, over HTTPS, to
  another archive host (invariant 4). A block that arrives but fails to
  decode fails the read and is not kept; a failed read is not retried as
  one. Opened archives
  are kept for the session, so only the first track pays their metadata
  requests.

### 7.8 Reanalysis GRIB export — removed in M19

The track GRIB export option, its dialog and its help/search entries have
been removed at the user's request. Weather fetching for track samples is
unchanged. The GRIB encoder and backend fixture tests are retained internally.

---

## 8. Source list

The right panel lists every source (ORC, file, track) in one list:

- Colour swatch: click to change (a palette of 16 plus a custom picker). New
  sources take the next unused palette colour.
- Visibility toggle: **hidden sources are excluded from the blend and from
  every plot** (D15). This is the one "include/exclude" switch.
- Weight slider (0–1, default 1). A weight above 1 saved by an earlier
  version loads as 1 (schema 7).
- Label (rename in place), kind icon, and a count (cells for polars,
  samples/used samples for tracks).
- Actions: Edit (opens the 3D stage focused on this source, §10.4; a ✎
  after it means the source holds edits), Compare (opens the Compare stage
  with this source as A and the blend as B, §11), Remove.
- Drag to reorder (display order only).
- A **Blend** entry at the top represents the current blend: its colour
  (the same picker as a source's; a new project's is `#e0457b`, seen on
  every bundled theme; a colour too close to the theme's background — a
  white blend saved before M14, on Paper — is drawn with a contrasting
  outline in the swatch, the 2D plot and the 3D grid lines), show or hide (the plots only; export
  always writes the blend), its coverage ("N direct, M filled"; the tooltip
  adds the empty cells, §12.3), a **Blend settings** button (§12.2) and
  **Export…** (§12.4). Each change is one undo entry ("Show blend", "Hide
  blend", "Change blend colour", "Change blend settings").

---

## 9. 2D views

### 9.1 Map

Available only when tracks have been imported. A WebGL2 world map with the
VectorEffects basemap (Natural Earth land and coastlines, embedded; no tiles).
Projections:
equirectangular and orthographic, chosen on the map and remembered in the
settings. Drag pans the flat map or turns the globe; the wheel zooms, about the
pointer on the flat map and about the centre on the globe; "Fit the world"
shows everything again. The globe's land is drawn through an equirectangular
mask texture, so no land triangle folds across the horizon.

- Every visible track is drawn in its source colour, two device pixels
  wide (a WebGL line is one: half a CSS pixel on a high-density screen).
  Filtered-out fixes are
  drawn dimmed (excluded ones less so). A track crossing the antimeridian is
  one continuous line: longitudes are unwrapped along each track.
- Hovering a fix shows time (UTC), SOG (the track's own speed, over the
  ground; asked 2026-10-04) and heading (each marked given or
  derived), TWS, TWA, Hs and current, in the display units; a value not yet
  known shows as a dash.
- Selecting samples in a polar view highlights them on the map, and a box
  selection on the map (Shift-drag) selects those samples in the polar views.
  "Show on map" in the 3D view and in the track list switches to the map and
  frames what was asked for. "Fit the tracks" frames every visible track.
- Tracks travel from Rust as one packed binary buffer (layout in
  `pe-app/src/map_tracks.rs` and `ui/src/map/trackPacket.ts`, pinned by a
  shared fixture), drawn in one WebGL2 draw call.
- Wind barbs at the hovered time are a stretch goal (§14).

### 9.2 Polar plot

A classic 2D polar diagram, the **2D** stage (asked 2026-10-02; until then
a section of the right panel with a full-size overlay). On the stage a
symmetric polar's 0°–180° axis runs down the middle of the view — the
part of the stage between the open panels — and the fan fills its right half, as a full circle's axis does; its controls are
centred across the top; the panel form,
kept in the code for a narrow place, puts the fan against its left edge.
The MCP service's `view://stage` still names the stage `"plot"`.

- A TWS slider (or "all") chooses the slice (D21). One value draws one curve
  per visible polar source (tracks are not polar sources) and the blend, all
  at that TWS; "all" draws one curve per visible source per wind speed that
  source's own grid has, rather than a shared slice — the classic diagram of
  several TWS curves at once. A curve is read by the project's
  interpolation rule (§7.6; `pe-polar`, no extrapolation) across the
  source's own TWA axis, through
  the source's overlay as the blend reads it: its edits written in and its
  excluded nodes empty (§10.3, §10.4, §12.3); dots are
  for every sample whose TWS is within ±1 kn (configurable in Settings,
  §3.4) of the slice, in their track's colour, selected ones ringed. A
  sample without wind has no place in the polar and is not drawn. A
  "Filtered" toggle adds the filtered-out samples, dimmed; an "Excluded"
  toggle adds the excluded ones, hollow, which are otherwise hidden. Rust
  sends excluded dots either way, so the toggle asks it for nothing.
- Dots travel from Rust as one packed binary buffer (layout in
  `pe-app/src/polar_plot.rs` and `ui/src/panels/dotPacket.ts`, pinned by a
  shared fixture), not JSON: "all" draws every sample with wind, and 50
  tracks of 10,000 fixes as JSON objects are tens of megabytes. Beyond
  20,000 dots each is drawn as a small square rather than a circle.
- **Colour** chooses what the dots' colour shows: their track (the
  default) or the **time of day** they were sailed at, with a legend of the
  four bands (§10.2). It is offered while a track is shown, and is view
  state, not saved.
- Curves use source colours. The blend is drawn thicker, in the Blend
  entry's colour, read from the blend grid (§12.3) the same way: at the
  slice, or in "all" one curve per output-grid wind speed that has a value
  off the 0° row. Those wind speeds join the slider's range. Hovering it
  shows "Blend". A hidden blend is not drawn.
- Hover shows the source, TWA, TWS and BSP; a dot's speed is labelled SOG,
  or STW where it was corrected for current (§7.5), since a track sample's
  speed is not a polar's boat speed (asked 2026-10-04). Hovering a point of
  the blend that is an output-grid cell (its angle and wind speed both on
  the grid) shows the cell as the 3D view does (§10.1): its value, where the value
  came from, and each source behind it with its speed and its share of the
  weight. A blend point between cells keeps the plain text.
- **Measure** turns the pointer into a ruler. While it is on, a dashed
  spoke at the pointer's wind angle and a dashed ring at its boat speed are
  drawn, each curve is marked where the spoke crosses it, and a readout in
  the plot's corner lists every curve's boat speed at that angle, fastest
  first, each with its difference in the display unit and in percent
  against the curve nearest the pointer. A curve is read between its own
  points as a straight line in speed over angle, and has no value outside
  the angles it covers. A click pins point **A** (on a curve's own value
  when the click is on its mark); the readout then adds the pointer's
  speed less A's, their ratio in percent and the angle between them, and a
  line joins them. Another click moves A; Escape lets it go (before the
  full-size overlay hears it); turning Measure off forgets it. The
  ordinary hover is off while measuring. On a symmetric plot the port side
  has no fan and measures nothing. Measuring reads the curves the plot was
  given and changes nothing.
- Ring values (BSP) sit just under the 90° spoke and angle labels just
  outside the outermost ring; no two labels overlap, and a ring value that
  would touch another label is left out.
- Asymmetric polar angle labels read 0–180° on each side of the diagram.
  Full-circle angles still position independent port/starboard curves and dots;
  only tick text is mirrored (270° is labelled 90°, 330° is labelled 30°).
- The rings, the slice's wind speed, its dot band and the hover are in the
  display speed unit (§3.4): rings at round values of that unit, placed at
  their speed in knots. Everything arrives in knots and is converted only
  where it is drawn as text (M17b). So does the 3D drag readout (§10.4).
- The rings reach the fastest curve, blend point or dot drawn: excluding
  an outlier brings them back to the data left, and ticking "Excluded"
  takes them out to it again.
- Full size opens the same plot as a large overlay on the current stage,
  including projects without tracks. Its own button, Escape, or switching
  stage closes it (M19b supersedes the Map-only overlay in D21).

---

## 10. 3D polar view

### 10.1 Scene

- Built with **three.js**, bundled by Vite from npm (no CDN; invariant 4)
  (D16).
- Axes: TWA (angle), TWS, BSP. Two layouts, toggled:
  - **Polar tower** (default): x = BSP · sin(TWA), y = BSP · cos(TWA),
    z = TWS. Each TWS is a classic polar curve; stacked they make a surface.
  - **Cartesian**: x = TWA, y = TWS, z = BSP.
- Every visible polar source is a translucent surface in its colour, with the
  grid lines drawn. Every sample is a dot in its track's colour. The blend is
  an opaque surface on the output grid, in the Blend entry's colour, sent
  while the entry is shown (source index `0xFFFFFFFF` in the scene).
- The camera looks at the origin (asked 2026-10-03): on first load, and
  from each preset, the origin is drawn in the middle of what the panels,
  the toolbar and the wave filters leave in sight, from far enough back
  that the corner of the scene farthest from it is in the frame. (The
  Compare stage's presets still look at the middle of what they show.)
- Orbit, pan, zoom; preset cameras (top, side, isometric); an axis
  legend with the display units. Top looks down the vertical axis (in the
  tower, the classic polar diagram with every TWS stacked); side looks
  across it, so each TWS is a level; the axes carry tick labels in the
  display speed unit.
- The axes span the points drawn: an excluded or filtered-out point only
  while its toggle shows it, no sample a wave range hides, and none of the
  blend's zero-speed row at 0° (which runs to the output grid's top wind
  speed). When that box grows or shrinks, the camera follows, scaled about
  the origin: it keeps the angle it looks from, and the data keeps its room
  on screen.
- Asymmetric angle ticks read 0–180° on each half, as in the 2D plot; the
  Cartesian axis reads 0–180–0°. The geometry, stored grid and editable cell
  identities retain their full-circle angles.
- Every surface is the source's own grid over its own axes, as edited
  (its cell overrides written in, §10.4): nothing is
  resampled or extrapolated, and an empty cell is a hole.
- Hovering the blend's surface, where no dot is under the pointer, shows
  the output-grid cell nearest the point hit (the surface is drawn finer
  than the grid in spline mode): TWA, TWS and the blend's BSP there; whether
  the value is direct evidence, filled in between neighbouring cells, the
  0° row's 0 kn, empty, or a manual correction; and each source the rule
  counted in the cell, in id order, with its own boat speed and its share
  of the cell's total weight (§12.3). A smoothed blend says so, since its
  value is then not exactly the sources' mean. The translucent sources in
  front are picked through, as they are seen through. Rust answers one cell
  per request (`blend_cell`), asked once per cell and revision; it is
  derived and never stored (invariant 2).
- Must hold 60 fps with 200,000 dots and 20 surfaces on the reference
  machines (§13), using instanced points.
- Rust assembles the scene and sends it as one packed little-endian binary
  buffer (`f32` coordinates, `u32` ids and flags), not JSON; the layout is
  documented once on each side (`pe-app/src/polar3d.rs`,
  `ui/src/polar/scenePacket.ts`) and pinned by a shared fixture. When no sample has moved
  since the scene the view holds (an edit, an exclusion, a filter), the view
  names that scene's samples key and only the samples' flags travel
  (0.8 MB rather than 11 MB at 200,000 samples); anything that moves samples
  (the environment, the derivation, corrected or ground values, a source
  shown, hidden, added, removed or moved) sends the whole scene.

### 10.2 Showing all known points

"All dots" means every known (TWA, TWS, BSP) triple: every sample from every
visible track, and the grid nodes of every visible polar source. Toggles: show
samples, show polar nodes, show surfaces, show filtered samples (dimmed),
colour dots by source / by Hs / by current speed / by time / by time of day.
Polar nodes have no environment, so they keep their source colour in every
colour mode.

**Time of day** is the band of the local solar day a sample was sailed in,
by local mean solar time: UTC shifted by the sample's longitude at 15° an
hour, with no time-zone table and no ephemeris, so the same on every
machine. Four bands (settled with the user 2026-10-01, D30): **night**
21:00 up to 05:00, **morning** 05:00 up to 12:00, **afternoon** 12:00 up to
17:00, **evening** 17:00 up to 21:00. Rust decides the band
(`pe_tracks::daytime`) and sends its two-bit code in bits 8–9 of each
sample's flags, in the 3D scene (layout version 5) and the 2D dots (version
2); the interface names and colours it (four colours of the Okabe–Ito set)
and shows the bands and their hours as a legend. A dot's tooltip names its
band. It is a display grouping and never enters the blend. A mode, or "show filtered", with nothing to show (no sample has
that value, no sample is filtered) is offered disabled with a tooltip saying
why.

**Wave range filters** sits at the bottom centre of the 3D view, between the
open side panels. Each metric uses one horizontal slider with round start/end
handles on a shared rail and a value at each end: significant wave height
(the Settings height unit), wave angle off the selected bow (0–180°), and wave
period (seconds). Both handles remain reachable when they coincide and support
native keyboard adjustment. Clicking and dragging the selected rail between the
handles moves both bounds together, preserving the range width and stopping at
either end of the scale. Hovering or dragging the rail keeps its normal thin
appearance; the transparent drag hit area never gains a button background.
The selected rail is keyboard-focusable: arrow keys
move it one step, and Home/End move it to the start/end of the scale. Pressing
the rail without moving does not change either bound. All three inclusive ranges apply together, in addition
to track/global/priority filters. Moving a slider previews the dots immediately
and updates the native blend, 2D/3D surfaces, sample counts and exports. The
constraints are saved and undoable, surviving changes of stage and reopen.
Missing measurements cannot satisfy active bounds; inactive bounds keep them.
A metric with no data has disabled sliders. Bounds cannot cross. Show the
resulting sample count and offer **Reset ranges**, which clears only these
additional wave bounds. Imported polar sources remain intact. Dots outside the
ranges are not actionable selections (§10.3).
The right-side controls and polar editor reserve space above the wave sliders.

### 10.3 Excluding dots

- Click selects a dot; Shift-click adds (or removes a dot already
  selected); a lasso or box (in screen space) selects many, Shift adding.
  The Rotate, Lasso and Box tools choose what a drag does; a click selects
  in each, and Escape clears the selection. The selection survives a
  refetch of the scene (dots are matched by source and grid cell, or by
  sample id).
- Only dots that are drawn are counted in the selection info and acted on
  by Exclude, Include and "show on map": a selected dot that a toggle hides
  (samples off, a filtered sample while "show filtered" is off, an
  excluded one while "Excluded points" is off) takes no part.
- **Exclude** removes the selection from the blend. Excluded points are
  then hidden (asked 2026-10-04) until "Excluded points" under Show is
  ticked, which draws excluded sample dots hollow and excluded ORC or file
  nodes as crosses, so they can be selected and included. **Include**
  restores them. Both are undoable.
- For polar sources, excluding a node stores an exclusion in the overlay; the
  node's cell is then empty for that source in the blend. The surface still
  shows the source (as edited, §10.4), with the excluded node drawn as a
  cross.
  One Exclude or Include over nodes of several sources is one undo entry;
  nodes already in the asked state are left alone.
- Selection info: count, mean TWS/TWA/BSP, source breakdown, "show on map".

### 10.4 Editing one source

Every source can be edited on its own (D17):

- **Edit** on a source focuses it: its surface is fully opaque, others fade
  (kept visible for context, "Hide other sources" hides them). An edit
  panel opens beside the 3D view; **Done** leaves edit mode and the edits
  stay. Removing the source leaves edit mode.
- The editable surface is the source's polar: the imported grid for files,
  the VPP grid for ORC, and the **polar segment** for tracks (§12.1), on
  the project output grid. In edit mode a track's segment is drawn as its
  surface with its cells as nodes; they are edited, never excluded (a
  track's samples are).
- Edit tools:
  - drag a node (the **Drag** tool, offered in edit mode) along the BSP
    axis as it appears on screen (straight up when that axis is seen end
    on); Shift snaps to 0.05 kn. The value is kept to 0–60 kn, and one drag
    is one undo entry;
  - a **table editor** (TWA rows × TWS columns, values and wind speeds in
    the display speed unit, stored in knots; a value typed at the table's
    two decimals shows back unchanged) beside the 3D view,
    with the same cells; typing a value (Enter or leaving the cell) is an
    edit, one undo entry each; emptying an edited cell resets it; a value
    typed into an empty cell fills it. Clicking a cell selects it (Shift
    adds); the 3D selection and the table's are one. Tooltips give the
    source's value, and for a track the cell's sample count and spread;
  - scale a selection by a percentage (of the value as edited; empty cells
    are left out);
  - smooth a selection (3×3 binomial kernel, 1-2-1 × 1-2-1, over the
    neighbours that have a value as the blend reads them — a node excluded
    from the blend takes no part — every cell read from the grid as it was;
    a hole stays a hole);
  - reset a selection to the source value;
  - for a track, choose its segment statistic (§12.1).
  Scale, smooth and reset are one undo entry each.
- Edits are stored as `cell_overrides` on the overlay (invariant 1), keyed
  by the editable surface's axis values, sorted and one per cell; an
  override whose cell is no longer on the grid (the output grid changed) is
  kept but has nothing to apply to. They are shown with a marker in both
  the 3D view (an edited node is a square) and the table (a highlighted
  cell). "Reset all edits" clears them, and with none left the source reads
  exactly as imported.
- **Every edit updates the blend and every open view** (3D, compare, 2D polar
  plot) within the §13 budget. Edits drive a recompute in Rust; the UI never
  computes a blend itself. Rust keeps what it derives from each source (its
  grid as edited, a track's placed and filtered samples and its segment)
  in memory, keyed by per-source revisions that only the changes reaching
  that source move, so an edit recomputes only the edited source (and,
  from M14, the blend). Nothing derived is saved (invariant 2).

---

### 10.5 Split Wave Angle

Requested by the user 2026-10-02. In **single view only** (a pane of split
or four-way view does not offer it), **Split Wave Angle**, beside the Box
tool, draws the 3D view once per wave direction.

- **The direction is relative to the boat**: degrees clockwise from the
  bow, 0° on the bow, 90° on the starboard beam, 180° astern, 270° on the
  port beam. It is the sample's wave "from" direction less the heading the
  wave angle off the bow already uses (through the water when the project
  corrects for current). Unlike that angle it keeps its side, so Rust sends
  it as a column of its own, `wave bearing`, in the scene (layout version
  5); NaN when the waves or the heading are unknown.
- **The count** is a slider with six stops: 4, 8, 16, 18, 24 or 36
  directions (8 to begin with). With *N* directions each is 360/*N* wide
  and **centred** on a multiple of 360/*N*, the first on the bow, so 0° is
  always the middle of a direction and never a boundary between two. A
  bearing on a boundary belongs to the direction clockwise of it.
- **From / To** chooses the sense: *From* puts a sample in the direction
  its waves came from, *To* in the direction they went to (the bearing plus
  180°). The copies are named for it ("From 45°", "To 225°").
- **Each copy holds only its direction's samples**, of those the view would
  draw anyway: the show toggles, the track, global and priority filters and
  the wave range filters (§10.2) all still apply, and the copies split what
  is left. Grid points, surfaces and guides are in every copy. A sample with
  no wave direction is in no copy; the controls say how many those are.
- **Each copy has its own blend** (asked 2026-10-02): the blend drawn in a
  copy is made with every track binned again from that copy's samples
  alone — after the same filters, exclusions and wave ranges — and every
  other source as it is, since a certificate or a polar file carries no
  wave direction. It is the ordinary blend rule over that evidence (§12.3),
  so a copy with no sample of its direction shows the reference polars'
  blend alone, and a copy where no source speaks has no blend. Rust answers
  all the copies' blends in one packet (`polar_scene_split`, "PE3W": one
  surface per copy that has a blend), cached on the blend's own key plus
  the split, and a hovered cell of a copy's blend (`blend_cell_split`)
  names the sources behind it *there*: the track with that direction's
  samples and their confidence. The whole blend is not drawn in a split
  view; it is unchanged, and so is export (invariant 2).
- **Each copy has a big arrow** against a small boat drawn bow up: it points
  at the boat from the copy's direction (*From*) or away from the boat
  toward it (*To*). Round the boat, on a faint ring of the whole circle, an
  arc shows the directions the copy holds — its 360/*N* degrees centred on
  its direction. The copy also says its direction and how many samples it
  holds.
- **The copies are one view**: one canvas and one camera drawn into a grid
  of viewports (a WebGL context per copy would pass the webview's limit at
  36), so rotating, panning, zooming and the camera presets move them all
  identically. The grid is the one whose copies are largest on their shorter
  side, and it lies in the part of the stage the toolbar, the side panel,
  the wave range filters and the docked panels leave free, following them
  as they open and close.
- **Hover is linked**: the dot under the pointer has its full tooltip, and
  in every other copy the sample nearest the same wind (within 5° of TWA
  and 1 kn of TWS) is ringed with its boat speed. Hovering a copy's blend
  marks the same cell in every other copy that has a blend, each with its
  own blend's speed there, so the eye reads how the waves change it. A
  click, a lasso or a box acts in the copy it starts in, on that copy's
  dots.
- Axis labels are drawn in the first copy, thinned so none is written over
  another, and not at all when the copies are narrower than 150 px.
- It is a way of looking: it is not saved with the project, never enters
  the blend and changes no export.

## 11. Compare

- The Compare stage takes two operands, A and B. Each is any source's polar,
  any track's polar segment, or the current blend. A hidden source can be
  compared too: hiding takes it out of the blend and the plots, not out of
  reach. Compare on a source row (§8) opens the stage with that source as A
  and the blend as B.
- Both are read onto the project output grid as the other views read them,
  and never extrapolated: a polar source with its edits written in, read
  by the project's interpolation rule (§7.6, §12.2) with every cell read
  from an excluded node empty, as the blend reads it (§12.3) — so a source
  compared with a blend of itself alone differs nowhere, in either rule; a track through its segment with its
  overrides; the blend as the views draw it. Rust computes everything — A,
  B, Δ, the classes, the statistics and the regions — and sends it as one
  packed binary buffer (layout in `pe-app/src/compare.rs` and
  `ui/src/compare/comparePacket.ts`, pinned by a shared fixture); the
  interface never computes a difference. An edit anywhere refetches it.
- 3D: A and B as translucent surfaces in their colours, plus a **difference
  surface** midway between them, coloured by ΔBSP = A − B with a diverging
  scale (orange: A faster; blue: B faster; a hue-less grey at zero),
  centred on zero and symmetric (its ends are ± the largest |Δ|), with its
  range shown in the legend. A toggle shows Δ as percent of B, (A − B) / B;
  a cell where B is under 0.1 kn is compared in knots but **not comparable
  in %**: drawn plain grey (no hatch), left out of the percentage
  statistics, and counted in the summary in % mode. The scale is linear in OKLab
  with both poles at the same lightness, colour-blind safe (poles ≥ 20 ΔE
  apart under simulated protan, deutan and tritan vision), and has one set
  of stops for dark themes (arms lighten away from zero) and one for light
  themes (arms darken); neither pole is the feature search's flash orange.
  Each surface can be shown or hidden; the layouts and preset cameras are
  the 3D view's (§10.1).
- **Overlap**: only cells where both A and B have a value are coloured.
  Cells covered by only one are drawn in neutral grey with a pattern at
  that operand's speed, and counted in the summary ("N only A, M only B"):
  diagonal stripes in the heat map; in 3D, a cross marker on every such
  node and both diagonals across every quad whose four corners are all
  one-only (a quad mixing compared and one-only corners is not hatched). The 0°
  row is 0 kn by definition (§12.3), not a measurement: it takes no part in
  the overlap, the statistics, the regions or the heat map.
- Summary: overlap cell count, mean and max |Δ| (with where the max is),
  the TWS/TWA regions where A is faster and where B is faster, and a 2D Δ
  heat map (TWA rows × TWS columns; hovering a cell shows TWA, TWS, A, B and
  Δ). The heat map is one canvas image, one pixel per cell scaled up, with
  the hover found by arithmetic, so a 512 × 512 grid costs about 25 ms to
  draw and no element per cell. A comparison Rust refuses clears the stage
  and says why. A **region** is, for one TWS, a run of neighbouring TWA cells where
  Δ > +threshold (A faster) or Δ < −threshold (B faster); a cell not
  compared or within the threshold ends a run. The threshold is a Compare
  setting, 0.05 kn by default, in the display speed unit; Δ is rounded to
  1e-6 kn first, so a difference of exactly the threshold is not over it on
  any platform.
- A swap button, and each operand's picker lists the blend and every source
  by colour and name (a track standing for its polar segment; a hidden
  source marked hidden).
- What is compared, the percent toggle and the threshold are view state:
  not undoable, not saved, kept in memory keyed by the project's id while
  the application runs (the id is stored in the project, so closing and
  reopening it finds the same choice). The stage is remounted for each
  project id, so nothing of one project's comparison shows under
  another's. An operand whose source is removed falls back (A to the first
  source, B to the blend).

---

## 12. Polar segments and the blend

### 12.1 Polar segment from a track

- Samples that pass the filters and are not excluded are binned onto the
  project output grid (§12.2), folding port and starboard. A sample goes to
  the node nearest it on each axis: a node's bin runs halfway to each
  neighbour, and past the first and last node by the same half-step as
  beside them; a sample beyond that is left out, and one exactly halfway
  goes to the upper node. Nothing is binned into a 0° TWA node: the 0° row
  is 0 kn by definition (§12.3), so samples nearest 0° are dropped, not
  moved to the next node.
- Per cell: statistic of BSP, selectable per track (in the edit panel,
  undoable) — median, mean, 75th or **90th percentile (default)**.
  Percentiles interpolate linearly between the two nearest ranks. A polar describes good sailing, not average
  sailing, so an upper percentile is the default (D18).
- A cell needs at least **5 samples** (setting) to have a value. Cells below
  that are empty.
- The segment keeps, per cell, its sample count and spread (the sample
  standard deviation of the boat speeds), including cells below the
  minimum; these drive the blend weight and appear in tooltips.

### 12.2 Output grid

Project setting, editable in Blend settings:

- TWS default: 4, 6, 8, 10, 12, 14, 16, 20, 25, 30 kn.
- TWA default: 0, 30, 35, 40, 45, 52, 60, 70, 75, 80, 90, 100, 110, 120,
  135, 150, 160, 170, 180.
- Polar sources are resampled onto this grid by the project's
  interpolation rule (§7.6): bilinear in TWA × TWS by default, or
  shape-preserving cubic (PCHIP) in the same two steps; never extrapolated
  beyond the source's axes. The two steps run along TWA
  within each bracketing TWS column first, using that column's own known
  angles, then along TWS; so a ragged Expedition polar reads each wind speed
  between its own points, and a query below a column's first or above its
  last angle has no value.
- Each axis is typed as a list (spaces, commas or semicolons between values,
  "." as the decimal point) and must hold 1–512 values, TWA in 0–180 and TWS
  in 0–70 kn, strictly increasing, with at most two decimals — so no two
  values are closer than 0.01 and every value is written exactly as it is
  (export writes axes with two decimals, §6). The editor says which values
  are wrong before anything is sent; Rust checks again. "Default grid" puts
  back the defaults above.
- Blend settings also hold: the statistic a newly imported track starts
  with (90th percentile by default; each track keeps its own, §12.1), the
  samples a track cell needs (5), the samples for full confidence (`n_full`,
  30), smoothing (off) and current correction (§7.5). **Apply** makes
  everything one undo entry ("Change blend
  settings"); a new grid re-bins every track, and an edit on a node the new
  grid lacks is kept in the overlay with nothing to apply to.

### 12.3 Blend

For each cell (TWA, TWS) of the output grid, over the **visible** sources
that have a value in that cell:

    blend = Σ wᵢ · bspᵢ / Σ wᵢ

- `wᵢ = source weight × confidence`, the source weight in 0–1 (§8).
  Confidence is 1 for polar sources and
  `min(1, n / n_full)` for track segments, with n the cell's sample count and
  `n_full` default 30. **An overridden cell counts with confidence 1**: a
  track cell the person edited (§10.4) is vouched for, whatever its sample
  count, 0 included (D23).
- **Polar statistic** (Blend settings, default *mean*): how the polar
  sources' terms of a cell are combined before they meet the tracks. *Mean*
  is the rule above unchanged. *Min*, *median*, *max* and *p90* sort the
  cell's polar terms by boat speed (ties by source id), and hand their
  **combined weight** to the term at that rank — the median and p90 ranks
  interpolate linearly between the two neighbouring terms, as a weighted
  pair — so the polars as a group keep the same weight against the tracks
  whichever statistic is chosen. Track terms are untouched. The contributions
  read back (`blend_cell`, the tooltips) show these effective weights, so a
  polar outside the chosen rank is absent. Stored in the document with a
  default, so older projects load as *mean* and blend as before.
- Cell overrides apply before blending; exclusions remove the cell.
- A polar source is read onto the output grid by the project's
  interpolation rule (§12.2), its edits
  written in; **every output cell read from an excluded node is empty** for
  that source — reading across the node from its neighbours would put back
  part of what was taken out. A track is read through its segment (§12.1),
  already on the output grid, its excluded nodes empty.
- Cells with no source stay empty, then are filled in order by: interpolation
  along TWA within the same TWS; then along TWS; the 0° row is 0 kn. Each
  step fills only **between** two known values (nothing is extrapolated), the
  TWS step also from the TWA step's values, and the 0° row takes no part in
  either (as in binning, D22). A cell nothing reaches stays empty and is
  written empty.
- Optional smoothing (off by default) over the filled grid: the 3×3
  binomial kernel over neighbours with a value (as the edit tool's, D22);
  the 0° row stays 0 kn and takes no part; empty cells stay empty.
- The blend shows its own coverage: cells with direct evidence (at least one
  source with a positive weight had a value) versus filled (interpolated),
  and the empty rest. The 0° row is 0 kn by definition, not evidence, and
  counts in none of them; a blend with no value off the 0° row is empty.
- Sources are summed in id order, never list order (display only, §8), and
  every value is rounded to the canonical knot precision (1e-6 kn), so the
  same project gives the same bits on every platform (invariant 5). A cell
  whose weights sum to zero has no direct value.
- The terms of a cell's weighted mean — each source with a value there and
  a positive weight, with that weight — can be read back
  (`pe_polar::blend::contributions`), from the same code that sums them, for
  the tooltips of §9.2 and §10.1.
- The blending rule is isolated in `pe-polar::blend` behind one function so it
  can be changed later without touching views (asked before changing, see
  `CLAUDE.md`).

### 12.4 Export

- **Export…** on the Blend entry opens a dialog: the format (Expedition
  `.txt`, Adrena `.pol` or CSV `.csv`, §6), the grid (the project's output
  grid, or custom axes typed as in §12.2 — the blend read onto them
  by the project's interpolation rule, never extrapolated, and with the 0° row no anchor, as in the
  fill: an output 0° row is 0 kn, and an angle between 0° and the blend's
  first other angle is empty; tracks are binned on the project grid, so
  that is where the blend is made), and a preview of the grid as it would be
  written (filled cells muted, empty ones blank). **Save…** opens the native
  save dialog with the format's extension; the file is written atomically.
- Export always recomputes the blend from the sources and overlays, never
  from what the views have cached (invariant 2).
- Export refuses, naming the problem, a grid two of whose axis values would
  be written the same with two decimals (the reader would refuse the file:
  "TWA values 42.001 and 42.004 would both be written as 42"), an axis
  value a polar file cannot hold, a boat speed over 60 kn, or a blend with
  no value off the 0° row (no visible source, say). Every file export writes
  reads back as the grid written (a property test), and a fixed project's
  three files are pinned by SHA-256 in `pe-app/tests/export.rs`. The four CI
  targets that run tests (macOS Intel and Apple silicon, Windows x64,
  Linux) run it; Windows ARM64 only builds the tests, as the hosted runner
  cannot run them.

---

## 13. Performance budgets

On the reference machines (M1 MacBook Air; a 2020 Intel i5 Windows laptop;
a Snapdragon X Windows ARM64 laptop):

| Operation | Budget |
|---|---|
| Start screen visible after launch | < 1.5 s |
| ORC search result update per keystroke | < 30 ms |
| Local track search, prepared index | < 50 ms backend plus a 50 ms typing delay; one-time indexing runs in the background |
| Blend recompute after an edit (20 sources, 200k samples) | < 50 ms |
| Edit to every open view updated | < 100 ms |
| 3D view with 200k dots, 20 surfaces | 60 fps |
| Open a project with 50 tracks | < 2 s |
| Reanalysis for a 5-day race, cold cache | reported, not budgeted; progress every second |
| Second boat from the same event | < 5 s when its blocks are in memory; otherwise only the hours and boxes the first boat did not need |
| Tracker address to a pickable boat list | as short as the tracker allows: the first response naming the boats, no weather (D24) |

The native live-analysis filter pass was measured on the Intel development Mac
with 200,000 observations and all four previous/next change thresholds: 45.4 ms
for changing data and 48.8 ms for steady sailing (optimized development
libraries). The comparisons are O(n). These timings cover filtering, not IPC or
rendering.

Reanalysis is read by block (§7.5, D27): one hour of one variable is a
64-byte head and usually one block of its global field — about 0.43 MB of
wind, 0.17–0.31 MB of wave height or direction at mid-latitudes — instead
of the 1.7–3.3 MB field. Measured (M14e, debug build, home broadband): a
Fastnet 2025 boat of 120 h, 1,585 samples, downloaded 159.6 MB in 99 s
where whole chunks would have been 1,279.5 MB, and adds 13 KB to the
project; the second boat of the race downloaded 9.6 MB. Fetch weather starts
immediately without a size-estimate modal (requested 2026-10-07).
Wind, waves and currents are always sampled hourly
(requested 2026-10-07); there is no coarser interval or size-based fallback.
The optional MCP estimate counts, per ERA5 hour the samples need, five heads and the blocks
holding their stencil rows (one hour's measured block sizes), leaves out
blocks already in memory, and adds one typical current block per variable
per box and block of hours crossed in the first tier whose box holds the
position. Whirlwind instead counts the exact compressed inner chunks absent
from its memory and disk caches. Existing projects retain the recorded
interval of older fetched weather; a new fetch uses hourly sampling and
starts over if the old interval differs. GRIB weather reads also use hourly
sampling.


---

## 14. Deferred (post-v1)

- Other trackers found in `tracker-index` (Kwindoo, RaceQs, TracTrac,
  GeoRacing, Estela, TackTracker, Metasail, iSail, YachtBot, Kattack).
- Instrument log import (NMEA, Expedition logs) as a track source with
  measured BSP and wind.
- Polars per sail and crossover charts.
- Custom theme editor.
- Wind barbs and wave fields on the map.
- Linux ARM64 build.
- Additional languages (VectorEffects ships nine).

## 15. Resolved questions

See the decisions log in `plan.md` §5.


3D dots show source, TWA, TWS and speed on hover: BSP for a polar node, SOG
for a track sample, or STW where its speed was corrected for current (a flag
in the scene, bit 3; asked 2026-10-04). Track dots additionally show UTC
time, the time of day (§10.2), wave height, angle and period, current speed, and
excluded/filtered status.
Hover follows visible dot picking and clears during navigation or selection.
