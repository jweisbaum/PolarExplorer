# PolarExplorer — Plan

**Status:** Draft v0.1 · **Date:** 2026-09-27

How PolarExplorer gets built, in order. `spec.md` says what it does;
`CLAUDE.md` says how to work in the code. Update a milestone's status in the
same commit that finishes it.

**2026-10-08: v0.3.7 release preflight — complete.**
The workspace, npm manifests/lockfile, Cargo lockfile and Tauri configuration
all name 0.3.7. The production frontend build, release-version guard, ten
driver/tooling tests, four macOS signing-configuration tests and four release
asset tests pass. The feature validation below covers this release's source.
The tagged Release builds workflow produces and checks all five platforms;
local preflight does not claim a fresh cross-platform install/signing review.

**2026-10-08: Deferred tracker downloads and conditional vessel search — complete.**
Open requests only YellowBrick/Geovoile boat metadata. Import tracks downloads
positions and imports the choices, retaining choices for retry and preventing
late answers after cancellation from importing. Blue Water Tracks waits for the
first Import tracks to download its combined response, then shows its picker
(as confirmed by the user). Cached events still open without a request.
The vessel-search heading, input and results are hidden when the configured
metadata file is absent, with presence refreshed after settings changes,
metadata jobs and window focus. Help and all eight translations follow the flow.
Validation: 871 Rust tests pass, as do clippy, formatting, typecheck and the
offline check. All 526 UI tests pass across the full suite and focused reruns
(the translation cleanup and new metadata-directory regression were rechecked).
All five performance checks pass; the tracker fixture was updated to count the
selected preview line separately after Import tracks. For 444 boats, listing
took 338 ms, the map update 104 ms, ticking 6 ms and searching 43 ms.
Desktop tracker import, position-failure/retry and track-library checks pass
with local fixtures. Request counts prove no position request before Import,
cache reuse and metadata-only refresh. Screenshots of the deferred picker,
download progress, retained retry selection, hidden search and visible search
with configured metadata were inspected. The test applications were closed.

**2026-10-08: Named storage providers and anonymous S3 — complete.**
Settings now names S3, R2 and Tigris in every language, and Help uses the same
provider names. S3 metadata and chunk-range reads use unsigned requests and
ignore AWS environment variables and legacy credential files. R2 and Tigris
keep their existing signing credentials. Offline desktop error tests now use
an invalid cache path so public S3 cannot turn them into live downloads.
The 25 Whirlwind tests pass, including unsigned metadata and both range forms.
A live anonymous sample at 48N, 5W, 2000-01-03 12:30Z returned wind, waves and
current in 2.16 s (11 requests, 1,363,360 bytes); a repeat made no requests.
Validation: all 867 Rust tests, 520 UI tests, five UI performance checks,
clippy, formatting, typecheck, translations/Help and the offline check pass.
The UI selection timing test exceeded its budget during compilation and passed
on a focused rerun. Local HTTP fixtures required running outside the sandbox.
Desktop source-selection and track-library checks pass. The weather check also
passes after adjusting its expected status for the earlier cache-validation
failure (no weather was fetched, and retry stays available). Provider labels,
persistence and the weather settings screenshots were inspected.

**2026-10-07: Bulk Whirlwind weather — complete.**
Selected Whirlwind tracks now share one chunk-planning and download pipeline:
up to 128 compatible queued tracks, 10,000 samples per batch, and 72 hours of
lookahead per track. Time-ordered batches include different races and dates;
overlapping tracks fetch and decode each required inner chunk once per batch.
The existing 64-request cap applies to the whole pipeline. Each shard's chunks
start when its index arrives, without waiting for unrelated indexes. Decoding
has a separate CPU limit (up to eight workers), and decoded float16 bytes share
the existing bounded RAM cache with compressed chunks and indexes. Hourly
stencils are planned once for all seven parameters.

Disk-cache I/O and checksums run concurrently; reservations include temporary
files in the size cap, and clearing/changing the limit waits for active I/O.
Per-track progress, cancellation, partial results, source provenance and boat
tab identity remain independent. Shared reads that fail for one chunk retry
per-track subsets; archive/credential/cache failures stop the cohort without
repeating the failure once per boat. UI refreshes are coalesced per batch.
Open Data keeps its existing runner.

Fixture verification: 50 routes needed exactly eight shard indexes and twelve
inner chunks, with no duplicate ranges. Re-reading all fifty routes made no
network requests or additional decodes. On this run the cold batch took 176 ms
and the fifty warm reads took 12 ms. Job regressions cover overlapping routes,
sample limits, disjoint dates, cancellation/resume, failures and equal source
IDs in different tabs. All 866 Rust tests, 520 UI tests, five UI performance
checks, clippy, formatting, typecheck and the offline check pass. The selection
timing test and performance suite needed reruns after concurrent builds stopped;
their thresholds were unchanged.

The desktop test imported Astarte / Harvest Moon 2021, Azure / Pacific Cup 2022
and Towhee / Newport Bermuda 2022, selected all three, and fetched hourly R2
weather together. All three were active simultaneously and reached ready in
17.439 s, with 388/395, 509/559 and 1249/1249 samples containing wind. A forced
refetch of every sample took 217 ms from the warm cache and preserved those
counts. Cold and warm screenshots show plotted samples and were inspected.
An earlier desktop launch waited out its build timeout, and the first live
attempt failed an R2 shard request after retries; a direct range read and the
fresh desktop retry succeeded. The job-error regression also verifies that
such an archive failure empties the queue without per-boat retry loops.

**2026-10-07: Cal 40 search matches and empty weather status — complete.**
Investigated the reported R2 fetch with no dots. The blank boat tab contained
land-event tracks returned by "cal 40": "cal" matched names such as Pascal,
while "40" matched a timestamp or ID. Their completed fetches had no usable
wind. The same R2 reader successfully fetched three actual Cal 40 sailing
tracks through library import and the Fetch weather button: Astarte / Harvest
Moon 2021 (388/395 samples with wind, 11.9 s), Azure / Pacific Cup 2022
(509/559, 13.3 s), and Towhee / Newport Bermuda 2022 (1249/1249, 6.2 s).
All three displayed dots; screenshots were inspected.

Complete-query matches within one vessel value now precede incidental
matches across fields, preserving all searchable fields and catalogue order
within each tier. The existing on-disk index remains compatible. Completed
weather with no plottable samples says "no points to plot" in all languages;
weather status has its own wrapping line and refetch remains available.
Validation: 855 Rust tests and 520 UI tests pass, plus final library/track
regressions, formatting, clippy, typecheck and offline checks. All five UI
timing checks pass after rerunning two that exceeded their budgets during
compilation. Prepared real-library searches take 0.53–7.74 ms (previously
0.46–9.90 ms); the actual "cal 40" query now returns Astarte and Azure first.
Desktop checks verified ranking and the empty-weather status using an isolated
copy of the affected project; the visible sidebar screenshots were inspected.

**2026-10-07: Start weather directly — complete.**
Removed the download-size modal and its UI estimate request. Single-track
and selected-track actions immediately queue hourly weather. Ready tracks
still refetch all samples; partial tracks resume. Pending starts reject
duplicate clicks, failed starts preserve selection for retry, and existing
job progress and cancellation remain. Updated Help and all translations.
Validation: all 854 Rust and 519 UI tests, typecheck, clippy, formatting,
five performance checks and the offline check pass. The weather, track-library
and data-source desktop checks pass; a live R2 route fetched all 12 positions
from one click with no estimate modal and displayed Weather: ready.
Screenshots of the completed route, selected-track fetch and retry state
were inspected.

**2026-10-07: R2, Tigris and combined Whirlwind chunks — complete.**
Added Source 2 (Cloudflare R2) and Source 3 (Tigris), using the supplied
embedded S3 credentials as explicitly requested. Both use the confirmed
`whirlwind-hindsight/hindsight` bucket/prefix. The R2 management token is
unnecessary and is not embedded. Signed GETs use the selected endpoint and
region; credentials stay outside IPC, settings, project files and logs.
All Whirlwind sources now use the R2 example's single `data` array: 72 hours ×
7 named parameters × 40 × 40 cells per Zstandard float16 inner chunk.
Separate wave-array reads are gone. Each requested inner chunk is downloaded
once, including requests for different weather components. The existing
bounded cache and clear controls remain; endpoint, shard ETag and byte range
separate cache entries. Provider switching replaces a route's prior weather
and records the selected source, including on queued jobs.
Recorded R2 metadata and hand-computed fixtures cover exact byte ranges,
parameter ordering, antimeridian/time boundaries, parallel reads, source
isolation, cache reuse across routes/restarts and invalidation. The same
fixture stencil now needs two requests instead of four. Local mock timings:
209 ms serial and 84 ms with four requests allowed in flight.
Validation: 854 Rust tests, 519 UI tests, typecheck, clippy, formatting,
five UI performance checks and the offline boundary pass. Live R2 retrieved
all seven values at 48N, 5W, 2000-01-03 12:30Z with 11 requests and 1,363,360
bytes including metadata; memory reuse made no requests, and a restarted
reader used the disk chunk without downloading it again. Observed cold R2
times were 20.7–38.3 s on this connection. Tigris authentication, metadata and
missing-shard handling pass, but its bucket currently contains no data chunks
under `hindsight/data/c/`, so live weather/cache validation there awaits upload.
Desktop source-selection and cache-control tests pass, including persisted
R2/Tigris choices, cache limits and clearing only owned files. A live R2
desktop import fetched all 12 route positions, showed Weather: ready, and
cached 860,648 bytes. Source-selection, cache and completed-route screenshots
were inspected.

**2026-10-07: YellowBrick position failure recovery — complete.**
The importer discarded an already loaded boat list when positions failed,
then reported the whole tracker unavailable. It now retains the list,
search and selections, stops progress, explains that positions failed, and
keeps Import disabled. Retry targets the listed event and preserves those
choices; Open starts a new address normally. YellowBrick also tries its KML
when RaceSetup succeeds but the binary remains unavailable after retries.
A failed RaceSetup or cancellation never starts a KML fallback.
Both regressions were reproduced before the fixes. Live Comanche loaded two
boats and 1,576 positions; its public KML also responded successfully.
Validation: all 849 Rust and 519 UI tests, typecheck, clippy, formatting,
five UI performance checks and the offline-boundary check pass. Both desktop
tracker tests pass. The failure fixture keeps the selected boat and search
after both position feeds fail, then Retry recovers through KML despite
continued binary 503s and imports its 1,670 positions. Screenshots of the
failure, recovery and imported track were inspected.

**2026-10-07: Wave slider hover appearance — corrected.**
The generic enabled-button hover selector outranked the transparent rail's
hover rule, painting its 28 px drag hit area blue. The rail's override now
wins while keeping keyboard focus and drag behavior. Four existing slider
tests and the offline check pass. An isolated WebKit desktop check reproduced
the old background using equivalent hover/active classes, then confirmed all
three rails stay transparent in both states. Normal/hover screenshots were
inspected and match.

**2026-10-07: Hourly-only weather and fast track search — complete.**
Removed the three-hourly option, estimates and automatic recommendation.
UI, MCP and GRIB weather reads accept hourly sampling only. Older projects
keep their saved weather; the next fetch replaces a legacy coarse interval
completely, and its estimate includes all affected samples. The dialog and
all eight Help/translation catalogues now describe hourly downloads only.

Track search now prepares substring matchers once per query, checks each
vessel once, reuses matching row IDs for pagination, and checks the visible
page's files in parallel with one directory resolution. A disposable local
search index survives restarts, validates the metadata path/size/mtime, and
rebuilds on changes or corruption. Startup and metadata-folder changes warm
it on a background thread; search work runs off the async dispatcher.
The typing delay is 50 ms and stale answers cannot replace a newer query.

Measured read-only against the user's 261,631,595-byte library (80,585 tracks):
before, 5,212 ms first search and 66–212 ms subsequent searches; after,
0.46–9.90 ms with the index prepared, with identical query result counts and
file-availability counts. First-time index construction took 7,135 ms;
reloading its 91,861,194-byte cache after restart took 998 ms, both now warmed
in the background. These are local backend timings; typing adds 50 ms.
Validation: 848 Rust tests, 518 UI tests (the corrected new search test was
rerun separately), typecheck, clippy, five UI performance checks, formatting,
offline boundaries, and desktop track-library and weather-source tests passed.
Desktop screenshots were inspected, including the hourly-only fetch dialog.

**2026-10-07: Persistent Whirlwind chunk cache — complete.**
Requested cache directory, maximum size in GB and Clear cache supersede the
memory-only D27 rule for Whirlwind. Open Data remains in memory. Validated
compressed inner chunks use a versioned, checksummed, bounded LRU disk cache;
shard ETags prevent reuse across archive rewrites. Cache I/O runs on blocking
workers, and clearing invalidates writes already in flight without touching
project weather or unrelated files.
Settings defaults to the platform cache directory and 10 GB, saves directory
and limit changes, and exposes usage and Clear cache. All eight translations
and Help cover it. Saving cache preferences preserves other Settings drafts.
Validation: 845 Rust tests, 516 UI tests, typecheck, clippy, formatting, five
UI performance checks, the offline boundary check and both desktop weather
tests passed. The live desktop test cached 5,640,265 bytes for 1,501 Fastnet
2019 samples. After a full app restart, another project using that route
estimated zero chunk downloads and processed its samples in 233 ms; clearing
then returned cache usage to zero, made a refetch require 5,639,785 bytes,
and preserved all 1,501 saved sample results. Timings are local measurements.

**2026-10-07: Whirlwind estimate hangs — corrected.**
The route estimate's synchronous body ran inside Tauri's async dispatcher,
then called the Whirlwind runtime's `block_on`, which panicked before replying
to IPC and poisoned the metadata lock. The command now awaits a blocking
worker for the whole estimate, including provider construction/replacement,
and converts worker failure into an IPC error. Regression coverage exercises
the real Tauri dispatcher, repeated pre-request failures and switching back
to Open Data, plus the desktop fetch dialog and busy indicator.
The regression reproduced the original runtime panic before the fix. After
the fix, the full Rust and UI suites, clippy, formatting, typecheck, UI
performance and offline checks passed. The desktop regression confirms failed
estimates and retries stop the spinner. A separate live desktop run on a copy
of Assent's Fastnet 2019 route estimated 6 MB and processed all 1,501 samples
in 2.61 s after starting the fetch (1,487 with wind, archive gaps retained).

**2026-10-07: Historical weather source selection — complete.**
Added persistent Open Data / Whirlwind selection. The Whirlwind reader uses
verified live Hindsight v3 metadata, sparse inner-chunk Range reads, async S3
concurrency, parallel pure-Rust Zstandard decoding, bounded session memory,
cancellation and dataset provenance. Credentials live in a private application
config file, never the repository or frontend. Validation covers source
switching, sparse request planning, interpolation, cache reuse, error handling,
concurrency, the live bucket and the actual Settings dialog. The workspace
suite passed (832 tests), followed by the updated weather/settings regression
suites; all 516 UI tests, typecheck, translations/help checks, five UI performance
tests, formatting, clippy and the offline boundary check passed. The desktop
UX test verifies both choices and persistence. A live sample at 48N, 5W,
2000-01-03 12:30Z read wind, waves and current in 2.51 s, 16 requests and
1,344,444 bytes including metadata; its repeat made zero requests. The local
latency fixture took 405 ms with one request at a time and 116 ms with four
in flight. These are developer-machine measurements, not a throughput promise.

**2026-09-27: First draft.** Spec, plan and CLAUDE.md drafted from the
product description, a survey of VectorEffects, verified vendor formats
(YellowBrick, Geovoile, Blue Water Tracks), the `tracker-index` reference
scrapers, and the ERA5 and Copernicus Marine archives. Open questions in §6.

**2026-09-28: M3 risk spikes.** Measured on the development machine only
(2019 MacBook Pro, x86_64 Intel i9, Intel UHD 630, home broadband); the three
reference machines of spec §13 are still to be measured. Spike code is kept
as the foundation of `pe-env`, `pe-trackers` and `ui/src/polar/`.

- *Reanalysis* (`PE_TEST_LIVE=1 cargo test -p pe-env --test live`), 24 h at
  50N 5W. WeatherBench2 u10/v10: 3.32 MB per hourly global chunk; cold
  0.287 s/chunk one at a time, 0.097–0.111 s/chunk with 8 in flight
  (≈ 32 MB/s); warm (disk cache) 3 ms/chunk read + decode; open 1.2 s
  (12 requests). ARCO-ERA5 swh 1.77 MB, mwd 1.67 MB, u10 3.32 MB per chunk;
  0.07–0.17 s/chunk with 8 in flight; open 1.05 s (14 requests, coverage
  from `.zattrs` to 2026-09-21). CMEMS merged geoChunked utotal/utide/uo:
  0.88/0.81/0.75 MB per chunk (178 days × 16 × 8 cells), 0.78 s each
  (latency-bound); a missing (all-land) chunk is HTTP 403 and reads as
  missing. **5-day race, hourly:** 120 h × 10.1 MB ≈ 1.21 GB, ≈ 40 s cold
  at this bandwidth, currents ≈ 3 MB per box crossed; 3-hourly ≈ 0.40 GB,
  ≈ 13 s. Second boat of the same event (reopen + 24 cached hours): 0.76 s.
- *Decoders*: YellowBrick AllPositions3 for Fastnet 2025 (5,726,173 bytes,
  444 teams, 714,380 fixes) decodes in 68 ms (debug build); live fetch of
  both responses 2.8 s. Geovoile hwx decodes 24 Heures Ultim 2025 and
  Vendée Globe 2016 (29 boats, 107,459 fixes) with seeds parsed per site;
  Route du Rhum 2022's page gives the same seeds as 2025. **No 2024 site
  fixture**: recording one (New York–Vendée 2024) was blocked in this
  session; to do before M11. The Vendée Globe 2016 viewer page now answers
  HTTP 500, so its seeds come from Appendix A.
- *3D* (`ui/bench3d.html` in headless Chrome, ANGLE Metal on the Intel UHD
  630, 1280 × 800): 200k dots + 20 surfaces hold 60 fps (frame p50/p95/p99
  16.7/16.8/16.8 ms over 600 frames; one start-up hitch of 0.1–0.6 s for
  shader compilation). At 400k dots + 40 surfaces p95 is 33 ms, so the
  headroom is under 2×. Building 200k dots 28 ms in the page (8 ms in Node),
  20 surfaces 8 ms in the page including the three.js objects (1 ms for
  the meshes alone in Node); one lasso over 200k dots 8 ms in the page
  (project 4.5 ms + select 12 ms in Node with a 64-vertex lasso). Chrome is
  not WKWebView or WebView2: the Tauri webviews must be checked in M7.
- *Budgets* (spec §13): 3D 60 fps — **go** on this machine, reference
  machines unmeasured. Second boat < 5 s — **go** (0.76 s for one variable;
  four variables need their opened arrays kept per session in M9, since
  each open costs ≈ 1 s of metadata requests). Reanalysis cold — reported
  as above, with per-chunk completions every ≈ 0.1 s, so progress every
  second is easy. Edit to views < 100 ms — **go** for the 3D side (rebuild
  28 + 8 ms, lasso 8 ms); IPC transfer of 200k samples unmeasured (M7).
  Start screen < 1.5 s, ORC search < 30 ms, blend < 50 ms and opening 50
  tracks < 2 s are not exercised by these spikes: no evidence against,
  measured in M5, M8 and M14. ORC search, measured in M5: p99 2.4 ms per
  keystroke over the full catalogue (debug build, Intel i9) — **go**.
  Blend, measured in M14: 1.0 ms after an edit at 20 sources and 200k
  samples (debug build) — **go**.
- *D19 decided*: hourly stays the default (see §5).

**2026-09-28: M14b — tracks first, weather on request (D24).** Measured on
the development machine (Intel i9, home broadband, live, `PE_TEST_LIVE=1
cargo test -p pe-app --test tracker_speed`), the time from the call to a
pickable boat list, before → after (after: boat list / full event). The
network dominated every phase: decode, per-boat building, previews and
serialisation were each under 10 ms in debug (Fastnet: payload 565 KB
serialised in 5 ms, import of 10 boats 15 ms), so the fixes are
concurrency, an early boat list, gzip and taking weather out of the import.

| Event | Debug before | Debug after | Release before | Release after |
|---|---|---|---|---|
| YellowBrick Fastnet 2025 (444 boats, 714,380 fixes) | 1.04 s | 0.15–0.38 s / 0.34–1.46 s | 0.37–1.04 s | 0.18–0.38 s / 0.45–1.43 s |
| YellowBrick Middle Sea 2024 via KML fallback | 16.4 s | RaceSetup time (local: 0.00 s) / 14.7–15.0 s | — | 0.00 s / 15.0–16.0 s |
| Geovoile New York–Vendée 2024 (28, 64,453) | 0.74 s | 0.25–0.29 s / 0.43–0.50 s | 0.50–0.52 s | 0.23–0.29 s / 0.46–0.49 s |
| Geovoile Solitaire du Figaro 2024 leg 1 (45, 21,990) | 1.09 s | 0.30–0.35 s / 0.58–0.76 s | 0.57–0.67 s | 0.27–0.32 s / 0.56–0.85 s |
| Blue Water Melbourne–Hobart Westcoaster 2025 (5, 863) | 2.18 s | 1.55–1.84 s (one response) | 1.99–2.25 s | 1.48 s |

Ranges are repeated runs; the network varies by a factor of three between
runs. Blue Water answers everything in one response, so it gets no early
list; gzip took its body from 248 KB to 23 KB (RaceSetup 375 → 65 KB,
AllPositions3 5.7 → 3.8 MB). The Fastnet IPC payload is now a 71 KB list
then 515 KB (previews rounded to 1e-4°; was 565 KB). UI (vitest,
happy-dom, development React, 444 boats × 64 preview points,
`TrackerImportDialog.perf.test.tsx`): ticking a boat 107–112 ms → 21–30 ms
(memoised rows, the preview drawn as two paths instead of 444); the first
table draw 234–253 ms before, 267–295 ms after the list is already shown
(the old code measured 460 ms in the same later session, so the machine
varied; happy-dom is several times slower than a webview). The largest
cost the user saw, the weather step opened automatically after every
import (a 5-day race is ≈ 1.2 GB hourly, D19), is gone: importing makes no
reanalysis request (`importing_issues_no_reanalysis_request`).

**2026-09-28: M14d — ORC search by individual field (D26).** A user
request: "in the orc section, add some ux to search each field
independently." A "Search by field" disclosure under the ORC search box
(folded by default, remembered in `localStorage`) holds boat name, sail
number, country, model / type, builder, designer, year built from–to and
certificate year; every filled field and the main box must match, each
field's words only against that field. `pe-orc` keeps one pre-folded key
per field beside the all-fields key, so a keystroke still scans one string
per condition per record. Measured (`cargo test -p pe-orc --test
catalogue`, debug, Intel i9, `--test-threads=1`, three runs): 658 keystrokes
(the 188 before plus 470 typed into single fields), median 1.6–1.7 ms, p99
2.5–2.6 ms; before, 188 keystrokes, median 1.8 ms, p99 2.8–4.4 ms on the
same machine that day. First load and index 0.18–0.24 s → 0.27 s.

**2026-09-28: M14e — small weather downloads, per-position storage only
(D27).** A user request. Measured live (`PE_TEST_LIVE=1 cargo test -p
pe-app --test weather_download`, debug build, Intel i9, home broadband),
fetching hourly through the real archives:

| Track | Downloaded | Whole chunks (before) | Time | Stored (`tracks/<id>.json`, deflated) |
|---|---|---|---|---|
| Fastnet 2025 "Black Betty", 120 h, 1,585 samples | 159.6 MB, 1,388 requests | 1,279.5 MB | 99 s | 49.5 KB, of which the environment 13.3 KB (8.4 B/sample) |
| Second boat "Mare", 120 h, 2,034 samples, same session | 9.6 MB, 116 requests | 46.9 MB more | 16 s | 61.2 KB, environment 15.4 KB (7.5 B/sample) |
| "Teamwork", 72 h, 970 samples (after wind and waves shared one pool) | 111.1 MB, 1,118 requests | 836.5 MB | 51 s | 32.2 KB, environment 8.5 KB (8.8 B/sample) |

The estimate said 155.1 MB and 104.5 MB for the first and third. The
provider end to end over every tier: 10.1 MB where whole chunks would be
69.4 MB. On a synthetic 1,000 samples the track entry went from 120.2 KB
(schema 1, one object per sample, derived values included) to 33.7 KB
(schema 2 by column), the environment 7.0 KB of it. The before column is the
sum of the sizes of every chunk touched, from their headers, not a second
download (the live budget was 300 MB). Currents dominate the time: the NW
Shelf boxes are small and Copernicus Marine answers in 0.3–0.8 s, so
currents are now read beside the wind and waves. Review round 1 added
reading every ERA5 head of the track side by side before the batches: the
same 120 h boat then took **48.4 s** instead of 99 s, the same 159.6 MB.

**2026-09-28: M14c — agent-driven UI testing through WebDriver (D25).** A
user request. `npm run ux` builds `pe-app` with `--features webdriver`, then
for each of five tests starts the real development build (Vite on a free
port, the window 1440 × 900 without focus, a fresh `PE_AUTOMATION_ROOT`),
drives it and saves a picture per step under `target/ux-shots/<test>/`:
new project → Map stage; Polar files Import… (picker answered by the
dev-only queue) → row → curve pixels in the plot → 3D surface pixels; help
search in en/fr/de → result → orange flash around Fit the world; YellowBrick
from a local fixture server → boat list while positions download → loaded,
exactly one Cancel → a boat imported and drawn on the map; Settings → French
labels and `settings.json` written in the run's own root. Five pass in
3 min 50 s – 4 min 45 s on the Intel i9 (≈ 35–50 s each, mostly start-up).
Screenshots do not use the plugin's `/screenshot` (every canvas blank) or
`screencapture` (the wallpaper without the Screen Recording permission):
the client serialises the DOM into an SVG, then paints each canvas —
redrawn synchronously, since a WebGL buffer is not preserved — clipped to
itself, with whatever is positioned over it drawn again on top. Animations
are shown at rest. The MCP server (`pe-driver`) and the CLI share the
client; `npm run tools:test` covers the port-line parser, file names and
the MCP handshake.

**2026-09-29: M17a — carried polish, robustness and performance.** Sixteen
items earlier reviews deferred (report: `.superpowers/sdd/plan/task-17a-report.md`).
UI: plot labels laid out without collisions (`axisLabels`); the weather
pre-flight's legend on one line and its 3-hourly note only until the user
chooses; Fetch weather for selected tracks skips tracks already fetching
and clears the ticks; a shorter ORC placeholder (en/fr/de); the edit table
in the display speed unit; map tracks two device pixels wide (the Middle
Sea boat: 4,802 → 9,087 pixels of its colour in `npm run ux`); the 3D
"step" near 30 kn is the half-cup's rim seen in perspective over Farr 40's
own data (its 30° cell drops from 5.15 kn at 25 kn to 3.89 kn at 30 kn),
not a mesh fault — the side view shows the 30 kn rim level at every angle.
Robustness: gunzip preallocation bounded by the body cap; a dropped
`Pending` stops its own download; `tracker://listed` carries the dialog's
download key; a GRIB export panicking mid-way leaves no temporary (tested).
Tooling: the compositor clips each canvas to what the window shows.
*3D at 512 × 512* (two surfaces, packet arrays → `setData`, stand-in
renderer): Node 102–162 ms before, 19–48 ms after; in the WKWebView of the
dev build 51–88 ms before, 13–40 ms after. *Edit to every view* (spec
§13), re-measured in a release build (`perf_edit`, 20 polar sources, 20
tracks × 10,000 samples, Intel i9; 1-min load 3.4 at the first run, 7–12
during later ones from other processes): Rust's side of an edit 15.7–36.2
ms in total over three edit kinds and five runs (a polar source edit
15.7–23.5, a segment drag step 23.2–25.2, excluding 1,000 samples
29.8–36.2), of which the 3D scene 9.8–16.4 ms; blend after an edit 0.6 ms.
Within the 100 ms budget with the frontend's rebuild (above) on top.

**2026-09-29: M17b — translation and help completion.** French and German
audited string by string against the English and the glossary (report:
`.superpowers/sdd/plan/task-17b-report.md`). Terminology made one word per
concept (cell *cellule*/*Zelle*, grid *Raster*, statistic *Kennwert*, event
*Veranstaltung*, dialog *fenêtre*, tick *ankreuzen*, leave out
*écarter*/*weglassen*, Shift *Maj*/*Umschalt*); French punctuation spacing
normalised to U+202F; stale help (3D samples, Compare, units, settings)
rewritten in all three languages. The glossary gained every sailing,
meteorological and ORC term with the review status "machine-drafted, needs
a native sailor" and ⚑ on the least certain; `docs/TRANSLATION-REVIEW.md`
is the reviewers' checklist (the M17 deliverable "reviewed by a sailor who
speaks each language" stays open for them). Twenty sailor's words per
language find their control first (`sailor-queries.json`, unit-tested and
driven in `npm run ux` → `10-languages`, which also photographs every main
area in French and German). Fixed from those pictures: clipped source-row
buttons, the 3D side panel over the source list, the 3D toolbar under the
side panel, the track row squeezing its name, the navigation's sideways
scroll from filter fieldsets and the tracks' button row, an autosave option
wider than Settings, the start screen's default name staying English; the
screenshot compositor now draws scrolled containers at their offset. The 2D
plot's rings, slice, band and hover and the 3D drag readout follow the
display speed unit. Compare's three 512 × 512 surfaces, input + `setData`,
are pinned under 100 ms (min of 12 runs; 63–78 ms alone, 93 ms under the full suite's load).

**M17b review follow-up.** Status hints, errors and save/undo messages keep
their translation keys until rendered, including translated parameters, so
a language switch relabels an existing message. Shortcut labels use the
language's modifier names, and the German source action uses *Vergleichen*
while the stage remains *Vergleich*. Track filters show and accept the
chosen speed and wave-height units, converting to knots/metres at the input
boundary; leaving a rounded display value untouched creates no edit.
Long filter labels have more room; date/time inputs and dropdowns occupy a
full row so their native controls fit without sideways scrolling.
The 512 × 512 timing tests run serially in `npm run ui:perf`, separately
from the parallel correctness suite, in both CI and release builds. The
language UX flow checks status relabelling and edits BSP, TWS, wave-height
and current filters through Settings' km/h/feet and knots/metres choices.
Verified locally: 612 Rust tests, 340 UI correctness tests, four serial
performance tests, ten driver unit tests and all ten UX flows passed; the
language flow passed again after the final layout fixes (65 screenshots).
Formatting, Clippy, type checking, the production UI build and the offline
check passed. Compare's three-surface CPU rebuild measured 57–90 ms in
warm runs and the two-surface rebuild 18–40 ms on this Intel i9; reference
machine measurements and native-sailor translation review remain open.

---

## 1. Sequencing strategy

1. **Scaffold the VectorEffects skeleton first** (M0–M2) so every later
   feature lands in a shell that already saves, translates and is searchable.
   Retrofitting i18n and the feature registry is far more expensive than
   starting with them.
2. **Retire the three technical risks early**, as spikes before their
   features (M3): the reanalysis fetch cost, the hwx and AllPositions3
   decoders, and 3D performance with 200k dots.
3. **Polars before tracks.** The polar core, file import and ORC give a
   working, exportable app (M4–M7) before any network code.
4. **Tracks by source, file first.** File import proves the sample pipeline
   without scraping (M8); then environment (M9); then each tracker (M10–M12).
5. **Editing, blend, compare, export** (M13–M16) once every source exists.
6. **Release on all five targets** from M0 on in CI; signing and packaging in
   M18.

---

## 2. Milestones

### M0 — Workspace, CI on five targets, offline check · **complete**

**Goal:** an empty Tauri app that builds, tests and bundles on every target.

**Deliverables**

- Cargo workspace (edition 2024, resolver 3, pinned toolchain), crates from
  `CLAUDE.md` as stubs, workspace lints (`unsafe_code = "forbid"`, clippy
  `unwrap_used`/`expect_used` deny), `rustfmt.toml`, `clippy.toml` — copied
  from VectorEffects.
- Root npm workspace owning the Tauri CLI and scripts; `ui/` with React 19,
  TypeScript, Vite, Vitest.
- `pe-app` with the `AppError { kind, message }` pattern, one command, ts-rs
  export example and `npm run bindings`.
- `tools/check-offline.sh` adapted: network allowed only in `pe-env` (hosts
  `storage.googleapis.com/weatherbench2`,
  `storage.googleapis.com/gcp-public-data-arco-era5`,
  `s3.waw3-1.cloudferro.com/mdl-arco-`) and `pe-trackers` (`yb.tl`,
  `cf.yb.tl`, `*.geovoile.com`, `api.bluewatertracks.com`); CSP `'self'`.
- GitHub Actions: CI (fmt, clippy, tests, UI typecheck/test, bindings drift,
  offline) on macos-15, macos-15-intel, ubuntu-22.04, windows-2022, and a
  Windows ARM64 build job (`windows-11-arm` runner or cross-build from x64).
  Release workflow with tauri-action for all five targets.

**Acceptance**

- A bundled app starts on all five targets (manual check on the Windows ARM64
  reference laptop).
- `cargo tree` shows no `openssl-sys`, `blosc-src` or other `*-sys` crate
  except those Tauri itself needs.

**Risks:** Windows ARM64 is new relative to VectorEffects. `ring` needs clang
for `aarch64-pc-windows-msvc`; confirm the runner has it.

---

### M1 — Document model, `.wpsproj`, history, project lifecycle · **complete**

**Goal:** the full data model from spec §4 exists, round-trips and is
undoable, before any of it is visible.

**Deliverables**

- `pe-core`: `Project`, `Source`, `Overlay`, `Track` (fixes and samples as
  plain data), IDs, canonical floats, `Command` + `History` (200 entries,
  coalescing).
- `.wpsproj` ZIP I/O with `META-INF/version`, `project.json`,
  `tracks/<id>.json`, fixed entry timestamps, atomic save, migration table.
- Session state in `pe-app`: dirty flag and revision, New / Open / Save / Save
  As / Close with `discard_unsaved` guards, recent list (10), autosave and
  recovery.

**Acceptance**

- Property test: any generated project survives save → load → save with
  byte-identical output.
- Every command has an undo inverse test.
- Opening a file with a newer schema is refused with a message naming both
  versions.

---

### M2 — Shell: start screen, layout, settings, i18n, help search · **complete**

**Goal:** the app looks and behaves like VectorEffects with the PolarExplorer
palette, in three languages, with a working feature search.

**Deliverables**

- Start screen (spec §3.1) with the Harbour theme and ported themes.
- Project window layout (spec §3.2): title bar, project menu, collapsible left
  navigation with three empty sections, stage switcher, right panel, status
  bar.
- Save guard dialog and flow copied from VectorEffects (`saveGuard.ts`,
  `UnsavedChangesDialog.tsx`).
- Settings dialog and `settings.json` (spec §3.4).
- i18n: English, French, German catalogues, glossary files, coverage tests,
  language picker in Settings and on the start screen.
- Feature registry, search-as-you-type, reveal steps, orange flash
  (`--flash`), `features.test.ts`.
- Help window with one topic per area in all three languages.
- World map stage: WebGL2 basemap from VectorEffects' `basemap.bin`,
  equirectangular and orthographic.

**Acceptance**

- Switching language relabels every visible string, tooltip and the native
  menu with no reload.
- Every control in the shell is found by the search in each language and is
  flashed orange after its panel is revealed.

---

### M3 — Risk spikes · **complete**

**Goal:** measure the three unknowns before building on them. Spike code may
be thrown away; the numbers and fixtures are kept.

**Deliverables**

1. **Reanalysis cost.** Pure-Rust zarr v2 + blosc/LZ4 reader (port
   VectorEffects' `ve-zarr` HTTP store and `blosc.rs`). Fetch u10/v10 from
   WeatherBench2 and swh/mwd from ARCO-ERA5 for a 24 h window; fetch
   `utotal` from the CMEMS geoChunked store at one point. Record bytes,
   seconds and cache hit behaviour. Decide hourly vs 3-hourly default (D19).
2. **Decoders.** Rust decoders for YellowBrick AllPositions3 (Appendix B) and
   Geovoile hwx (Appendix A) passing against recorded fixtures (the Fastnet
   2025 binary; one 2024 and one 2016 Geovoile site).
3. **3D.** A three.js scene with 200k instanced dots and 20 surfaces on the
   three reference machines; lasso selection over 200k dots.

**Acceptance:** a short report appended to this plan's changelog with the
numbers, and a go/no-go on each budget in spec §13.

---

### M4 — Polar core and polar file import · **complete**

**Deliverables**

- `pe-polar`: `Polar` (TWA × TWS grid, optional cells), bilinear resampling
  without extrapolation, port/starboard folding.
- Expedition and Adrena/grid readers (format sniffing, all header variants,
  errors with line and column) and writers.
- Polar files section: multi-file import, list, remove (spec §6).
- Source list with colours, palette allocation, visibility, weight, rename,
  reorder (spec §8).

**Acceptance**

- Golden-file round trip for each format.
- Malformed-input tests never panic.

*M9b, user request, 2026-09-28*: the user's `polar_examples/` (688
real-world files, committed as test data) exposed two importer gaps, both
narrow tolerances rather than a loosened check — an Expedition label row
(`twa0 bsp0 TwaUp bspUp …`, or space-separated `pol Twa0 Bsp0 UpTwa UpBsp
…`) is now skipped instead of refused, and the TWS axis accepts up to 70 kn
(`MAX_TWS_KN`) while boat speed stays capped at 60 (`MAX_SPEED_KN`); a TWS 0
column and TWA 0 row of zeros were already accepted. 687 of 688 files parse;
`polars/J46 heel.txt` is a heel-angle table, not a boat-speed polar, and is
excluded by name with its reason recorded in
`pe-polar/tests/polar_examples.rs`, which walks the whole directory and
spot-checks four files by hand. `pe-app/tests/polar_import.rs` imports a
handful of the same files through `import_polar_files`.

---

### M5 — ORC catalogue and search · **complete**

**Deliverables**

- `tools/orc-catalogue-builder`: reads a jieter/orc-data checkout's
  `site/data/**.json`, normalises fields, writes `crates/pe-orc/data/
  catalogue.bin` with the source commit hash.
- `pe-orc`: lazy load, tokenised search index over all fields, ranking from
  spec §5.2.
- ORC section: search-as-you-type with thumbnails, filters, add, remove.
- ORC VPP → `Polar` conversion (both speed axes; beat and run angles).

**Acceptance**

- Search p99 < 30 ms per keystroke over the full catalogue on the slowest
  reference machine.
- A known certificate's VPP matches the ORC PDF values to 0.01 kn.

**Risks:** the per-boat files' schema changed across years; the builder
must report records it drops and why.

---

### M6 — 2D polar plot · **complete**

**Deliverables:** the right-panel polar plot (spec §9.2) with TWS slider,
source curves, blend placeholder, hover.

Built as specified, with three points settled that the brief left open (D21):
"All" draws one curve per visible polar source per wind speed that source's
own grid has (the classic multi-curve diagram), rather than one shared slice;
curve points are read at each source's own TWA axis (equivalent to a finer
sweep, since interpolation is piecewise-linear in TWA, but cheaper and exact
at every known angle); and the Map-stage overlay is a toggle owned by the
shell (`App.tsx`), opened by the panel's "Full size" button or the
`overlay:plot` reveal step, closed by its own button, Escape, or switching
stage. `polar_plot(tws: number | null)` returns curves, the TWS domain,
`dots` (always empty; the shape is final for M8/M9) and `blend` (always
`None`; the hook for M14). Tracks are not polar sources, so they never
contribute a curve, only future dots.

---

### M7 — 3D polar view · **complete**

**Deliverables**

- `ui/src/polar/` three.js scene (spec §10.1–10.2): polar-tower and
  Cartesian layouts, surfaces per source, instanced dots, legends, cameras,
  colour-by modes.
- Selection (click, shift, lasso, box), exclude/include as undoable commands
  (spec §10.3).
- Rust → UI transport for large arrays: a binary IPC response
  (`tauri::ipc::Response`) of packed `f32`, not JSON.

**Acceptance:** spec §13 3D budgets met on the reference machines.

Built as specified. The generic selection and exclusion machinery is
complete; per the pre-flight ruling, Exclude and Include act on **polar
nodes** of ORC and file sources now (`Command::ExcludeCells` /
`IncludeCells`, one history entry per action, a `Batch` across sources),
and `set_excluded` already takes sample ids, ignored until tracks have
samples (M8). `polar_scene` returns the scene as a packed binary
`tauri::ipc::Response` (layout in `pe-app/src/polar3d.rs` and
`ui/src/polar/scenePacket.ts`, pinned by `ui/src/polar/fixtures/scene-v1.bin`
from both sides). Samples, the blend surface (M14), colour by Hs / current /
time and "show filtered" have the wire shape and the controls but nothing to
show yet: the controls are offered disabled with a tooltip. "Show on map" is
disabled until M8. `pe_polar::blend_input` is the source grid the blend
will read, with excluded cells empty.

*Measured 2026-09-28 on the development machine* (2019 MacBook Pro, Intel
UHD 630, `ui/bench3d.html` in headless Chrome with ANGLE Metal, 1280 × 800,
now drawing M7's shaped dots): 200k dots + 20 surfaces hold 60 fps, frame
p50/p95 16.7/16.8 ms over 300 frames in two runs (p99 16.8 and 33.2 ms; one
shader-compile hitch of 0.15–0.33 s at start). 400k + 40: p95 33 ms, so
the headroom is still under 2×. Per edit at 200k samples: Rust packs the
scene in 29 ms (debug build) into 8.0 MB; the page unpacks it in 2.7 ms
(views, no copy) and rebuilds the drawn dots in 15 ms; a lasso takes 6–7
ms. Budget: **go** on this machine; the reference machines and the Tauri
webviews (WKWebView, WebView2), including the IPC transfer of the 8 MB
buffer, are still unmeasured — to do before release (M18).

---

### M8 — File tracks and the sample pipeline · **complete**

**Deliverables**

- `pe-tracks`: fixes, heading and speed derivation (spec §7.4), GeoJSON
  and CSV import with the column-mapping dialog (spec §7.3), boat picker for
  multi-boat files.
- Track list and map drawing (spec §7.1, §9.1), hover details.
- Samples without environment: dots appear once environment arrives (M9).

**Acceptance**

- Heading and speed derivation tests against hand-computed values, including
  the antimeridian, a stationary boat and a single-fix track.

Built as specified (acceptance: `pe-tracks/src/derive.rs` tests, values
hand-computed on the 6,371,229 m sphere: equator, antimeridian both ways,
north at 50°N, over the pole, off-line central difference, stationary,
single fix, gaps, prefer). `pe-tracks` holds geodesy, derivation, filters,
time parsing and the GeoJSON and CSV readers (never-panics proptests);
`pe-app/src/tracks.rs` inspects and imports outside the lock, one
`Batch` per import; `map_tracks.rs` sends visible tracks as a packed binary
buffer (unwrapped longitudes). New undoable commands: `ExcludeSamples`,
`IncludeSamples`, `SetSampleFilters`, `SetDerivation` (carrying every
sample's motion both ways, since `pe-core` cannot derive). Ids are capped at
2^53 − 1 (`pe_core::MAX_ID`, `Project::reserve_ids`, validation) so they
cross IPC as exact JavaScript numbers. Per the controller rulings: a sample
without wind has no place in the polar and is not drawn (the 2D and 3D dots
appear once M9 fills TWS/TWA — `samples_become_dots_once_they_have_wind`
and `samples_appear_in_the_3d_scene_once_they_have_wind` inject values to
prove it); sample exclusion and "show on map" are wired; the M7 minors are
fixed (only drawn dots are counted or acted on, numeric selection keys —
keying and re-finding all 200k samples selected measured 48 ms in Node
against 188 ms for the string keys it replaces, camera refit on project change, `buildDots` uses the
shown mode, "?" translated); the ±1 kn dot band is a Settings preference.
The wind, wave and current filters are modelled and evaluated but offered
disabled until M9. The import dialog's configuration controls (column roles,
time format and pattern, speed unit, boat picker) are tagged and registered;
since the dialog exists only once files are chosen, their search entries
land on File… (`Feature.landing`) and say a file must be chosen first. Only
its Cancel and Import answer buttons are untagged (spec.md 3.6).

*Measured 2026-09-28* on the development machine, debug build (`pe-app` at
opt-level 0), 50 tracks × 10,000 fixes: project summary 49 ms, map packet
141 ms (10 MB), 3D scene 173 ms, 2D plot 51 ms, excluding 10,000 samples
87 ms (summary included). Release numbers and the webview's draw rate at
this size are still to measure (M18).

---

### M9 — Environment: wind, waves, currents · **complete**

**Deliverables**

- `pe-env`: zarr v2 reader, blosc/LZ4, HTTP store (blocking reqwest +
  rustls/ring), on-disk chunk cache with size limit and LRU eviction, dataset
  coverage from metadata.
- Datasets: WeatherBench2 hourly u10/v10; ARCO-ERA5 u10/v10 (after
  WeatherBench2's end), swh, mwd; current tiers from spec §7.5.1 with
  `scale_factor` handling.
- Sampling: bilinear space, linear time, vector interpolation for
  directions, land-NaN stencil rule.
- Current correction (spec §7.5, D13).
- Job system: worker pool, progress events, cancel, partial results, resume;
  pre-flight download size estimate.
- Sample filters UI (spec §7.6).

**Acceptance**

- Sampled u10 at 2020-07-27 12Z, 50N 5W matches a value decoded
  independently from the same chunk (record the reference in the test).
- A cancelled job leaves a consistent project that resumes to the same
  result as an uninterrupted one.

Built as specified (acceptance: `pe-env/tests/sampler.rs`
`acceptance_u10_at_50n_5w_matches_numcodecs` — u10 = 9.382978439331055 at
2020-07-27T12Z, 50N 5W through the whole provider, the reference decoded
from the same WeatherBench2 chunk with numcodecs 0.12.1 in M3 and recorded
in the test; `pe-app/tests/env_jobs.rs`
`a_cancelled_fetch_resumes_to_the_uninterrupted_result` and
`…_saves_as_partial_and_resumes_after_reopening`, against a fake provider,
no network). `pe-env` gained the `Provider` trait and `Reanalysis`
(WeatherBench2 then ARCO wind, ARCO waves as unit vectors, the four current
tiers with int16 `scale_factor` unpacking and the surface level), whole-chunk
reads grouped per batch, the pre-flight estimate, HTTP retry/backoff and a
body cap, and the cache carries from M3. Current fixtures
(`currents-crop/`, 340 KB) are the NW Shelf, merged and GlobCurrent chunks
around 49.86N 5.13W cut to 48 hours, with numcodecs references
(`currents-crop/record.py`). `Sample::relate` (in `pe-core`, plain vector
arithmetic on stored values) recomputes TWA, tack, the D13 correction and
the wave angle, and `SetDerivation` runs it both ways (M8 carry). Jobs live
in `pe-app/src/env.rs`: one runner, per-batch writes, cancel, resume,
Refetch, cancel on project replacement; the import opens the pre-flight
(spec §7.5, §13). New undoable commands `SetUseCorrected`,
`SetStokesDrift`. The autosave snapshot is renamed into place under the
lock (M3 carry).

*Measured 2026-09-28*, live (`provider_end_to_end`, 5 positions through
every tier): cold 29.9 s, of which about 15 s is opening seven stores'
metadata; the same provider again 0.15 s (the second boat of a session);
a new provider over a warm cache 15.6 s (metadata only). A real race opens
only the stores its positions need. 5-day cold numbers stand as in M3.

---

### M10 — YellowBrick · **complete**

**Deliverables:** `pe-trackers::yellowbrick` (URL → key, RaceSetup with
ISO-8859-1, AllPositions3 decoder, KML fallback), the shared tracker dialog
(spec §7.2), boat picker with map preview, per-session event cache.

**Acceptance:** fixture tests from recorded responses; one live test behind
`PE_TEST_LIVE`.

*Done 2026-09-28.* `pe-trackers` gained the `TrackerClient` trait and the
shared event model (`event.rs`: resolve without network, one fetch
returns the whole event), `http.rs` (`Fetcher`: 256 MB body cap, the
§7.7 transient/permanent rules with three retries, progress, cancel; a
small copy of `pe-env`'s classification, since the two network crates do
not depend on each other) and `kml.rs` (a bounded reader for YellowBrick's
per-team `gx:Track` placemarks). The YellowBrick client reads RaceSetup
tags into a division and each team's own start and `finishedAt`; a binary
that does not decode, is refused, or is a web page falls back to
`https://yb.tl/<key>.kml` (the CDN answers 504 for it) with a 10-minute
timeout. `TrackOrigin::Tracker` gained optional `model` and `division`
(left out of the file when absent, so no schema bump). `pe-app/trackers.rs`
downloads on a worker with progress events and an immediate Cancel, keeps
four events per session, and imports through the file import's commit
(one Batch, next palette colours), setting each boat's start–finish as its
time window; the environment pre-flight opens after it. The UI adds
`TrackerImportDialog` (address, progress, Retry, table with search and
tick-all, SVG map preview over the basemap coastline), the Race trackers
help topic, and the registry entries (landing on YellowBrick…).

*Fixtures* (2026-09-28): Rolex Middle Sea Race 2024 — RaceSetup (97 KB),
the first three teams of `AllPositions3` (45 KB of 1.37 MB) and of the KML
(844 KB of 23 MB). The KML is checked against the binary: every KML fix is
a binary fix at the same time and 1e-5° position; the binary also holds
reports at repeated times (1689 against 1670 for the first team). A local
server replays the recordings through the client (primary, fallback, 5xx).
*Live* (`PE_TEST_LIVE=1`): Fastnet 2025 through the client 444 boats,
714,380 fixes in 0.4–1.6 s; Middle Sea 2024 KML 23 MB in 17 s; an unknown
key answers 500 four times and is reported "not answering". *Unverified:*
the AllPositions3 alt/lap/pc layouts — every race probed (about 50 keys:
Fastnet 2023/2025, ARC 2025, RMSR 2024 and others) has flags `0x02`, so
Appendix B's note stands.

---

### M11 — Geovoile · **complete**

**Deliverables:** `pe-trackers::geovoile`: viewer HTML parsing (rooturl,
resourcesurl, seeds), versions file (JS object literal parser), hwx decoder,
config XML, tracks and reports; multi-leg support; clear refusal of Flash and
pre-2016 trackers.

**Acceptance:** fixtures from at least three sites of different years decode
to the boat count and first/last fix recorded by hand from the live viewer.

**Risks:** Geovoile changes its format between editions. The decoder must
fail with "unsupported Geovoile version" rather than produce garbage: check
that the output parses and the first fix is a plausible time and position.

*Done 2026-09-28.* The `Geovoile` client resolves a viewer address with no
network (exact `*.geovoile.com` host, no user name or port, root segments,
`?leg=<n>`), then reads the page, versions, config, tracks and reports
(five progress steps). Every resource address from the page
(`resourcesurl`, `versionsurl`) is resolved against it and must be HTTPS
on a Geovoile host before it is requested. Reports are parsed by column
name (the 2016 and 2025 orders differ); each gives its heading and speed
(when non-zero) to the fix nearest it within 60 s, the latest status, and
the arrival or hidden time as the boat's finish. The config gives each
boat's class (the division) and its run's start. Legacy pages (no
`rooturl`: 2012–2015 HTML, Flash) are a new `TrackerError::Legacy`, shown
"older tracker" with no Retry; a format change stays "unsupported Geovoile
version" (its own error kind now, also without Retry). Both network crates
build their clients with a redirect policy: same host, or HTTPS to an
allow-listed host (tests end to end through local servers). M10 carries:
the tracker dialog ignores backdrop clicks while downloading or importing;
YellowBrick counts its KML step from the start, so progress never goes
back (asserted in the fixture and pe-app tests); the session keeps events
up to 2,000,000 positions rather than four events; spec §7.7 says the
tracker download's progress is in its dialog. The dialog offers a leg
picker for a race in legs.

*Fixtures and acceptance* (2026-09-28): five sites of four editions
decode to the boat count, total fixes and the first configured boat's
first and last fix — Vendée Globe 2016 (29 boats, 107,459 fixes; Appendix
A's seeds, its page answers 500), Route du Rhum 2018 (26, 28,038; seeds
split over two images, empty versions file), New York Vendée 2024 (28,
64,453), Solitaire du Figaro 2024 leg 1 of 3 (45, 21,990) and 24 Heures
Ultim 2025 (14, 8013). The references are the independent Python decoder's,
checked against each race's facts (start ports and times, official
arrivals in the reports: every arrived boat's last fix is after its
arrival, the tracks running on into port). They were not read off the
live viewers by eye: no browser in this environment. The Route du Rhum
2014 page is recorded as the refused generation. *Live*
(`PE_TEST_LIVE=1`): New York Vendée 2024 28 boats, 64,453 fixes (10,223
with official speed) in 0.7 s; Solitaire 2024 leg 1 45 boats, 21,990 fixes
in 0.6 s. The Vendée Globe 2024 tracker answers "Not available" at
`vendeeglobe.geovoile.com/2024/tracker/` (refused as no public event).

---

### M12 — Blue Water Tracks · **complete**

**Deliverables:** `pe-trackers::bluewater` (slug from URL, race JSON,
GeoJSON positions with SOG/COG).

*Done 2026-09-28.* The `BlueWaterTracks` client resolves
`race.bluewatertracks.com/<slug>`, an `api.bluewatertracks.com/api/race/
<slug>` link, or a bare slug (no network), then makes one request:
`GET /api/race/<slug>`. Live probing during research found the API answers
an unknown slug with HTTP 200 and `race` an empty array rather than an
object (`{"positions":[],"race":[]}`), not a 404 — checked before the
object is parsed, and a plain 404 is read the same way, both as
`NoSuchEvent`. A boat's division is the distinct `division` values across
its `handicaps` (each rating system usually agrees); SOG and COG are given
per position and used exactly as given (0 is a real value here, unlike
Geovoile's official reports). The event's start is `raceStartTime`; a
boat's finish is its own `finishTime` else the race's `trackTimeFinish`.
Positions are not guaranteed sorted or deduplicated per boat (unlike
YellowBrick's and Geovoile's own formats), so each boat's fixes go through
the same `pe_tracks::normalise` a file import uses before the client
returns, keeping `TrackerBoat`'s "oldest first" invariant for the dialog's
table and map preview.

M11 review carries, done alongside: (1) a new `TrackerError::Http { status,
why }` replaces matching "answered 404" in the message text — Geovoile's
same-generation 404 check now matches the status; (2) Geovoile's
`parse_viewer` clamps `nblegs` to 1–99 and refuses a `numleg` outside
`1..=nblegs` as unsupported, rather than trusting the page; (3) the tracker
session cache in `pe-app` now also keys by the *requested* key (an alias to
the actual one a race in legs was downloaded under), so re-pasting a
multi-leg address that resolves without a leg still hits the cache instead
of downloading again.

*Fixtures and acceptance* (2026-09-28):
`bluewater/melbournehobartwestcoaster2025-race.json` — the 2025 Melbourne
Hobart Westcoaster (5 boats, 863 positions) as served, crew, bios, images
and sponsor details cropped out. The whole event decodes through the
client (local server) to the recorded boat count, fixes and first/last fix
of Alien, every fix sorted, given and never derived. *Live*
(`PE_TEST_LIVE=1`): the same race, 5 boats, 863 fixes, 2.2 s, matching the
fixture exactly; an unknown slug reads as no public event.

---

### M13 — Polar segments and editing · **complete**

**Deliverables**

- Track → polar segment binning with statistic, minimum count, per-cell
  count and spread (spec §12.1).
- Edit mode per source (spec §10.4): node drag, table editor, scale, smooth,
  reset; overrides as overlay commands.
- Recompute pipeline: an edit invalidates only the affected source and the
  blend; every open view refreshes.

**Acceptance:** spec §13 edit-to-view budget; removing all overrides restores
the source byte-for-byte (invariant 1).

Built as specified (spec §10.4, §12.1, D22). `pe-polar::segment` bins a
track's used samples onto the output grid (nearest node, half-step bins,
beyond the outer half-steps left out, ties up; linear-rank percentiles;
spread = sample standard deviation; count and spread kept for every cell);
`pe-polar::edit` writes overrides onto a grid and computes scale and smooth
(3×3 binomial kernel over present neighbours). `pe-core` gains
`Command::EditCells { source, action, cells: [CellEdit { before, after }] }`
— one command for every tool, `action` naming the history entry, drags
coalescing via `Command::merge` — and `Command::SetSegmentStatistic`;
`cell_overrides` are validated sorted, one per cell, 0–60 kn. Acceptance:
`removing_every_edit_restores_the_source_byte_for_byte` (pe-core) and
`every_tool_is_one_entry_and_reset_all_restores_the_source_byte_for_byte`
(pe-app, through IPC) compare `.wpsproj` bytes. `pe-app/src/derived.rs`
holds the derived cache (per-source revisions moved only by the commands
that reach a source, never saved; untargeted changes invalidate all, the
environment fetch names its track); the 3D scene, the 2D plot, the table and
the project summary all read it. New IPC: `polar_edit_surface`,
`edit_polar`, `set_segment_statistic`, `polar_plot_dots`; `polar_scene`
takes `focus` and `samples_key`. Carried items: M6 — 2D curves read each
source through its overlay (edits in, excluded nodes empty); M7 — scene
layout v2 (48-byte header, samples key, flags-only mode; fixtures
`scene-v2.bin`, `scene-v2-flags.bin`), and the frontend keeps the samples'
dots when their flags are unchanged, rebuilding the nodes alone; M8 — 2D
dots travel as the packed "PE2D" buffer (`dots-v1.bin`) and draw as squares
beyond 20,000.

*Measured 2026-09-28 on the development machine* (2019 MacBook Pro, Intel
i9; `cargo test --release -p pe-app --test perf_edit -- --ignored`), 20
polar sources and 20 tracks × 10,000 samples, Rust side of edit → every
view (command + summary, flags-only 3D scene, 2D curves, 2D dots at a
slice, table): a table edit on a polar source **25.8 ms** (scene 16.9 ms,
0.91 MB); a drag step on a track segment **23.4 ms** (no re-binning); excluding
1,000 samples of one track **36.1 ms** (that track re-binned). Cold full
scene 64.4 ms, warm 34.3 ms, 8.1 MB. Debug build (pe-app at opt-level 0):
61, 67 and 97 ms. Frontend (Node, 200k samples, warm): unpacking a
flags-only scene 1.1 ms and rebuilding the dots 5 ms when the flags are
unchanged (36 ms when every sample's dot is rebuilt). **Go** on this
machine at under 100 ms; the IPC transfer, `setData` in WKWebView/WebView2
and the reference machines are measured before release (M18). M8 carry, 50
tracks × 10,000 fixes in "all" (release): gathering 500,000 dots 25 ms;
JSON objects (the M8 shape) 197 ms and 66.6 MB against the packed buffer
33 ms and 14.0 MB. The minimum samples per cell is used (5) but edited in
Blend settings, which arrive in M14.

---

### M14 — Blend and export · **complete**

**Deliverables**

- `pe-polar::blend` (spec §12.3) with coverage output; Blend settings
  (grid, statistic defaults, `n_full`, smoothing, current-correction toggle).
- Export: Expedition `.txt`, Adrena `.pol` and `.csv`, with a preview and
  the choice of which grid to export.

**Acceptance:** golden exports; byte-identical on all five targets (CI
compares hashes).

Built as specified (spec.md §8, §9.2, §10.1, §12.2–12.4, D23).
`pe-polar::blend::blend` is the one function holding the rule (weighted
mean, `w = weight × confidence`, `min(1, n / n_full)` for track cells;
fill along TWA, then TWS, only between known values; 0° row 0 kn; optional
3×3 binomial smoothing; per-cell origin for coverage); `blend::on_grid`
reads a polar source onto the output grid with every cell read from an
excluded node empty. `pe-polar::export` checks a grid writes back (axis
collisions named, out-of-range axes, > 60 kn, nothing to write) before the
writers run. `pe-core` gains `BlendSettings::default_statistic` (serde
default, no schema bump; new tracks start with it), strict
`OutputGrid::validate` (1–512 values, two decimals, ≥ 0.01 apart, TWA
0–180, TWS 0–70) and `Command::SetBlendSettings` / `SetOutputGrid` (the
dialog's Apply is one entry; the Blend entry's switch and colour name their
own). `pe-app/src/blend.rs` assembles the blend from the derived cache for
the views (cached by the visible sources' `Arc`s, weights, grid, `n_full`
and smoothing) and **from scratch for export** (`blend::fresh`); new IPC:
`set_blend_visible`, `set_blend_colour`, `set_blend_settings`,
`export_preview`, `export_polar`; `ProjectSummary.blend`; the 2D plot's
`blend` is now a list of curves; the 3D scene sends the blend surface
(`BLEND_SOURCE`). Frontend: the Blend row (colour, show/hide, coverage,
Blend settings, Export…), `BlendSettingsDialog`, `ExportDialog` (format,
project or custom grid, preview table, native save dialog), `axes.ts` (the
grid editor's rules, the M4 carry). Carried items: M4 — export refuses
colliding axes naming both values, the grid editor refuses values closer
than 0.01, and a property test re-imports every export as the grid written
(`pe-polar/tests/blend.rs`); M6 — the plot's hover layout includes the blend
(`plotMaxBsp`); M7 — the blend surface takes the Blend entry's colour and
the 3D bounds include it. Acceptance: golden files
`pe-polar/tests/golden/blend.{txt,pol,csv}` (a fixed blend, two cells worked
by hand) and the SHA-256 of a fixed project's three exports pinned in
`pe-app/tests/export.rs`, run by every CI target that runs tests (four;
Windows ARM64 builds its tests but cannot run them on the hosted runner).

*Measured 2026-09-28 on the development machine* (Intel i9, debug build,
`perf_edit`): the blend after an edit at 20 polar sources and 20 tracks ×
10,000 samples 1.0 ms (budget 50 ms); an export from scratch (every segment
binned again) 70.5 ms; edit → every view 73, 73 and 92 ms (M13: 61, 67,
97), the summary now carrying the blend's coverage.

---

### M14b — Tracks first, weather on request · **complete**

User request, 2026-09-28: "the yellowbrick downloader is too slow. it should
first download just the tracks with no weather info. then prompt the user
to pick a track and then download the weather separately" and "do the same
for the other tracker scrapers they need to download the tracks super
fast" (D24).

**Deliverables:** measured per-phase timings for every tracker; the boat
list before the positions; concurrent requests; no weather step after any
import; Fetch weather… per track and Fetch weather for selected tracks…;
an estimate dialog that opens at once.

**Acceptance:** importing issues zero reanalysis requests; fetching weather
for a chosen track works as before; time to the boat list recorded before
and after (see the 2026-09-28 M14b entry at the top).

*Done 2026-09-28.* `TrackerClient::fetch_listed` hands the boats (no
fixes) to a callback as soon as the tracker names them; `fetch` is it with
a no-op. `Fetcher::spawn` runs a GET and its decoder on their own thread
(`Pending::wait` relays progress, returns at once on cancel), so
YellowBrick reads RaceSetup and AllPositions3 together and Geovoile its
config, tracks and reports together; every request asks for gzip and is
inflated by `flate2` (pure Rust) under the same 256 MB cap, so `pe-env`'s
client is unchanged. `pe-app::trackers::download_listed` emits
`tracker://listed` (a `TrackerEventView` with `positions: false`); preview
coordinates are rounded to 1e-4°. The dialog shows the list at once with
"…" for positions, lets boats be searched and ticked meanwhile, keeps the
ticks when the positions arrive (unticking boats with none), and imports
with **Import tracks** once they are in; rows are memoised and the preview
is two SVG paths. `Tracks.tsx` no longer opens the fetch after an import
(hint: fetch weather from the track list), adds a tick box and **Fetch
weather…** / Cancel fetch to each track row (replacing Refetch environment
in the details) and **Fetch weather for selected tracks…**; the status
reads "Weather: …". `EnvFetchDialog` shows its choices at once with
"calculating the download…" and keeps a choice made before the estimate
arrives. Tests: listing fixtures for YellowBrick and Geovoile, gzip and
spawn units, the zero-request import test, the UI listing and multi-select
tests, and the live `tracker_speed` measurements.

---

### M14c — Agent-driven UI testing (WebDriver) · **complete**

User request, 2026-09-28: "ensure that the project is set up with webkit
testing so that all agents can interact with ui elements, take screen
shots, and run ux tests agentically" (D25).

**Deliverables:** VectorEffects' optional `webdriver` feature on `pe-app`
(off by default, `tests/webdriver_optional.rs`); `tools/webdriver/` —
client, CLI, MCP server (`pe-driver` in `.mcp.json`); a UX suite
(`npm run ux`) over the real development build with a screenshot per step;
`docs/AGENT-UI-TESTING.md`.

**Acceptance:** `npm run tools:test` and `npm run ux` pass; the feature
test passes and `cargo tree -p pe-app -e normal` has no WebDriver crate;
the MCP server answers the handshake and lists its tools.

*Done 2026-09-28.* See the M14c changelog entry at the top.

---

### M14d — ORC search by individual field · **complete**

User request, 2026-09-28: "in the orc section, add some ux to search each
field independently." (D26).

**Deliverables:** per-field queries in `pe_orc::search` (`Fields`) and in
`orc_search`'s `OrcFilters`; the "Search by field" disclosure with its
count and Clear; spec §5.2; help topic and registry in en/fr/de.

**Acceptance:** pe-orc unit tests per field (accents, compact sail and
name, AND across fields, year ranges, exact-field ranking); the keystroke
timing test over per-field queries (p99 < 30 ms); UI tests; `npm run ux --
orc` types into two fields and photographs the results.

*Done 2026-09-28.* See the M14d entry at the top.

---

### M14e — Small weather downloads, per-position storage only · **complete**

User request, 2026-09-28: "The weather download should not be so many
gigabytes. do not store the entire time step of data, simply store the wind
speed and direction (and current, and wave height and direction)
interpolated to each position in the track. It should be kilobytes per
track." (D27).

**Deliverables:** block-range reads in `pe-env` (header, then only the
blosc blocks holding the positions' rows); no chunk cache on disk, an
in-memory block LRU with its Settings field and the old cache removed once;
compact per-sample storage (schema 2); the estimate at block level with
"stored in the project"; currents checked; GRIB export documented.

**Acceptance:** a fixture-backed Range test (a real ARCO chunk served by a
local server) requests only the head and the needed blocks and equals the
whole-chunk decode; live bytes before and after for a real track; no file
under the old cache after a fetch; cancel/resume, tier and provenance tests
green.

*Done 2026-09-28.* See the M14e entry at the top. `blosc::Header` parses,
lists block extents and decodes one block; `OpenVariable` reads a blosc
chunk's first 64 bytes by `HttpStore::get_range`, then runs of needed blocks
(206 → blocks, 200 → the whole chunk kept and later chunks read whole, 404
→ no data), keeping heads and compressed blocks in `memory::BlockCache`;
non-blosc stores go through `zarrs` whole. `cache.rs` is gone. Wind and
waves share one pool per batch (`dataset::read_cells_of`) and currents run
beside them. `SampleColumns` stores ids and fixes as runs, motion, the
rounded environment (`canonical::env_*`), dataset indices and the fetched
flag; `Sample::quantise_env` at ingest; schema 1 files migrate. Settings:
`weather_memory_mb` (256), `legacy_cache_notice`. Tests: `tests/ranges.rs`
(four), blosc block units, memory LRU, the estimate by hand, storage size,
migration and damaged-column tests, the legacy removal, UI and `npm run ux
-- weather` (notice, pre-flight, Settings).

---

### M15 — Compare · **complete**

**Deliverables:** Compare stage (spec §11): operand pickers, difference
surface, overlap rules, summary, 2D Δ heat map.

Built as specified (spec.md §8, §11, D28). `pe-polar::compare` compares two
grids on the same axes: cells both cover get Δ = A − B (rounded to 1e-6
kn) and Δ % of B (none where B is 0 kn); cells one covers are counted A
only / B only; the 0° row is never compared; mean and max |Δ| (with its
cell) and the signed range in both units; regions per TWS as runs of
neighbouring TWA cells over +threshold or under −threshold.
`pe-app/src/compare.rs` reads each operand (`CompareOperand`: `blend`,
`polar` of an ORC or file source, `segment` of a track, hidden sources
included) onto the output grid through the session's derived cache — a
polar source as the blend reads it (`blend::on_grid`, excluded nodes
empty), a segment with its overrides, the cached blend — and answers
`compare_polars` with one packed buffer ("PECM" v1, pinned by
`ui/src/compare/fixtures/compare-v1.bin`). Frontend: `CompareView`
replaces the placeholder (A and B translucent in their colours, the
difference surface midway between them with per-vertex colours on a
blue–orange OKLab scale with one set of stops per theme scheme, grey
hatched quads where one operand only; legend with the symmetric scale
and range; heat map TWA × TWS with an SVG hatch and a hover readout;
summary with regions), colour-and-name operand pickers, swap, Δ %
toggle, threshold in the display unit; choices are view state per
project (`compareState.ts`); the source row's Compare opens the stage
with that source as A and the blend as B. `scene3d.ts` surfaces gain
vertex colours and a hatch (`geometry3d.hatchLines`). Help topic in
en/fr/de, fifteen registry entries, every string in fr/de; the
placeholder stage component and its strings are gone. Tests:
`pe-polar` compare units (hand-worked grid, threshold equality, runs,
zero B, refusals, identity), `pe-app/tests/compare.rs` (overlap against a
segment worked by hand, overlays, blend, hidden source, refusals, nothing
written), layout and fixture tests both sides, model/state/palette tests,
the shell test, and `npm run ux -- compare` (three polar files, Compare
from a source row, picker, Δ %, swap, Paper theme; hatched cells counted
against the summary).

*Measured 2026-09-29* (Intel i9, debug build, `perf_edit`, 20 polar sources
and 20 tracks × 10,000 samples): the comparison after an edit 0.2–0.4 ms
(a source against the blend, 4.3 KB). It replaces the 3D scene while the
Compare stage is shown (one stage at a time), so it adds nothing to the
edit-to-view budget; the other views measured 85, 100 and 126 ms this
run against 73, 73 and 92 ms in M14 with the machine under other load
(the 3D scene alone 42–46 ms), which should be re-measured on a quiet
machine.

---

### M16 — Reanalysis GRIB export · **complete**

**Deliverables:** `pe-grib` ported from `ve-grib::writer` and `packing`,
regional template 3.0 grids across the antimeridian, optional waves
(discipline 10, category 0, parameters 3 and 4 for Hs and direction) and
currents; the per-track export dialog (spec §7.8).

**Acceptance:** ecCodes `grib_dump` and wgrib2 read every message with
correct times, grid and values in CI; output hashes identical across
targets.

Built as specified (spec.md §7.8, D14). `pe-grib`: `packing` ported as is
(16 bits), `writer` ported with a regional `GridSpec` (La1/Lo1 in
micro-degrees, `Lo1`/`Lo2` in 0–360 so a prime-meridian box has `Lo2` <
`Lo1`), wave height and direction (10/0/3, 10/0/14 — ERA5's `mwd` as ecCodes names it, corrected from 10/0/4 in review — at the surface) beside
wind and current, a bitmap only where a value is missing, the local-use
section "Created with PolarExplorer"; `region` (bounding box + 2° on the
0.25° grid, the shortest longitude arc, global when the margins close the
circle, poles clamped) and `times`; `file::GribFile` (messages streamed to
`<name>.tmp`, synced and renamed; dropped uncommitted, it removes the
temporary); `reader` (test-only, `testing` feature) ported with a bitmap
and `decode_all`; `examples/emit.rs` for ecCodes. `pe-env`: `Options`
gained `parts` (wind, waves, current) so an export reads only what it
writes, `Access::Urls` (stores at other addresses: tests and the UX
suite), `estimate_export` (per part, blocks of the area's rows per hour;
current boxes from a 0.1° lattice), and a wave direction on a grid node
at an archive hour is the archive's value with no trigonometry (libm's
last bit differs between platforms; invariant 5). `pe-app/src/grib.rs`:
the plan from a track's fixes, the preview, the export (the current tile
by tile over every time into a spool beside the export, then wind and
waves up to six hours a read, each hour's messages written as they
arrive), one job at a time with `grib://progress` and cancel; the
`webdriver`-only `PE_DRIVER_REANALYSIS` loopback seam. Frontend:
`GribExportDialog` (area, times, per-part download, file size, Save…,
progress, Cancel export), the track's button enabled, four registry
entries, the environment help topic extended in en/fr/de, every string in
fr/de. Tests: region and writer units (hand-assembled section 3, sign
magnitude, calendar dates, parameter tables), `tests/roundtrip.rs` (both
seams, bitmap, constant and all-missing fields through the reader),
`pe-env` parts and node-direction tests, `pe-app/tests/grib_export.rs`
(archives served over HTTP with `Range`: the real `wb2-crop` blosc chunk
for wind — numcodecs' 9.382978 m/s at 50N 5W — and synthetic waves and
GlobCurrent; golden file `crates/pe-grib/tests/golden/reanalysis.grib2`
pinned by SHA-256; wind-only equals the golden's wind; cancel leaves the
path and no temporary; the job through a project), UX flow
`09-grib-export.test.mjs`. CI job `grib` runs ecCodes (`grib_ls`,
`grib_dump`, `grib_count`, every value by `grib_get_data` in
`tools/check-grib.sh`) on the emitted sample and the golden file; wgrib2
is not packaged for the CI image and is not run (ecCodes is the
independent decoder, as in VectorEffects).

---

### M17 — Translation and help completion · **complete**

**Deliverables:** French and German reviewed by a sailor who speaks each
language; help topics complete; every control in the search.

---

### M18 — Release · **complete**

**Deliverables:** signing and notarisation (macOS), Windows signing, bundles
for all five targets, About with ORC catalogue provenance and data
attributions (ECMWF/Copernicus ERA5, WeatherBench2, Copernicus Marine,
jieter/orc-data MIT), user guide.

*Completed 2026-09-30 under the signing-ready preflight scope.* About and
searchable Help credit all data providers in en/fr/de. `docs/USER-GUIDE.md`,
`DATA-SOURCES.md` and the ORC MIT notice are bundled resources;
`docs/RELEASING.md` documents artifact-only builds, tagging, optional credentials
and installation checks. The release workflow validates partial Apple secrets,
supports Windows PFX signing with SHA-256 and RFC 3161 timestamps, checks the
resulting signatures, and includes a documentation archive in release checksums.
Missing, empty or mixed-version installers prevent publication. No signing
credentials is a supported test-build mode: ad-hoc macOS, unsigned Windows.

Local Intel Mac validation: app and DMG built in release mode; app signature,
DMG integrity and bundled documents verified. fmt, clippy, 612 Rust tests
(20 ignored), 340 UI tests, 4 serial performance tests, 10 driver tests,
8 release-tool tests and the offline check pass. The Help UX flow passes in
all three languages; its credits screenshots were opened and reviewed.
The repository is [jweisbaum/PolarExplorer](https://github.com/jweisbaum/PolarExplorer)
(named PolarEffects, with `build/v1` as its default branch, until 2026-10-04;
`main` since). At commit `0c2932d`,
[CI passed all ten jobs](https://github.com/jweisbaum/PolarEffects/actions/runs/36720234361)
and [all five platform bundles passed](https://github.com/jweisbaum/PolarEffects/actions/runs/36720247704)
in artifact-only mode. This includes the Windows PFX fixture, Windows application
manifests, cross-compilation of ARM64 tests and independent ecCodes validation.
The first Windows run exposed a Unix-only absolute-path fixture; it now uses a
native temporary path and the full Windows suite passes.

Downloaded all eight installers: two Mac DMGs, Windows x64 MSI and NSIS,
Windows ARM64 NSIS, and Linux AppImage, Debian and RPM. The complete-set guard
and independent SHA-256 verification pass. Both downloaded Mac apps have the
expected architecture, valid ad-hoc signatures and exact documentation resources;
both DMGs pass `hdiutil verify`. Debian and RPM contain the expected guide and
notices. Local verified payloads and `SHA256SUMS` are in
`target/release-downloads/0c2932d/verified/`.

Actual issuer signing/notarization awaits certificates (the accepted preflight
scope); Windows packages are unsigned and Mac builds are ad-hoc signed. No
release tag was created and no GitHub release was published. Native-sailor
review and reference-device installation/launch/performance checks remain open.

---

### M19 — Track filtering and polar analysis · **complete** (2026-09-30)

Requested 2026-09-30, after the M18 artifact builds. Preserve imported
sources and store new edits as undoable overlays; old projects keep their
existing behaviour.

- [x] Track filters: unknown waves/current, tack/gybe and stop exclusion
  windows, wave direction relative to COG, UTC timestamp filtering with
  selectable minute/second intervals.
- [x] Global filters in the 3D view, shown only with tracks, applied after
  individual filters; no global start/end dates.
- [x] Remove the GRIB export option and its help/search entries.
- [x] Colour by wave period, wave angle and wave/wind angle; UTC date/time
  range for time colouring.
- [x] Numeric boat measurement search and paginated polar results.
- [x] ORR polar support and a user-started Settings scraper, deduplicated;
  RegattaMan’s supplied valid list (300 certificates, 600 polar variants).
- [x] Independent port/starboard in the blend, corrections and export, with
  asymmetric 360° mode in 2D/3D; linear/monotone spline interpolation;
  TWA steps 1/2/5/10° and TWS steps 1/2/5/10 kn.
- [x] Manual blend corrections as reversible cell overlays.
- [x] Ordered priority filter groups with fallback for sparsely sampled
  TWA/TWS cells (first group meeting the pooled minimum, as confirmed).
- [x] Migration/round-trip/undo, numerical and UI checks, translations,
  help, documentation and actual-app visual verification.

Validation on the Intel Mac (2026-09-30): the actual-app
`09-polar-analysis` walkthrough passes with eight inspected screenshots,
covering ORR measurement search and pagination, duplicate prevention,
full-circle plots, a port-only correction, track/global filters, a two-minute
interval, UTC colour legends and ordered priorities. French/German strings
and help controls pass their coverage checks. Production UI build, TypeScript,
formatting and strict workspace clippy pass. The final workspace run passes
631 tests with no failures; the default suite skips 21 live/performance tests,
with the three edit/filter performance tests run separately below. The UI suite
and focused reruns pass, IPC bindings are regenerated, and the offline check
passes against the production bundle.

Release-mode measurements: 200,000 points take 8.6 ms for hard filters and
25.1 ms for two priority groups; edits update the Rust data for all views in
25–36 ms (100 ms budget). Gathering 500,000 2D dots takes 24.6 ms and packing
23.7 ms. All three Rust performance tests and all four UI performance tests
pass; two 512×512 surfaces update in 22–71 ms, and a 444-boat list takes
317 ms to display, 23 ms to tick and 39 ms to search.

### M19a — Complete ORR certificate capture · **complete** (2026-09-30)

Requested 2026-09-30: extend the polar-only ORR scrape to retain all public
certificate data and ratings. Preserve named list fields, original metric
measurements and enum labels, ownership/comments, drawing parameters, all
rating JSON blocks, and the original speed/time-allowance tables. Keep stable
SKU/variant deduplication and immutable imported copies. Schema 4 adds optional
details while keeping older projects readable. Refresh the bundled catalogue,
check full-catalogue completeness, and verify parser fixtures, metadata
round-trip/undo, legacy caches, search, translations and the app workflow.

All 300 certificates were refreshed successfully: 600 unique SKU/variant keys,
77,097 fields and 18,360 rating entries. Each certificate retains all 21 list
columns, five rating groups and four original tables. The original 600 speed
grids compare equal to the earlier snapshot. The maintainer refresh refuses to
replace a snapshot if any certificate fails, and the completeness integration
test checks every bundled record.

Validation: 636 workspace tests pass (21 intentionally skipped live/performance
tests), strict workspace clippy and formatting pass, bindings regenerate,
26 translation/help tests and four UI performance tests pass. The production
UI build and offline check pass. The 42-second actual-app walkthrough passes
with eight screenshots; ORR builder/build-year search and the updated Settings
description were inspected in the captured images.

### M19b — Project navigation · **complete** (2026-09-30)

3D becomes the first and default stage. Map follows it only when imported
tracks exist; removing the final track from Map returns to 3D. Full-size 2D
plots remain available without tracks. Move the Project menu between Search
and Settings, with its popup aligned to stay within the window. Update the
help, translations and existing UI walkthroughs.

Validation: all 342 UI tests pass across the full run and the focused rerun
of the comparison test updated for the new default. The production UI build,
TypeScript checks and offline check pass. Five actual-app walkthroughs pass
with 25 screenshots, covering the default view, menu placement and bounds,
track import/removal/undo, full-size 2D plots without tracks, help navigation
in three languages and French settings. The changed layouts were inspected.

### M19c — Wave display ranges and retained boat searches · **complete** (2026-09-30)

- [x] Add immediate lower/upper 3D display sliders for wave height, angle off
  the bow and period, intersected with existing visibility and analysis filters.
- [x] Preserve metre/feet conversion, inclusive boundaries, sample identities
  and selection safety; leave project data, blend and surfaces unchanged.
- [x] Add a visible sample count, reset, disabled states for absent wave data,
  French/German translations and help entries.
- [x] Keep boat search query, page and remaining results after importing a track;
  remove successfully imported rows and update the count without changing
  catalogue offsets. Failed imports remain available to retry.
- [x] Complete unit, performance and real desktop checks, including consecutive
  boat imports and wave sliders over the recorded weather fixture.

Validation: 651 Rust tests, 346 UI tests and all five performance checks pass;
formatting, clippy, TypeScript, production UI build and offline checks pass.
For 200k samples, warmed dot-building plus scene updates take 36–47 ms with
all samples and 16–27 ms with the three ranges active. Two desktop walkthroughs
pass with nine screenshots, covering consecutive imports, all six slider bounds,
actual canvas changes, filter composition and reset. The changed layouts were
visually inspected.

**Follow-up — search imports and asymmetric labels (complete).**
Successfully imported boat rows now leave the current search, while its query,
page and remaining rows stay available. Both polar views show 0–180° tick
labels on each half; full-circle geometry, source values and editable grid
identities are preserved. Validation: the full 346-test UI suite and the updated
44 geometry tests pass, as do the 651 Rust tests, five performance checks,
formatting, clippy, TypeScript, production UI build and offline checks.
Database-library and polar-analysis desktop walkthroughs pass; the remaining
search rows and mirrored labels in both plots were inspected in screenshots.

**Follow-up — select all imported tracks (complete).**
Add Select all beside the batch weather button above the imported tracks list.
It ticks every imported track, using the existing download estimate and job
queue; running/queued tracks are omitted from the request. The control is
translated and searchable in Help.

### M20 — PolarExplorer, 0–1 weights, time of day, blend tooltip, measure · **complete** (2026-10-01)

Requested by the user 2026-10-01; decisions in D30.

- [x] Rename the application to PolarExplorer in everything a person reads
  (window, menus, start screen, help, three languages, user agent, GRIB
  provenance, documents, release asset names). Identifiers and data paths
  keep the old name, so nothing on disk moves.
- [x] Source weights run 0–1 (schema 7); a stored weight above 1 loads as 1,
  in every boat tab.
- [x] Compare reads a polar source by the project's interpolation rule, as
  the blend does; the spec's stale "bilinear" wording corrected.
- [x] Colour dots by the time of day (night, morning, afternoon, evening by
  local mean solar time) in the 3D view and on the 2D plot, with a legend
  and the band in a dot's tooltip. Packet layouts: 3D scene 4, 2D dots 2.
- [x] Hovering the blend, in 3D and on the plot, names the cell and each
  source behind it with its speed and share of the weight (`blend_cell`,
  `pe_polar::blend::contributions`).
- [x] A Measure tool on the 2D plot: every curve at the pointer's wind
  angle against the one pointed at, and from a pinned point to the pointer.
- [x] The linker failure reported with this request was a build made with
  incremental compilation before the morning's `incremental = false`; every
  configuration links on the current tree, and the dead 6.6 GB incremental
  cache is removed.

Validation: 701 native tests and 397 UI tests pass, with the five serial
performance checks, formatting, workspace clippy, TypeScript and the offline
check. The new desktop walkthrough `15-measure-daytime-blend` passes and its
four screenshots were inspected (the time-of-day legend and green afternoon
dots in 3D and on the plot, the blend cell's tooltip naming its source, the
Measure readout with a pinned point). The whole desktop suite: 15 of 16 pass.
Two stale expectations from earlier changes were corrected (`09`: the
asymmetric switch is no longer in the title bar; `10`: the German heading is
"ORC-/ORR-Polaren"). **Open, not from this milestone:** `01-new-project`
fails at "undo restores Map without reopening it" — undoing the removal of
the last track selects the Map stage again; the stage code is untouched here.
The rename's search-and-replace also rewrote the name inside the binary GRIB
golden file; it was regenerated from the writer and its pinned hash updated.

### M21 — MCP service · **complete** (2026-10-01)

Design approved 2026-10-01 (D29,
`docs/superpowers/specs/2026-10-01-mcp-service-design.md`, whose §10 lists
where the implementation departed from it); implementation plan in
`docs/superpowers/plans/2026-10-01-mcp-service.md`.

- [x] The listener: `rmcp` over Streamable HTTP on `127.0.0.1`, a bearer
  token checked before any MCP handling, `Host` and `Origin` held to the
  loopback names, nothing created while the setting is off.
- [x] Settings → MCP service: the switch, the port, the token and its
  rotation, the address, who is connected, and a translated line when the
  port cannot be opened.
- [x] 51 tools in eleven groups, each calling the interface's own command;
  `invoke` for the rest, with its exclusions held to the registered commands
  by a test.
- [x] The interface follows: `document://changed`, a boat's stage and tab,
  the selection, a screenshot of the stage, and the status bar's **MCP**
  badge — also for a client that keeps no session, and cleared at once when
  the service is switched off.
- [x] *Add to Claude Code*, *Add to Codex*, *Add to Claude Desktop* (an
  extension holding no secret), and text for other clients. No ChatGPT.
- [x] Invariant 4's inbound exception recorded (CLAUDE.md, spec §1.5,
  §3.7); `check-offline.sh` admits in `pe-app` only the service's server
  crates with their server features.

**Validation (2026-10-01).** 783 Rust tests (27 ignored: live and
platform-only), of which 58 drive the service over HTTP against a mock
application and one runs Claude Desktop's bridge under Node; 420 UI tests;
the 5 performance checks; formatting, workspace clippy, TypeScript, the
offline check (its `pe-app` rule checked against seven mutated manifests)
and the driver tooling's 10 tests. The desktop walkthrough
`16-mcp-follow` connects the MCP SDK's own client to the real application
and passes; its four screenshots were inspected (Settings with the service
on, the interface after the client's import and weight, a second boat on
Compare, the same boat removed). The whole desktop suite: 16 of 17 pass.

One fresh-context review of the whole milestone found no critical defect
and six important ones, all fixed with a test that failed first: a client
on the 2026-07-28 protocol was never counted (so no badge); a misspelt key
in a patch was dropped and the tool answered success; `project_save` and
`export_polar` replaced existing files without being told to; the `Host`
and `Origin` tests could not fail; a boat removed or restored by a client
left the boat tabs inconsistent; and `view_stage` for a named boat set the
stage of whichever boat was on show. With them: weather cancel and wait
are per boat, the offline check's `pe-app` rule is an exact feature list,
switching off clears the count at once, the bind error is translated, and
the ORR catalogue scrape is the tool `orr_refresh`, as the design's consent
line said. The minors left are in the design's §10 and below.

**Open.** `01-new-project` still fails at "undo restores Map without
reopening it" (as under M20; `BoatWorkspace` keeps the requested stage at
"map" while the last track is gone, so undoing the removal shows the map
again). Not narrowed: `track_files_inspect` reads the first rows of any
text file it is pointed at, as the interface's "All files" dialog can.
Deferred minors from the review: an unknown boat id is refused as "No
project is open."; `tracker_event` does not stop when its request is
cancelled; enabling and disabling at the same instant is not serialised; a
refused write on the start screen closes an open New Project dialog; the
Codex and settings files are written before they are made owner-only, a
symlinked `config.toml` is replaced by a file, and `claude`'s own error
text is shown unscrubbed; the accept loop does not back off on a
persistent accept error; UNC paths, device files and `invoke`'s path
arguments are not checked; three test gaps (a dropped connection rather
than a closed session, one assertion that cannot fail, the nothing-open
refusal checked for six tools rather than all).

### M22 — ORC scraping and catalogue schedules · **complete** (2026-10-02)

Requested by the user 2026-10-02; decisions in D31.

- [x] `pe-trackers::orc`: ORC's own service, `data.orc.org` (`DownRMS`),
  one JSON document per country for the current VPP year, read the way the
  embedded catalogue's records were, plus each certificate's reference
  number. A recorded answer (Norway, three certificates) is the fixture.
- [x] Scraped certificates are kept beside the embedded catalogue in its
  own format and merged into one catalogue, each certificate once. A
  certificate is its reference number; one ORC no longer lists is
  withdrawn in place; a scraped twin of an embedded entry takes its place;
  catalogue ids do not move.
- [x] Settings → ORC polars: start, cancel, progress by country, what was
  added, updated, removed and left out. The ORC panel refreshes when a
  scrape finishes, shows a downloaded certificate's number, and its footer
  credits what was downloaded to ORC.
- [x] *Download automatically* on both catalogues: manually only, on
  startup, on shutdown; at most once a day. On shutdown quitting waits and
  says so; quitting again cancels and quits.
- [x] `orc_refresh` for the MCP service; `data.orc.org` on the allow-list
  and in `check-offline.sh`; invariant 4, spec §5.4 and the notices amended.

**Validation (2026-10-02).** 800 Rust tests (28 ignored: live and
platform-only); 424 UI tests; the 5 performance checks; formatting,
workspace clippy, TypeScript, the offline check and the driver tooling's 10
tests. Live, against ORC's service: the scraper's own test read 7,730
certificates from all 33 countries in 84 seconds with none left out; the
desktop walkthrough `17-catalogue-scrape` downloaded them through the real
application twice (the second time adding, updating and removing nothing)
and its six screenshots were inspected; `18-catalogue-shutdown` showed a
quit waiting for a shutdown download and a second quit cancelling it with
nothing stored; `19-catalogue-startup` showed a startup download starting
by itself and being cancelled. The same three walkthroughs run offline in
the default suite (Settings and schedules; quitting quits when nothing is
scheduled; a startup download is skipped when the catalogue is a day
fresh). The whole desktop suite: 19 of 20 pass; `01-new-project` fails as
under M20.

**What the live runs changed.** The first scrape through the real
application stored 7,716 of 7,730 certificates: the rule then was "the same
boat's certificate of the same year replaces the stored one", and 14 boats
hold two valid certificates in 2026 (a crewed and a double-handed one, an
offshore variant, a second sail configuration), most with different
polars. Scraping again "updated" 26. The rule is now the reference number
alone, with what ORC no longer lists withdrawn, and a second live scrape
reports nothing added, updated or removed. The size was also over-estimated
before measuring (100–150 MB): it is about 60 MB, 84 seconds on this
connection.

**Not done.** ORR's schedule shares the ORC schedule's code and its tests;
its scrape was not run live in this milestone.

### M23 — Fleet controls and the linked blend tooltip · **complete** (2026-10-02)

Requested by the user 2026-10-02.

- [x] The layout switch (single, split, four-way) is three icons in the top
  bar beside Add Polar, each named in its tooltip and to a screen reader.
- [x] A tab's controls call it a polar: **Add Polar**, new tabs named
  *Polar N*, and **Undo Delete Polar**; French and German with them. The
  boat tabs are drawn as tabs, each with a **×** that closes it after
  asking (it replaced the Delete Polar button and its row); the last tab's
  × is disabled.
- [x] **Export all…** sits at the bottom right, beside the version.
- [x] In split and four-way view, hovering the blend's surface shows every
  pane's own blend cell at the same wind, each tooltip beside its cell.
- [x] Split and four-way view have no left and right panel toggles.

**Validation.** `FleetWorkspace.test.tsx` (10) and `synchronization.test.ts`
cover the icons, the names, the tab ×'s confirmation and the linked blend
hover; the driver's `13-boat-tabs` (7 shots) and `14-tracker-project`
exercise them in the app. Whole suite at the end of M24: 451 UI tests, 811
Rust tests, 19 of 21 driver tests (`01-new-project`'s undo-restores-Map
check is the known pre-existing failure; `09-polar-analysis` passed on its
own and failed once in the full run at an unrelated Add click).

### M24 — Split Wave Angle · **complete** (2026-10-02)

Requested by the user 2026-10-02; decisions in D32. Spec §10.5.

- [x] **Split Wave Angle**, beside the Box tool, in single view only: the 3D
  view drawn once per wave direction. A slider of 4, 8, 16, 18, 24 or 36
  directions; **From** or **To**.
- [x] Directions are centred on multiples of 360/*N* from the bow, so none
  begins or ends at 0°.
- [x] Each copy holds only its direction's samples, after every filter and
  the wave range filters, with its count and a big arrow against a boat.
  Samples with no wave direction are in no copy and are counted in the
  toolbar.
- [x] One canvas and one camera, a viewport per copy: rotation, pan, zoom and
  presets are shared; a hovered dot is marked at the same wind in the other
  copies, and a hovered blend cell in all of them.
- [x] The scene packet is version 5, with the wave bearing clockwise from the
  bow (0–360) beside the folded wave angle.
- [x] **Each copy has its own blend** (asked later the same day): every
  track binned again from that copy's samples, the other sources as they
  are, through the ordinary blend rule; cached beside the blend
  (`Derivations::split_blends`), sent as one "PE3W" packet
  (`polar_scene_split`), and a hovered cell read from that copy's blend
  (`blend_cell_split`). The whole blend and export are untouched.
- [x] An arc round each copy's boat shows the directions it holds.

**Validation.** Hand-computed: the analysis fixture's 120 samples
(heading 90° or 270°, waves from 3k°, a 0.2 kn current turning the heading
through the water 1.8°) fall 22/23/11/0/0/0/11/23 into eight directions and
44/24/0/22 into four, which the app shows and `20-wave-split` asserts; a
track of ten samples half on the bow, half on the beam blends 7.036 kn and
7.086 kn in those copies against 7.081 kn whole (`wave_split.rs`), and over
IPC 54.36/7 and 54.86/7 kn with a polar file beside it
(`tests/wave_split.rs`), a mutation of the direction filter failing both.
`waveSplit.test.ts` (10), `scene3d.test.ts` (11, the grid of copies and the
copy-bound surfaces), `scenePacket.test.ts` (the "PE3W" reader) and
`PolarView.test.tsx` (23) cover the interface; the driver's
`20-wave-split` (7 shots, looked at) walks 8, 4, To, a narrowed wave range,
the linked dot hover, each copy's own blend (6.74 kn in the 45° copy
against 7.0 kn where no sample is) and 36 copies clear of every panel.
Thirty-six blends of the fixture arrive well within a second.

### M25 — Filters consolidated, both catalogues, the 2D stage · **complete** (2026-10-03)

Requested by the user 2026-10-02; decisions in D33.

- [x] Catalogue search and the track library search load the next page as
  the list is scrolled to its end; no page buttons.
- [x] The ORC/ORR picker is gone: one search over both, each hit badged.
- [x] Measurements under Search by field are folded until asked for; each
  row is the measurement, then Min and Max under those headings.
- [x] Include Stokes drift is gone (the setting loads off).
- [x] Track filters grouped as Boat speed (BSP, VMG), Boat heading (heading,
  COG, direction change, Remove tacks and gybes) and Wind, waves and current
  (TWS, TWD, TWA, …). Heading, COG and TWD are compass sectors. Each group
  offers provided-or-derived only where the track provides that quantity;
  the separate Heading and speed section is gone.
- [x] Gone: tack/gybe window, stop speed and window, timestamp interval and
  unit, given-or-derived origin filters. Remove tacks and gybes leaves out
  the sample on either side of each change of tack.
- [x] "Weight" beside each weight slider.
- [x] 3D: the camera looks at the origin (2026-10-03), drawn in the middle
  of what the panels leave free, all of the polar in sight.
- [x] 2D is a stage (3D, 2D, Compare, Map — Map moved last 2026-10-03), its
  0° axis down the middle and its controls centred;
  the right panel holds the sources only.
- [x] Split and four-way: a linked pane's hover tooltip sits beside the
  matching dot, not in the corner.

**Validation.** Hand-computed filter cases in `pe-tracks` (heading, COG and
TWD sectors wrapping north; VMG −5.909 kn at TWA 170° and 6 kn; a tack takes
out exactly the two samples beside it, a gybe likewise, nothing across a
gap); `derive.rs` prefers heading and speed apart; migration 7 → 8 drops the
six filters, splits `prefer` and turns Stokes drift off in the project and
every boat tab, the bytes then saving stably. In the real app (driver):
`09-polar-analysis` searches both catalogues, sees both badges, scrolls to
load the next page and adds the ORR hit once; `12-live-analysis` checks the
supplied-wind fixture by hand — one tack (10 of 12 used), VMG ≥ 0 keeps 11,
VMG ≤ −1 keeps 1, COG 80–100° keeps 6 — and that a lone sector bound is held
rather than refused; `02`/`15` use the 2D stage with the fan right of the
centre line between the panels; `20-wave-split` checks the first-load 3D
framing (every dot inside the free area, centred within 15 %). Whole suite:
809 Rust, 454 UI and 5 perf tests pass; the driver suite passes but for the
long-standing `01-new-project` undo-restores-Map check, and `10-languages`
passed on a rerun after a focus-timing flake.

### M26 — Six more languages · **complete** (2026-10-03)

Requested by the user 2026-10-03: Spanish, Italian, Dutch, Chinese, Japanese
and Arabic, beside English, French and German.

- [x] Every interface catalogue (17 areas, 1,755 strings) and the whole help
  reference in each, with a glossary per language (`GLOSSARY.<lang>.md`).
  Machine-drafted; each glossary flags (⚑) the terms a native sailor should
  check.
- [x] The native menu and the About credits in each language (`menu.rs`).
- [x] Arabic lays the page out right to left; sliders and plotted numbers
  stay left to right.
- [x] The language picker and the help's Language entry name all nine.

**Validation.** The coverage tests hold each catalogue to every string, its
placeholders and nothing unused; the topics test holds each help reference
to the English pages; the features test finds every control by its label in
each language; the menu test checks every label differs from English but
for words each platform shares (Zoom, File, Help). The app was looked at in
Chinese, Japanese and Arabic (3D, 2D, filters, Settings).

### M27 — Whole races over MCP · **complete** (2026-10-03)

Asked 2026-10-03: the agent should load a race's boats with their polars
and tracks in one go, and use its own knowledge of the boats to find more.

- [x] `race_project`: the interface's Open project from tracker… as one
  tool — a tab per boat with its track, certificates and library tracks.
- [x] `library_search` and `library_import` over the user's track library.
- [x] Geovoile races build too, in the dialog and in `race_project` (asked
  2026-10-03): from the leg the link shows, matched mostly by the agent's
  own search, since Geovoile gives only names and sail numbers.
- [x] The guide's "A whole race" recipe and "Working through a job" rule;
  the short instructions name both and still fit 2,048 characters.

**Validation.** `tests/mcp.rs` builds a race over HTTP from a recorded event
in the session's tracker cache (nothing reaches a tracker): two tabs, the
matched boat's certificate and track, the unmatched boat empty; refusals for
Geovoile, an unknown match mode and unsaved work; the library answers
`downloaded: false` and refuses an import in words when not set up. The
guide test holds every tool it names to the tools that exist.

## 3. Testing strategy

- **Unit**: geodesy, derivation, interpolation, binning, blending, parsers,
  decoders, GRIB packing.
- **Fixtures**: recorded vendor responses and zarr chunks in
  `tests/fixtures/`, small enough to commit (crop chunks in a test helper
  where needed; document how each was recorded).
- **Golden files**: polar exports, GRIB bytes, `.wpsproj` bytes.
- **Property tests**: document round-trip, undo/redo inverses, blend
  invariants (a single visible source blends to itself; weights scale-free).
- **Live tests**: behind `PE_TEST_LIVE=1` and `--ignored`, never in default
  CI; a weekly scheduled workflow runs them to catch vendor changes.
- **UI**: Vitest for logic, i18n coverage, feature registry.

## 4. Risk register

| Risk | Impact | Mitigation |
|---|---|---|
| Reanalysis download volume (global chunk per hour per variable) | Slow first import; large disk use | Shared chunk cache, size estimate before start, sparse block/chunk reads, M3 measurements |
| Vendors change formats (Geovoile especially) | Imports break | Plausibility checks, clear "unsupported" errors, weekly live tests |
| Scraping terms of use | Legal or blocking | Only user-initiated single-event fetches, polite concurrency, no credentials (D5); ask before adding trackers |
| WeatherBench2 frozen at 2023-01-10 | No wind for recent races from WB2 | ARCO-ERA5 fallback (D12, Q2) |
| Coarse tides before 2020-11 outside NW Europe/IBI | Current correction weaker for older races (GlobCurrent's 0.25° FES2022 tide) | Regional tidal reanalyses first; Q1, Q7 (settled: GlobCurrent includes the tide) |
| Windows ARM64 toolchain | Build failures | CI job from M0; no C deps (D6) |
| 3D performance with large tracks | Janky editing | Instanced points, binary IPC, M3 spike |
| ORC schema drift across years | Missing boats | Builder reports dropped records |

## 5. Decisions log

| # | Decision | Rationale |
|---|---|---|
| D1 | Copy VectorEffects' architecture: Tauri 2, Rust domain, React view, ts-rs bindings, one stylesheet with theme tokens | Requested; proven in the sibling app. Settled with the user 2026-09-27 |
| D2 | Back end is Rust only; no headless browser. Tracker formats decoded in Rust | Requested; the `tracker-index` scrapers use puppeteer, which cannot ship |
| D3 | ORC catalogue built from jieter/orc-data per-boat files and embedded | `ALL2025.json` is Python repr with 34 boats; per-boat files hold ~18k. Settled with the user 2026-09-27 |
| D4 | One tracker dialog flow: URL → download all boats → pick → import | Requested behaviour, identical across trackers |
| D5 | Direct track imports use public JSON/BIN endpoints. Catalogue discovery may use explicitly authorized local YellowBrick credentials and associate only products listed as free. Never bundle credentials. | Updated 2026-09-30 at the user's request to use the SYRF scraper credentials for catalogue code lookup |
| D6 | No C/C++ dependencies beyond Tauri's own; rustls with ring | Windows ARM64 target and "Rust only" requirement |
| D7 | Speeds stored in knots, directions in degrees | Every polar format and ORC is knots; avoids conversion noise in exports |
| D8 | Default theme "Harbour" (blue); same structure as VectorEffects | Requested "same styling, slightly different colours" |
| D9 | Save guard copied: Save / Don't save / Cancel; back end refuses without `discard_unsaved` | Requested; no path can drop work silently |
| D10 | i18n copied (English as key, coverage tests); v1 = en, fr, de | Requested |
| D11 | `.wpsproj` = ZIP of canonical JSON + per-track entries, fixed timestamps | Diffable, deterministic, recoverable |
| D12 | Wind from WeatherBench2; waves from ARCO-ERA5; wind after 2023-01-10 from ARCO-ERA5 | WB2 wind requested by the user 2026-09-27; WB2 has no waves and ends 2023-01-10. The post-2023 fallback is proposed, see Q2 |
| D13 | Correct boat speed and wind for current; store raw and corrected | Polars are water-relative; tracks are ground-relative |
| D14 | GRIB2 written natively from a fixed in-code message template, ported from `ve-grib` | Requested "same approach as VectorEffects"; VectorEffects keeps the template in code, not in a file |
| D15 | Visibility is the include/exclude switch for the blend | One concept instead of two; requested "hidden or visible to include or exclude" |
| D16 | three.js, bundled locally, for 3D | VectorEffects has no 3D; three.js is mature and works in every Tauri webview |
| D17 | Edits stored as overlays (cell overrides, exclusions) | Invariant 1; reversible, auditable |
| D18 | Track segment cell statistic defaults to the 90th percentile, minimum 5 samples | Polars describe good sailing; the mean undershoots |
| D19 | Reanalysis is hourly only, with no coarser option or size-based fallback (updated 2026-10-07) | Requested by the user: remove the 3-hourly fetch option and always fetch hourly. Supersedes the original D19/D27 interval recommendation. Existing saved weather retains its provenance. |
| D20 | Current tiers: regional tidal reanalysis → global merged (uo + utide, 2020-11+) → GlobCurrent (geostrophic + Ekman + FES2022 tide, 1993+; its 202411 metadata, checked 2026-09-28, Q7) | Only anonymous sources; GlobCurrent (FES2022) gives tides globally from 1993, not only NW Europe/IBI |
| D21 | 2D polar plot (M6): "All" draws one curve per visible source per wind speed that source's grid has; curves are read at each source's own TWA points; the full-size view is a Map-stage overlay toggled by the shell, closed by its own button, Escape or a stage switch | Spec §9.2 named the slider's "all" state and the full-size overlay without saying what either draws or how the overlay opens and closes |
| D23 | Blend and export (M14): an output cell read from an excluded node of a polar source is empty for that source (not read across it); the fill steps interpolate only between known values and the 0° row takes no part in them (set to 0 kn last, "filled" unless a source had it); sources are summed in id order and cells rounded to 1e-6 kn; the Blend settings dialog applies as one undo entry, the Blend entry's switch and colour as their own; the grid editor takes two decimals at most (≥ 0.01 apart); a custom export grid is the project-grid blend resampled; export refuses axis collisions, > 60 kn and an empty blend, naming the values | Spec §12.3 named the rule and the fill order without saying how exclusions reach a resampled cell, whether the 0° row anchors the fill, the summation order or how settings are undone; §12.2 and the M4 carry left the grid editor's precision open |
| D23a | M14 review round 1 (controller rulings): a track cell with an override counts with confidence 1 whatever its sample count, 0 included; the 0° row is no evidence — it counts in no coverage, and a blend with no value off it is empty and refused by export; a custom export grid does not anchor on the 0° row (output 0° row 0 kn); new projects' blend colour is `#e0457b` and a colour lost on the background is outlined; out-of-range sample counts are clamped on load; the blend cache keys sources by id | Review of M14 |
| D24 | Tracker and file imports download tracks only, never weather. The boat list shows as soon as the tracker names the boats (YellowBrick's RaceSetup, Geovoile's config), while the positions download; independent requests run at once and are asked for gzipped. Weather is a separate step the user starts per track (Fetch weather…) or for ticked tracks (Fetch weather for selected tracks…); weather starts immediately without an estimate dialog (updated at the user’s request on 2026-10-07) | Settled with the user 2026-09-28: "the yellowbrick downloader is too slow. it should first download just the tracks with no weather info. then prompt the user to pick a track and then download the weather separately"; "do the same for the other tracker scrapers they need to download the tracks super fast". Supersedes "import starts the fetch" in spec §7.5 (M9) and the post-import pre-flight of M10 |
| D25 | Agent-driven UI testing: `pe-app`'s optional `webdriver` feature compiles in `tauri-plugin-webdriver-automation`, an endpoint on `127.0.0.1` at a random port that drives the whole interface. Off by default, never in `npm run build`; `tests/webdriver_optional.rs` holds the manifest to that (check:offline cannot see a listener). Under the feature only: `PE_AUTOMATION_ROOT` redirects the settings, recent list, autosave and cache to a driver's temporary directory, and `PE_DRIVER_YELLOWBRICK` (a `http://127.0.0.1:<port>` origin only) serves the YellowBrick dialog from a local fixture server. In development builds only (`import.meta.env.DEV`): queued answers for native file dialogs, `__peOpen`, and a synchronous redraw on the WebGL canvases for screenshots. The suite runs the dev build (StrictMode) | Settled with the user 2026-09-28: "ensure that the project is set up with webkit testing so that all agents can interact with ui elements, take screen shots, and run ux tests agentically". Copied from VectorEffects (M71); the inbound exception to invariant 4 is worded as VectorEffects words its invariant 5 |
| D26 | ORC search by field: under the all-fields box, a "Search by field" disclosure (folded by default, remembered per user in `localStorage`) with boat name, sail number, country, model / type, builder, designer, year built from–to (the former year and country filters, moved in) and certificate year. Every filled field and the main box must match; a field's words must start words of that field only (same folding and compact forms); the certificate year matches from its start. A field equal to its whole field ranks above every other tier. The field boxes are not saved; Clear empties them and keeps the main box | Settled with the user 2026-09-28: "in the orc section, add some ux to search each field independently." |
| D27 | Weather downloads read only the blosc blocks of each archive chunk that hold a track's rows (HTTP Range: a 64-byte head, then the blocks), for ERA5 and the current geoChunks alike; Open Data keeps downloads only in an in-memory LRU of blocks for the session (Whirlwind uses the bounded disk cache requested 2026-10-07) (256 MB default, 16–4096 MB), and an earlier version's chunk cache is removed once with a status-line notice. A project stores per sample only its non-derived values by column, the environment rounded to 0.01 kn, 0.1° and 0.01 m (schema 2; derived angles recomputed on load). The original D19 size-based interval recommendation was superseded on 2026-10-07: sampling is always hourly | Settled with the user 2026-09-28: "The weather download should not be so many gigabytes. do not store the entire time step of data, simply store the wind speed and direction (and current, and wave height and direction) interpolated to each position in the track. It should be kilobytes per track." No anonymous point-chunked ERA5 exists; the blocks are the finest unit the archives allow (≈ 1/8 of an ERA5 field, 1/5–1/7 of a geoChunk) |
| D28 | Compare (M15, controller rulings): computed in Rust from the derived cache, operands read on the output grid as the other views read them (a polar source as the blend reads it, excluded nodes empty; a segment with its overrides; the cached blend; hidden sources allowed), one binary packet; Δ rounded to 1e-6 kn, Δ % of B absent where B is 0 kn; the 0° row takes no part; a region is a per-TWS run of TWA cells beyond ±threshold (0.05 kn default, a Compare setting); the difference surface lies midway between A and B, one-only cells at that operand's speed, grey and hatched; the diverging scale is blue–orange in OKLab, symmetric about zero, with dark- and light-scheme stops and not the flash orange; A, B, % and threshold are view state keyed by project id, not undoable or saved; a source row's Compare opens A = source, B = blend. Review round 1: in % mode a cell where B is under 0.1 kn is not comparable in % (plain grey, out of the % statistics, counted; packet v2 carries the count); the heat map is a canvas; the 3D hatch covers only all-one-only quads and every one-only node gets a cross | Spec §11 named the operands, the surfaces, the overlap rule and the summary without saying how operands are read, where the difference surface lies, what a region is, or how choices persist |
| D29 | MCP service (M21): an MCP server inside the application, following VectorEffects' (its `docs/superpowers/specs/2026-09-16-mcp-service-design.md`): `rmcp` over Streamable HTTP on `127.0.0.1` only (default port 47392, one above VectorEffects'), a bearer token issued when the Settings switch is turned on and cleared when it is turned off, `Host` and `Origin` held to loopback names, always compiled in but creating no socket, thread or task while off; curated tools plus an `invoke` escape hatch, every one calling the interface's own command, so an agent's edit is undoable and the views follow it; buttons for Claude Code, Codex and Claude Desktop. **No ChatGPT button**: ChatGPT reaches MCP servers only over public HTTPS or OpenAI's Secure MCP Tunnel, never loopback, and the application neither exposes itself nor runs a tunnel. Recorded as the one inbound exception to invariant 4, not a repeal: a second inbound socket would need the same four properties (off until switched on, bound to a token that switch issues, loopback names only, the domain through the interface's commands) and its own entry. The WebDriver rule (D25) is unchanged. Design in `docs/superpowers/specs/2026-10-01-mcp-service-design.md` | Requested by the user 2026-10-01 ("add mcp server support with on/off in settings … follow the design and decisions in vector effects"); the ChatGPT ruling settled with the user the same day |
| D30 | M20 (settled with the user 2026-10-01): the application is **PolarExplorer**, renamed in everything a person reads only — the bundle identifier `com.polareffects.desktop`, the settings folder, the PostgreSQL id namespace `polareffects/syrf/…`, the repository, the `pe-` prefix and `.wpsproj` are unchanged, so nothing on disk moves. Source **weights run 0–1** (they ran to 2); a stored weight above 1 is **clamped to 1** on load (schema 7), not rescaled, so a blend that leaned on one changes. Dots can be coloured by the **time of day**, four bands of local mean solar time (UTC shifted by longitude, 15° an hour): night 21–05, morning 05–12, afternoon 12–17, evening 17–21 — clock bands, not the sun's elevation; decided in Rust and carried in bits 8–9 of each sample's flags (3D scene layout 4, 2D dots layout 2). The blend names what is behind a cell (`blend_cell`, from the same code that sums the weighted mean). Compare reads a polar source by the project's interpolation rule, as the blend does. The 2D plot has a Measure tool that reads the drawn curves and changes nothing. The GRIB provenance text follows the name, so `reanalysis.grib2` and its pinned hash changed | The user's request of 2026-10-01 and their answers to four questions (rename depth, band definition, ChatGPT, old weights) |
| D31 | M22 (settled with the user 2026-10-02): **the ORC catalogue can be scraped at run time**, from ORC's own public service `data.orc.org` (`DownRMS`, ORC family, one JSON document per country for the current VPP year), the origin of the embedded jieter/orc-data catalogue. This amends invariant 4's "never fetched at run time" and adds `data.orc.org` to `pe-trackers`' allow-list; the embedded catalogue stays, and scraped certificates are kept beside it in the catalogue's own format. **No duplicates:** a certificate is its `RefNo` — scraped again it replaces the stored one in place; one its country's list no longer names is withdrawn; and it takes the place of its embedded twin in search. Two valid certificates of one boat in one year (crewed and double-handed: 14 boats in the first live scrape, which is what corrected an earlier same-boat-same-year rule) are both kept, told apart by their numbers. **Another year's certificate of the same boat is kept beside it**, as for ORR. **Schedules:** each catalogue, ORC and ORR, is downloaded *Manually only* (default), *On startup* or *On shutdown*; a scheduled download is skipped when that catalogue was written less than 24 hours ago, because a full ORC scrape is about 60 MB from a public service. On shutdown quitting waits, says so, and a second quit cancels the scrape and quits | Requested by the user 2026-10-02 ("add options to scrape the orc polars to the settings panel. ensure scraped orc polars are not saved as duplicates. add settings to automatically scrape the orc and orr polars on startup, shutdown or manually only"); host, same-boat-newer-year and frequency chosen by the user the same day |
| D32 | M24 (settled with the user 2026-10-02): **Split Wave Angle sorts samples by the wave direction relative to the boat** (0° on the bow, 90° starboard beam, 180° astern, 270° port beam), not by compass direction, and its counts are **4, 8, 16, 18, 24 and 36** (the request named 4, 8, 16 and 18; 24 and 36 were added when asked). Directions are centred on the bow so 0° is never a boundary. The copies are viewports of one canvas and one camera rather than a view each: a WebGL context per copy would pass the webview's limit well before 36, and one camera is what keeps them in step. The split is a way of looking, kept in the view and not in the project. |
| D34 | (settled with the user 2026-10-06): **the SYRF database's metadata download is restored, read-only**, amending invariant 4's "no crate connects to a database" (2026-10-04). Only `pe-app`'s `library::database` has a PostgreSQL client (`postgres` with rustls/ring; `check:offline` admits it there alone). It connects only when the person presses **Download boat metadata** or **Test connection**, with no schedule. The server enforces read-only twice, with `default_transaction_read_only` on and a `READ ONLY` transaction, so the earlier download's temporary tables became CTEs. The SQL export, `pg_dump` and scraping into the database stay removed. The download **merges**: the database's records replace their own earlier ones, scraped records stay, and the database copy wins a race both hold | The user's request of 2026-10-06 ("restore a setting in the settings to do a read only metadata export from the syrf database") and their answers: read-only metadata only, merge keeping scraped records |
| D33 | M25 (settled with the user 2026-10-02): **Remove tacks and gybes leaves out only the sample on either side** of a change of tack, no time window. Schema 8: the dropped filters vanish on load, a track's one given-or-derived preference becomes one for heading and one for speed (both what it was), and Stokes drift loads off; the setting stays in the document for the fetch. Full-size 2D puts the 0° axis on the stage's centre line; the right panel keeps the sources. Heading/COG/TWD ranges are compass sectors needing both bounds; the editor holds a lone bound until its partner is typed rather than send half a sector. The 3D first-load framing uses a camera view offset to the free area's centre, set once, so later panel toggles leave the framing as it was (spec §3.2). |
| D22 | Polar edits and segments (M13): one `EditCells` command for every edit tool (overrides before/after per cell, the tool naming the undo entry, drags coalescing); segment bins are half-steps around each output-grid node with nothing beyond the outer half-steps, and nothing is binned into a 0° TWA node (samples nearest 0° are dropped, not moved: the 0° row is 0 kn, spec §12.3; controller ruling); spread is the sample standard deviation; smooth is the 3×3 binomial kernel over neighbours with a value as the blend reads them (excluded nodes take no part; controller ruling); the 3D samples key mixes a per-opening nonce, so two openings never share one; views show sources as edited, and the 2D curves also leave excluded nodes out | Spec §10.4 and §12.1 named the tools, the statistic and "count and spread" without the binning edges, the spread measure, the kernel or how undo groups them |

## 6. Settled before coding started

The user accepted the proposals below on 2026-09-27 ("all good").

- **Q1 — Tides before November 2020 outside NW Europe/IBI:** accept "no
  tide" for those samples, flagged and filterable. FES2014 stays deferred.
  *Superseded by Q7 (M9):* GlobCurrent 202411 already includes a FES2022
  tide, so those samples have tides and none is flagged.
- **Q2 — Wind after 2023-01-10:** ARCO-ERA5 supplies it (D12).
- **Q3 — Stokes drift:** excluded by default (`uo + utide`), with a setting
  to include it.
- **Q4 — Blend rule:** weighted mean per cell with sample-count confidence
  (spec §12.3).
- **Q5 — Linux ARM64:** deferred.
- **Q6 — Terms of use:** the user accepts the risk; imports stay
  user-initiated, one event at a time, with polite concurrency.

### Settled in M9

- **Q7 — GlobCurrent and tides:** settled by the stores' metadata on
  2026-09-28 (controller ruling): GlobCurrent 202411 `uo`/`vo` are "absolute
  geostrophic velocity + depth Ekman + tide velocity" (FES2022) in both the
  multi-year and near-real-time stores, so the tier is recorded
  `has_tide: true` and nothing is labelled "no tide". The tier order is
  unchanged.

---

## Appendix A — Geovoile hwx decoding

Verified on 24hultim 2025 and Vendée Globe 2016 (2026-09-27), whose
resources are committed fixtures, and in M11 (2026-09-28) on Route du Rhum
2018, New York Vendée 2024 and Solitaire du Figaro 2024 leg 1, also
committed.

- The seeds are four 24-bit constants in the first base64 `/C/…` segment of a
  `data:image/png` source in the viewer HTML (the segments may be spread
  over several such sources: Route du Rhum 2018 has the constants in one
  and the keystream in the next). They differ per site (2022–2025
  sites: `0x7BC495, 0x4557FA, 0xD56AAF, 0xFF8040`; VG2016:
  `0x88FE88, 0xFE88AA, 0xEECC80, 0xA0A0F0`), so always parse them.
- Keystream (all arithmetic masked to 24 bits):

```
step(): t = x; t ^= (t << 11) & 0xFFFFFF; t ^= (t >> 8) & 0xFFFFFF
        x = y; y = z; z = w; w ^= (w >> 19) & 0xFFFFFF; w ^= t
dec(b): r = b ^ (x & 0xFF); step(); return r
```

- Container:

```
skip buf[0] steps
out_len = dec(buf[1]) << 16 | dec(buf[2]) << 8 | dec(buf[3]); i = 4
loop until out.len() == out_len:
  flags = buf[i] ^ (i & 0xFF) ^ 0xA3; i += 1      // flags are NOT keystream-decoded
  for bit in 7..=0:
    0 → out.push(dec(buf[i])); i += 1
    1 → b = dec(buf[i]); len = (b >> 4) + 3
        off = ((b & 0xF) << 8 | dec(buf[i + 1])) + 1; i += 2
        copy len bytes from out[out.len() - off], byte by byte
```

- Output is UTF-8: XML for `config`, JSON-like JS literals for the rest.
- A working Python reference was written during research; port its test
  vectors into `crates/pe-trackers/tests/fixtures/geovoile/`.

## Appendix B — YellowBrick AllPositions3

Verified against `fastnet2025` (5,726,173 bytes consumed exactly, 444 teams,
positions at Cowes and the Cherbourg finish).

```
u8  flags   bit0 alt, bit1 dtf, bit2 lap, bit3 pc
u32 refTime epoch seconds
repeat until EOF:
  u16 teamId (= RaceSetup teams[].id), u16 count
  count moments, newest first:
    (peek & 0x80) == 0 → absolute:
        u32 t (at = refTime + t), i32 lat, i32 lon
        [i16 alt] [i32 dtf [u8 lap]] [i32 pc]
    else → delta from the previous (newer) moment:
        u16 w (dt = w & 0x7FFF; at = prev.at − dt), i16 dLat, i16 dLon
        [i16 alt] [i16 dDtf [u8 lap]] [i16 pc]
lat, lon = value / 1e5 degrees; dtf in metres
```

The alt, lap and pc layouts are unverified (the Fastnet data had those flags
off); cover them with a fixture from a race that sets them before relying
on them. The M3 decoder follows the table literally: in a delta moment,
`alt` and `pc` are read as values, not deltas, and only `dDtf` accumulates.


## SYRF PostgreSQL library — 2026-09-30

Implemented database settings and connection test, local filtered boat metadata,
boat-name track search/import, native scraping and startup/shutdown scheduling,
and portable full-database SQL export (spec §3.4.1). Matched the supplied database
to `syrf-tracks-individual-production` after the user corrected the archive root.
Read-only validation found 79,440 searchable tracks and imported a real Lurline
track. Isolated PostgreSQL ingestion and SQL export/restore checks verify stable
identities and schema compatibility. The initial anonymous-only YB discovery
limitation is addressed by the authenticated catalogue follow-up below.

Validation: all 644 Rust tests and all 342 UI tests pass, including help
and translation coverage. The database desktop UX test covers failed/successful
connections, persisted directories, search, import and the weather button, with
inspected screenshots. Live native scrapes into an isolated schema clone passed
for Fastnet 2025 (444 tracks), 24 Heures Ultim 2025 (14 tracks) and Melbourne
Hobart Westcoaster 2025 (5 tracks), including a repeat scrape without duplicate
rows and reimport of the resulting individual GeoJSON. A separate regression
check verifies reuse of an upstream calendar GUID for a new Geovoile leg,
preserving original URLs, IDs, privacy and the earlier start time.
Historical reused provider boat IDs are matched to existing race track
references; ambiguous matches fail instead of overwriting another boat.
Formatting, clippy, TypeScript, UI performance and the network-boundary check
pass. The macOS production build (`npm run build -- --no-bundle`, no WebDriver)
passes. Windows and Linux builds were not executed on this host.

### Follow-up: search every vessel field

Implemented local search across every Vessels value, including model, class,
make, builder, measurements, booleans and nested/custom JSON values. Query words
may span fields of the same vessel. The folded search index is cached in memory
and shared across that vessel's tracks. Existing version-1 metadata snapshots
work without rewriting or downloading them again. Updated the search label,
placeholder, help keywords, French/German translations and user guide.

Validation: 645 Rust tests and 342 UI tests pass, along with clippy, formatting,
TypeScript, UI performance and the network-boundary check. The desktop test
individually searches model/class/make/builder, combines fields, then imports
the result and verifies the weather control; its screenshot was inspected.

### Follow-up: asymmetric mode in the top bar

Moved **Asymmetric polar (360°)** from Blend settings to the title bar beside
the stage switcher. The checkbox applies the existing mode and output-axis
change immediately through the same undoable settings command. Blend settings
uses the selected mode for axis validation, presets and defaults. Help search
lands on the top-bar control, with the existing French/German translations.
Undo/Redo works with the checkbox focused. Updated the guide and specification.

Validation: 645 Rust tests and 342 UI tests pass; the focused shell/settings
rerun, formatting, clippy, TypeScript, production UI build, UI performance and
offline checks pass. The desktop polar-analysis walkthrough verifies the
top-bar toggle and Undo/Redo, then edits the full-circle grid and renders both
plots; the top-bar and Blend settings screenshots were inspected. Its catalogue
pagination check now waits for the new rows, not just the updated page number.


### Follow-up: YellowBrick v3 catalogue and authenticated race codes

YellowBrick discovery now reads `App/Races?version=3`, resolves codes through
configured `MyRaces?version=4` credentials, associates missing products only
when explicitly listed as free, and re-reads the codes. The user's authorized
SYRF credentials are stored only in their local settings, never bundled or
logged. Settings exposes masked user-key and UDID fields with help and French /
German translations. Authentication errors redact request URLs, cancellation
stops further association, and three consecutive failures end association.

Recorded mobile XML fixtures cover repeated noncontiguous base URLs and child
races. Codes with ampersands, dots and encoded spaces round-trip through import.
Known database URLs are matched by exact catalogue ID; numeric IDs, product IDs
and titles are never guessed into tracker codes. The metadata directory's
`yellowbrick-races.json` records every catalogue entry, including unresolved
ones. Ingestion groups new child races under the numeric parent calendar ID,
retains existing GUIDs and original URLs, and treats YB key case aliases as one
race.

Live validation on 2026-09-30 found 1,689 catalogue entries and 2,767 distinct
URLs after exact database matches. 48 entries had no available code and remain
explicitly unresolved. Public RaceSetup checks passed for Aeolian 2026,
Palermo–Montecarlo 2021 and Stars and Spokes 2023; the historical ARC 2011 endpoint
returned HTTP 503 after bounded retries. No production track database writes
were made for this follow-up's verification.

Validation: 651 Rust tests and 342 UI tests pass, together with formatting,
clippy, TypeScript and the network-boundary check. An isolated PostgreSQL test
verifies parent grouping, case alias deduplication and original URL/GUID
preservation; its scratch database was removed afterward. The desktop database
walkthrough passes with six screenshots, including masked credentials, settings
persistence, a green read-only database connection, vessel-field search and
track import with weather actions. Credential and connection screenshots were
inspected.

The four serial UI performance checks and the macOS production build
(`npm run build -- --no-bundle`, WebDriver excluded) pass. The final built-bundle
network check passes. Windows and Linux builds were not run on this host.


### Follow-up: side panels overlay the centre view

The left navigation and right source/plot panel are positioned over a stage
that always fills the workspace. Toggling either panel preserves the map and
polar canvas bounds, drawing buffer and camera framing. Floating view controls
stay in the uncovered area. Panel dialogs remain above the title bar and both
panels; the full-size polar plot also opens above the panels. Existing toggle
state and section behaviour are preserved. Updated spec §3.2 and the user guide.

The desktop project walkthrough measures the stage and canvas in 3D, Map and
Compare through all four panel combinations, verifies pointer hit targets, and
checks modal and full-size plot stacking. The walkthrough passes; 3D and Compare
screenshots were inspected.

Validation: 651 Rust tests, 342 UI tests, four UI performance checks, formatting,
clippy, TypeScript, the production UI build and network-boundary check pass.
Both desktop walkthroughs (project layout and polar analysis/editing) pass.
No Rust domain code or project formats changed.


### Follow-up: live analysis, supplied wind and dot tooltips · **complete** (2026-10-01)

Add 3D dot hover details, supplied true wind in CSV/GeoJSON/SYRF imports with an
undoable per-track choice, and AWA/wind change thresholds in track, global and
priority filters. All change thresholds compare only the immediately previous
and next points, as confirmed by the user. Preserve downloaded weather separately
from immutable supplied data
(schema 5), and keep stop/tack/gybe windows available at every filter level.
Apply valid numeric edits while typing, serialize rapid changes and ignore stale
project summaries. Shorten the top-bar label to Asymmetric polar.

The main wave sliders now also feed the native blend and both views as saved,
undoable additional constraints. This supersedes M19c's display-only behaviour;
other analysis filters remain independent.

Validation: 660 Rust tests and 349 UI tests pass (the full UI run plus the
focused rerun after correcting a German help-search wording conflict). Formatting,
workspace clippy, TypeScript, the production UI build and the offline boundary
check pass. All five UI performance checks pass. The native filter pass over
200,000 points with all four change thresholds takes 45.4 ms for changing data
and 48.8 ms for steady sailing. The wave-range and live-analysis desktop
walkthroughs pass, with screenshots inspected. These cover live edits without
blur, supplied/downloaded wind switching, each filter family, and real dot hover.
Save/reopen tests include high-precision supplied wind, undo/redo, and unchanged
project bytes. Supplied wind used in calculations matches document precision;
the source data and downloaded weather remain separate.

### Follow-up: compact wave range controls · complete (2026-10-01)

Place wave height, angle and period ranges at the bottom centre of the 3D view.
Each row uses one rail with two round start/end handles and visible bound values.
Retain native keyboard adjustment, separate hit areas even when handles meet,
unit conversion, non-crossing bounds, reset, and live updates to the blend.

Validation: 660 Rust tests, 349 UI tests and all five UI performance checks pass.
Formatting, workspace clippy, TypeScript, the production UI build and the offline
boundary check pass. The desktop wave-range walkthrough passes with screenshots
inspected, including both handles at a shared endpoint, centring as either dock
opens or closes, and clearance below the polar editor.

### Follow-up: drag the selected wave range · complete (2026-10-01)

Make the selected section between each pair of wave handles draggable. Move both
limits together without changing their width, clamp at the scale endpoints,
retain independent handle interaction and live filtering, and provide keyboard
movement of the selected range.

Validation: all 660 Rust tests, the full UI suite (351 tests), the final focused
UI run (46 tests, including an added saved-range scale regression), and five UI
performance checks pass. Formatting, clippy, TypeScript, the production UI build
and the offline boundary check pass. The desktop walkthrough and inspected
screenshots confirm the middle rail receives pointer input, handles remain
reachable, and dragging updates the native blend before release. Unit checks
cover endpoint clamping, pointer cancellation, feet conversion, keyboard
movement and a stable scale when saved bounds exceed the remaining samples.

### Follow-up: multiple boats, synchronized comparison and tracker projects · complete (2026-10-01)

1. Persist independent boat documents in one project, migrate older projects,
   scope requests and background work to boat identities, and preserve recovery.
2. Add editable boat tabs, add-boat and export-all controls, and two/four-pane
   3D comparison with synchronized rotation and corresponding-point hover.
3. Preserve tracker boat metadata and build a native, cancellable event-project
   import that finds polars and tracks of identical models, using synonymous
   descriptive fields and identity clues without broad class/name-only matches.
4. Validate model matching, save/reopen, isolation, export, synchronized views,
   and offline tracker import with unit/integration and real desktop checks.

Implemented independent boat documents with save/recovery and scoped background
jobs, editable tabs and export-all, two/four-pane 3D comparison, and native
YellowBrick/Blue Water project creation. Automatic polars and historical routes
require specific model evidence. Names never establish identity; contradictory
models, builders, lengths or identity MMSIs prevent the relevant association.
Unresolved boats retain their original race track without guessed sources.

Validation: 674 native tests, the full 356-test UI suite plus two new help
isolation tests, targeted final UI checks, typecheck/production UI build,
fmt/clippy and offline checks pass. Desktop tests 11, 13 and 14 cover wave
filters, linked camera gestures and four-way hover, restored single-view panels
and rendering, boat creation/renaming, and same-name tracker boats with different
model evidence. Screenshots were inspected. All five serial performance tests
pass: 200k-sample wave-range updates take 15–32 ms; two 512×512 polar surfaces
take 21–39 ms; three comparison surfaces take 62–94 ms (100 ms budget).

### Follow-up: scrape finished races only · complete (2026-10-01)

Apply one completion gate to on-demand, startup and shutdown library scraping,
including explicit URLs. Require terminal results for all participants and
reject future or unverified completion. Check YellowBrick setup and Geovoile
reports before track requests; check each Geovoile leg independently. Blue
Water's combined metadata/positions response must be read to establish status
and must never be ingested when unfinished. Preserve normal interactive imports.
Verify skipped downloads against recorded HTTP requests and completed imports
against provider fixtures, then run repository and desktop checks.

Validation: 680 native tests, 358 UI tests and all five serial performance tests
pass. Typecheck/production UI build, fmt, workspace clippy and offline checks
pass. Recorded HTTP tests prove no YellowBrick positions/KML or Geovoile tracks
are requested for ongoing, future or unverified races; completed fixtures still
download. Blue Water's combined response is rejected before ingestion when any
boat lacks a terminal result. The database-library desktop walkthrough passes,
and the finished-races policy screenshot was inspected.

### Follow-up: repair Intel macOS development linking · complete (2026-10-01)

Disable incremental compilation throughout the development profile to prevent
Rust 1.97.1 from reusing broken internal LLVM symbol references in optimised
workspace crates. Keep the existing math optimisations and release profile.
Update build guidance and verify normal Tauri development compilation and an
edit/rebuild with the default profile.

Validation: the native app links, and `npm run dev` launches with
`CARGO_INCREMENTAL` unset (reusing the existing Vite server). A timestamp-only
change to `pe-core/src/lib.rs` triggers the person's existing development
watcher, which recompiles pe-core without `-C incremental` and successfully
relaunches the app. All 680 native tests, formatting, workspace clippy,
TypeScript, five UI performance checks and the offline check pass. The unchanged
frontend also passed its full 358-test suite immediately before this build fix.
The Homebrew Rust library deployment-version warnings are separate from the
undefined-symbol failure; the application's deployment target is unchanged.

### Follow-up: boat controls and exact-vessel tracker projects · complete (2026-10-01)

Move Add boat beside the project title, rename tabs in place, hide tabs during
comparison, and add reversible boat deletion. Start every boat with 0° upwards.
Offer exact-vessel matching as an alternative to identical-model matching and
show all available tracker boat metadata in the project-opening report. Verify
identity safeguards, deletion/save/recovery, initial camera direction, and the
complete desktop interactions.

Implemented Add boat beside the project title, inline tab renaming (double-click
or F2), dropdown-only navigation in comparison layouts, 0°-up initial cameras,
and Delete boat/Undo delete boat. Root promotion preserves the fleet title, file
location, sibling views and comparison layout; removed boat identities cannot
redirect late work. Exact-boat matching applies to both polars and historical
tracks, requiring corroborated identity and rejecting conflicting specifications.
Reports show primary vessel details and expandable complete original metadata.

Validation: all 683 native tests pass, including saved-file/recovery deletion
round trips, restoration of local edit history, stale identities, and positive
and negative exact-vessel matches for polars and historical tracks. The full
361-test UI suite passes, plus the added first-boat comparison-deletion test
and final focused checks (50 tests; the final boat-only rerun also passes).
The final app lifecycle/UI rerun passes all 44 selected tests. Formatting,
workspace clippy, TypeScript/production UI build, offline checks and five UI
performance checks pass. Both desktop walkthroughs pass; inspected screenshots
cover the top-bar control, two/four-pane navigation, 0°-up orientation, deletion
and restoration, both tracker matching modes, original boat details and adding
a boat to a tracker-created project.

### Follow-up: cancel the tracker boat list · complete (2026-10-01)

Add Cancel beside Open project in the boat-list report. Stage the import until
explicit confirmation so cancellation and Escape preserve the existing fleet,
unsaved edits, history and recovery. Guard confirmation against stale previews
and changes to the current project. Verify both native state and desktop flows.

Validation: all 686 native tests and 365 UI tests pass, plus five performance
checks, formatting, clippy, TypeScript/production UI build and offline checks.
The desktop walkthrough verifies cancellation from the start screen and an
unsaved three-boat project, then confirms normal opening still works; screenshots
were inspected. Native tests preserve saved bytes, unsaved edits, boat history
and recovery, and reject stale confirmation. The help walkthrough now hides
boat tabs using the fleet comparison layout before testing their reveal action.

### Follow-up: plot scales, stage memory, file-only track library, class selector · complete (2026-10-04)

- [x] 2D and 3D axes fit the shown dots: excluded and filtered points stop
  stretching them; **Excluded points** shows excluded dots and rescales.
- [x] Dot tooltips read SOG (STW for current-corrected dots); polar nodes keep BSP.
- [x] Map, Compare, 2D and 3D stay mounted, so a stage comes back as it was left.
- [x] The database connection is removed; the library's scraper is restored
  writing only GeoJSON files and `boat-metadata.json`, finished races only,
  with a status bar and cancel for a manual scrape.
- [x] Open project from tracker offers classes, several at once, each group
  its own class and one tab per boat (2026-10-05);
  MCP `race_project` takes `classes`.

Validation: workspace fmt, clippy and tests, UI typecheck, tests and
performance checks, offline check and driver tests pass; UX tests 10 (scrape
of a recorded finished race, status bar) and 14 (class selector) pass with
screenshots inspected.

### Follow-up: scrape held races never, and patiently · complete (2026-10-05)

- [x] A race the library holds is never fetched again, listed or not; a
  snapshot boat without a file no longer makes 682 races look incomplete.
- [x] Geovoile legs: an address without a leg holds the leg its name gives,
  and any leg holds the race; new saves record the leg's own address.
- [x] The scraper's copies of races the snapshot already held (6 in the
  person's library) are removed at the next scrape's start.
- [x] Requests 1 s apart, 2 s between races, busy answers retried at 5–120 s
  (or the server's `Retry-After`), turned-away races retried once at the end.
- [x] Races download Blue Water first, then YellowBrick, then Geovoile.
- [x] Errors name the tracker; the status counts "already in the library";
  a scrape that ends before its start answers no longer shows running.

### Follow-up: read-only SYRF database metadata download · complete (2026-10-06)

- [x] Settings → Track library → **SYRF database (read only)**: connection
  fields, **Test connection**, **Download boat metadata** and **Cancel
  download** (D34), in nine languages and the help search.
- [x] Read-only on the server's side (session default and `READ ONLY`
  transaction); CTEs instead of the earlier temporary tables.
- [x] Merged into `boat-metadata.json`, keeping scraped races; a scrape and
  a download never run at once.
- [x] `check:offline` admits the PostgreSQL client in `pe-app` only.
- [x] Closing Settings saves the library section's changes (a changed
  GeoJSON folder was lost before); a refused change keeps it open.

Validation: workspace fmt, clippy and tests, UI typecheck and tests, and
the offline check pass. The merge has a hand-built test. An ignored test
(`PE_TEST_POSTGRES=1`) builds a scratch database on the local server,
downloads it, and shows the session refuses an `INSERT` and a temporary
table; it passed. Not run: a download from `syrfbackendprod` itself.

### Follow-up: polar blend statistic · complete (2026-10-10)

- [x] Blend settings → **Polars** → **Polar blend statistic**: *min*,
  *median*, *mean* (default, the rule as it was), *max* or *p90* across the
  visible polar sources per cell (spec §12.3); the polars keep their
  combined weight against the tracks; `blend_set` takes `polar_statistic`.
- [x] In nine languages, the help topic and the help search.

Validation: workspace fmt, clippy and tests (hand-computed min, median, max
and p90 cells, polars mixed with a track, export), UI typecheck and tests,
and the offline check pass.
