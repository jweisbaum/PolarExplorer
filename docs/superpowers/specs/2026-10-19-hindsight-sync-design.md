# Hindsight Sync as the reanalysis source — design

Date: 2026-10-19. Asked for by the user in this session; the answers recorded
here are theirs unless marked *my call*.

## Intent

PolarExplorer stops choosing between built-in reanalysis archives. Instead it
asks **Hindsight Sync**, the user's own desktop app (repository
`/Users/jon/HistorySyncer`), which reanalysis Zarr archives exist on this
computer and in the cloud, and the user picks one of those in Settings. That
archive is the one every weather fetch reads. Without Hindsight Sync there is no
weather: Settings says so, with the app's logo and a link to get it.

Why: the archives are managed in Hindsight Sync already (buckets, credentials,
local copies, chunking). Keeping a second, hard-coded list of them in
PolarExplorer, with credentials baked into the binary, is what this removes.

## Goals

1. Settings detects a running Hindsight Sync on loopback, by a saved port
   (default 8765), a **Scan** button, and a manual port field.
2. When it is not detected and never has been: a warning with the Hindsight
   Sync logo, the sentence *PolarExplorer needs Hindsight Sync to download the
   reanalysis.*, and a link to `https://hindsight-sync.whirlwindai.com`.
3. When detected: a dropdown of the reanalysis archives it lists, local and
   cloud, never climatologies. Each row shows the archive's name, where it is
   (the cloud host, or the folder), its chunking (degrees × hours) and the
   parameters it stores.
4. The chosen archive is the dataset every weather fetch reads. PolarExplorer
   remembers its location, credentials included, so a fetch works while
   Hindsight Sync is closed. Cloud archives are read with the credentials
   Hindsight Sync hands out, never anonymously: an archive it lists without
   credentials cannot be chosen.
5. Once a connection has succeeded, the warning never shows again; a later
   absence is a quiet "not running" state with the same Scan and port controls.
6. The old **Data sources** picker (Open Data, Whirlwind S3 / R2 / Tigris) and
   everything only it used are removed: the Open Data readers and their hosts,
   the fixed Whirlwind endpoints, the built-in R2 and Tigris credentials.

## Non-goals

- No change to what a fetch stores in the project, to the sampling, the
  interpolation, the batches, the caches or the budgets of spec §7.5 and §13.
- No MCP tool for choosing archives (the IPC commands are reachable through
  the existing `invoke` table, as the old `set_data_source` was).
- No support for climatology archives, and no reading of Hindsight Sync's
  archives by any route other than its API: PolarExplorer never lists buckets
  or scans folders itself.
- Hindsight Sync is not changed. Its API is `api_version: 1` as documented in
  its README ("Read-only archive API").

## Anti-goals

- Credentials reaching the webview, a log, a project file, a URL or `Debug`
  output. Today's redaction rules keep applying.
- A probe of anything but loopback. Detection must never touch the network.
- A fetch that silently reads a different archive than the one shown in
  Settings. The archive captured when a fetch is queued is the one it reads,
  and a track never mixes archives (today's rule for sources, kept).
- Guessing the archive layout. The reader validates the Zarr metadata it is
  handed exactly as it does today, only wider (below).

## Constraints that bind the builder

- CLAUDE.md invariant 4 changes wording, not spirit. The two network crates
  stay `pe-env` and `pe-trackers`. `pe-env` gains two kinds of allowed
  destination: `http://127.0.0.1:<port>` (Hindsight Sync's API, loopback only,
  the port from Settings) and *the archive location Hindsight Sync handed it*:
  an S3-compatible endpoint named in the API response, or a folder on disk.
  `tools/check-offline.sh` keeps its source-level check: the only literal
  hosts left in `pe-env` are `127.0.0.1` and the AWS endpoint template
  (`https://{bucket}.s3.{region}.amazonaws.com`, with `{…}` placeholders as the
  Geovoile pattern already allows). The fixed WeatherBench2, ARCO-ERA5,
  CloudFerro, Whirlwind S3, R2 and Tigris hosts leave the allow-list.
  The product link is opened by the operating system's browser, never fetched;
  it is a literal URL in `pe-app`, so the offline check gets one named
  exception for that constant, with a comment saying it is opened, not read.
- `pe-app` keeps no HTTP client. The Hindsight Sync client lives in `pe-env`
  (a `hindsight` module) and is called from `pe-app`.
- Settings stay global, validated and saved by Rust at once (spec §3.4).
  `settings.json` is the only place the remembered location is written.
- The `.wpsproj` layout does not change. Provenance keeps using
  `DatasetRecord { name, version, fetched_at, has_tide }` with new values
  (below). No schema bump.
- Every string through `t()`/`msg()`, in all nine languages; every control
  with a `data-feature` and a registry entry; a UX test in
  `tools/webdriver/ux/` drives the real Settings section against a fake
  Hindsight Sync; spec.md, plan.md, the user guide and the help topics are
  updated in the same commits.

## Hindsight Sync's API, as this design relies on it

Read `/Users/jon/HistorySyncer/README.md`, section "Read-only archive API",
and `src/api/catalog.rs` for the exact JSON. What PolarExplorer uses:

- `GET http://127.0.0.1:<port>/v1/archives/cloud/reanalysis` and
  `/v1/archives/local/reanalysis`. Both answer
  `{ api_version: 1, archives: [...], errors: [...] }`. The client sends no
  `Origin` header (the server refuses one) and a `Host` of `127.0.0.1`.
- Per archive: `id` (opaque), `name`, `storage` (`cloud` | `local`), `uri`,
  `mode`, `start_datetime`, `stop_datetime`, `chunk_degrees`, `chunk_hours`,
  `products` (what it stores, e.g. `wind10m`), `parameters` (the full stored
  `param` axis), `layout_version` (2 or 3), `root_metadata`, `array_metadata`.
- `location`: for cloud, `protocol: "s3"`, `provider`, `bucket`, `prefix`,
  `endpoint_url` (null for Amazon S3: the endpoint is then
  `https://{bucket}.s3.{region}.amazonaws.com`), `region`,
  `force_path_style`, and `credentials` `{ access_key_id, secret_access_key,
  session_token }` or null with a `credential_error`. For local,
  `protocol: "file"` and an absolute `path`.
- Unknown fields are to be ignored; a missing `api_version` or one other than
  1 is "not Hindsight Sync" for detection purposes.

Detection means: a GET of `/v1/archives/cloud/reanalysis` answers HTTP 200
with `api_version: 1` within 2 seconds. A refused connection, a timeout, or
any other answer is "not detected".

## The archive layouts the reader must accept

Today's reader (`pe-env/src/whirlwind/layout.rs`) insists on float16 and on
exactly seven parameters. Hindsight Sync writes two layouts:

| `historysyncer_layout_version` | data type | `param` axis |
|---|---|---|
| 2 (older archives) | float16 | the seven: `u10 v10 ucur vcur wave_direction wave_height wave_period` |
| 3 (current) | float32 | those seven plus `mslp z500 wind_speed wind_constancy` |

The reader is widened to accept both: data type float16 or float32 (decoded
accordingly), any parameter count, locating the seven it needs by name as
`parameters()` already does. Everything else it validates today stays
validated (rectilinear shards, `sharding_indexed`, index at the end with
crc32c, the band sums). The inner-chunk size bound becomes a count of
values, the same four million it allows today at two bytes each, so a
float32 chunk may decode to 16 MiB; the disk cache's per-entry cap follows
the same rule (*my call*: layout 3 at 20° × 60 h or more would otherwise
fail at fetch time though Hindsight Sync lists it as valid). The user-set
memory budget is unchanged. An archive whose
`products` leave some of the seven unstored holds NaN there, which the
sampler already treats as missing. The builder confirms from Hindsight Sync's
`src/layout.rs` and its root attributes that layout 3's wind, current and
wave units are the same SI units as layout 2 (m/s, m, degrees, s); if a unit
differs, that is a conversion on ingest in `pe-env`, where m/s is converted
today, not a project change.

## What the user sees

### Settings → Reanalysis (replaces the "Data sources" control inside
"Downloaded weather")

A new section, `settings:reanalysis`, placed where **Data sources** was. Three
states:

**Never connected, not detected.** The warning: the Hindsight Sync logo
(bundled, from `/Users/jon/HistorySyncer/ui/assets/hindsight-sync.png`, the
256 px copy, placed in `ui/src/assets/`), the sentence *PolarExplorer needs
Hindsight Sync to download the reanalysis.*, and a link reading
`hindsight-sync.whirlwindai.com` that opens `https://hindsight-sync.whirlwindai.com`
in the system browser. Below it, **Scan** and the **Port** field.

**Connected before, not detected now.** One muted line: *Hindsight Sync is not
running. The remembered archive is still used for fetches.* (or *…No archive
is chosen.* when none is), then **Scan** and **Port**. No logo, no link.

**Detected.** A dropdown **Reanalysis archive** listing every reanalysis
archive from both endpoints, cloud first then local, each in its own group
(`<optgroup>` *Cloud* / *This computer*). A row reads

    <name> — <where> — <chunking> — <parameters>

where *where* is the endpoint host for cloud archives (`endpoint_url`'s host,
or `<bucket>.s3.<region>.amazonaws.com` when it is null) and the folder path
for local ones; *chunking* is `<chunk_degrees>° × <chunk_hours> h`; and
*parameters* is the archive's `products` joined by commas (the products are
what it stores; the full `param` axis is the layout, not a choice). The
first option is *Choose an archive…* while none is chosen. An archive that
Hindsight Sync lists in `errors`, or whose cloud `credentials` are null, is
shown disabled with its `error` / `credential_error` as the title (a cloud archive is never read
anonymously, so without credentials it is not a choice). A
remembered archive no longer in the list is shown as a last option marked
*(no longer listed)* so the user can see what fetches still read.

Under the dropdown: one muted line with the chosen archive's time range
(`start_datetime` to `stop_datetime`, UTC dates) and its `mode`, then
**Scan** and **Port** as before, with a muted *Connected to Hindsight Sync on
port N*.

**Scan** probes, in order: the saved port; 8765 if different; the port in
Hindsight Sync's own settings file if that file is readable
(`<platform config dir>/com.historysyncer.desktop/settings.json`, key
`archive_api.port`, absent meaning 8765). The first that answers is saved
as the port. The button shows *Scanning…* and is disabled while a scan runs.
*My call:* reading the sibling app's settings file is a read of a file on
this computer, not a reach into the network, and makes Scan find a changed
port without the user copying it.

**Port** is an integer field, 1–65535, saved on change like every other
setting; a change triggers a probe of that port only.

Probes also run without a button: once when the app starts and whenever the
Settings dialog opens. *My call:* the user asked for detection; a loopback
GET to their own app is not a fetch of anything they did not ask for.

A probe that succeeds sets `connected_once = true` (never cleared), replaces
the archive list the UI shows, and, if the chosen archive is in the list,
refreshes the remembered location and credentials from it (credentials rotate
in Hindsight Sync; the remembered copy follows).

### Fetching weather

`Fetch weather…` with no archive chosen is refused with an error naming the
setting: *Choose a reanalysis archive in Settings before fetching weather.*
Otherwise the fetch reads the remembered archive, Hindsight Sync running or
not. An authentication failure fails the fetch as today, with the message
adding *Press Scan in Settings to refresh the archive's credentials.* A
missing object or chunk is missing data, as today.

The archive chosen is captured when the fetch is queued, as the source was.
Queued tracks that share an archive share batches as today; a change of
archive between two fetches of one track replaces that track's weather (a
track never mixes archives).

### Where "Open Data" used to show

The "Downloaded weather" section keeps its memory field; its explanatory
line loses the Open Data sentence. The Whirlwind cache and Network sections
stay as they are; "Whirlwind cache" is renamed **Reanalysis cache** in the UI
and the help, and the Network section's "Whirlwind downloads up to 64 chunks
at once" says "Reanalysis downloads…". The on-disk cache child
`whirlwind-hindsight-v1` keeps its name (existing caches stay valid).

## Settings model

In `pe-app/src/settings.rs`, `data_source: DataSource` is removed (an old
`data_source` key in `settings.json` is ignored, like any unknown key) and
replaced by:

```
reanalysis: ReanalysisSettings {
    port: u16,                  // default 8765, 1–65535
    connected_once: bool,       // default false
    archive: Option<RememberedArchive>,
}

RememberedArchive {
    id: String, name: String, storage: Storage (Cloud | Local),
    uri: String, layout_version: u32,
    chunk_degrees: u32, chunk_hours: u32,
    products: Vec<String>,
    r#where: String,            // the display host or folder, as shown in the row
    mode: String, start: String, stop: String,   // RFC 3339 UTC, for the muted line
    location: ArchiveLocation,
}

ArchiveLocation = S3 { bucket, prefix, endpoint_url: Option<String>, region,
                       force_path_style, credentials: Option<Credentials> }
                | File { path }
```

`Credentials` is `pe_env::whirlwind::Credentials` (redacted `Debug` already),
given `Serialize`/`Deserialize` for `settings.json` only. The IPC `Settings`
type (ts-rs) carries `reanalysis` **without** `location`: a view struct
`ReanalysisView { port, connected_once, archive: Option<ArchiveView> }` where
`ArchiveView` is `RememberedArchive` minus `location`. A test serialises a
`Settings` with a remembered cloud archive to the IPC JSON and asserts the
text contains neither key id nor secret. `settings.json` is written with
`write_atomic` as today, mode 0600 on Unix (*my call*: it now holds a
secret, the way Hindsight Sync's own settings file does).

Settings commands (all in `lib.rs`'s handler, `mcp/invoke.rs`'s TABLE, and
`tools/effects.rs`; `set_data_source` leaves all three):

- `reanalysis_scan() -> ScanResult` — the Scan button.
- `reanalysis_probe() -> ScanResult` — the saved port only (startup, dialog
  open, port change).
- `set_reanalysis_port(port) -> Settings`.
- `set_reanalysis_archive(id: Option<String>) -> Settings` — chooses from the
  last probe's list (held in memory in `AppServices`); `None` clears.

`ScanResult { detected: bool, port: u16, archives: Vec<ArchiveView>,
unavailable: Vec<{ id, name, storage, reason }> }`. The frontend keeps the
last result in the dialog's state; `ipc.ts` adds the four calls.

## Reading the archive

`pe-env/src/whirlwind/` becomes the reanalysis reader for any Hindsight
Sync archive:

- `Source` (the three fixed endpoints) and `URL` go. `Whirlwind::new` takes
  an `ArchiveLocation` (moved to `pe-env`) and credentials from it. The S3
  transport takes the endpoint, region and path style from the location
  and signs every request with its credentials; the anonymous S3 path goes
  (no cloud archive is read without the credentials Hindsight Sync
  vendored).
- A **file transport** reads a local archive's shard index and inner chunks
  from the folder with the same byte ranges (`File::seek` + `read_exact`),
  through the same `Archive`/`plan`/decode path. No disk cache for file
  archives; the memory cache applies as for S3. A folder that is gone or
  unreadable fails the open with the path in the message.
- `Dataset`'s three Whirlwind variants and all Open Data variants go, along
  with `dataset.rs`'s Open Data readers, `http.rs` and `store.rs`'s
  HTTP/Zarr v2 paths, the Open Data tiers in `sampler.rs`, and their
  fixtures and tests. `Reanalysis` is constructed from an `ArchiveLocation`
  only. What stays of `Dataset` is whatever the sampler still needs for
  provenance; the builder may replace the enum by the provenance values
  below.
- Provenance (`DatasetRecord`): `name` = the archive's identity: for a
  cloud archive its `uri` exactly as Hindsight Sync gives it (`s3://`,
  `r2://` or `tigris://…`); for a local copy its `source_uri` when present,
  so a copy and the cloud archive it copies count as one source and no home
  folder path lands in a shared project, and its `file:` URI only when there
  is no source (*my call*); `version` =
  `hindsight-layout-<layout_version>`; `has_tide` = `Some(true)` as today
  (the archive's current is the GlobCurrent tidal current). "Different
  source" for a track = any recorded dataset whose `name` differs from the
  archive's identity. Cache namespaces (memory and disk) use that identity
  where they used `Dataset::id`; chunk cache keys still include the shard
  ETag, so a local copy and its cloud source never alias a stale shard.
- GRIB export (`grib.rs`) takes its reader from the same provider: it reads
  the chosen archive and is refused with the same message when none is
  chosen; its download estimate uses the reader's exact chunk accounting
  (today's Whirlwind `estimate`) in place of the Open Data estimate
  (*my call*).
- Stokes drift: unchanged. The project field and `set_stokes_drift` stay;
  the reader already ignores it (`stokes_drift: false`).

### Hindsight Sync client (`pe-env/src/hindsight.rs`)

`probe(port, timeout) -> Result<Option<Listing>>`: the two GETs, parsed into
`Listing { archives: Vec<ListedArchive>, errors: Vec<ListedError> }`, with
`ListedArchive` carrying the fields the settings model needs plus the raw
`location`. `None` when not detected. It uses the crate's existing reqwest
client (`net`), connects only to `127.0.0.1`, follows no redirects, and
sends no `Origin`. Tests run it against a loopback `axum`/`hyper` stub
serving recorded responses copied from Hindsight Sync's `src/api/tests.rs`
shapes (cloud S3 with null endpoint, R2 with endpoint and path style, an
archive with `credentials: null`, a local one, an `errors` entry, a
climatology that must never appear since the endpoints exclude it).

## Removal list

- `DataSource`, `set_data_source`, `data_source_set`, the `whirlwind_source`
  mapping, the UI select `settings:data-source` and its registry entry and
  translations, the four option strings.
- `pe-env`: `whirlwind/source.rs`, `URL`, the Open Data datasets and
  readers, the fixed host list in `check-offline.sh`.
- spec.md: §3.4 Data sources bullet (rewritten as **Reanalysis**), §7.5's
  "With Whirlwind (Fast) S3 selected…" and "With Open Data (Slow)
  selected…" blocks and the table, §7.5.1 Current source (the fallback chain
  is Open Data's), every "Open Data" and "Whirlwind (Fast)" mention; CLAUDE.md
  invariant 4's crate/host sentence and the `pe-env` line of the layout table;
  `docs/USER-GUIDE.md`; help topic `settings` and the features file.
- The built-in R2 and Tigris credentials, and the spec sentences about them.

## What's left to the builder

- Module and file names inside the above; the exact TS component split in
  `ui/src/settings/` (a `Reanalysis.tsx` beside `WeatherCache.tsx` is the
  natural shape).
- How the logo is imported (a Vite asset import keeps it under the `'self'`
  CSP).
- The platform opener for the link: `pe-app` already shells out to `open`
  / `cmd /C start` in `mcp/desktop.rs`; the same pattern with `xdg-open` on
  Linux, in a command that opens only this one constant URL (the webview
  never passes a URL).
- The fake Hindsight Sync for the UX test (a Node `http` server in the test
  on a free port; the test types that port into the field, presses Scan,
  picks an archive, and screenshots all three states).
- Migration of a user's existing `settings.json`: `data_source` is dropped;
  nothing else moves.
- The decisions log entry in plan.md (next D number) and the milestone
  (next M number) recording this change and the removal of Open Data.
