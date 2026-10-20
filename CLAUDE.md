# CLAUDE.md

Guidance for Claude Code working in this repository.

**Read `spec.md` before changing behaviour. Read `plan.md` before starting new
work.** This file is the operational layer: invariants, layout, commands,
conventions, and recipes. It follows the sibling application VectorEffects
(`/Users/jon/VectorEffects`), whose architecture this project copies. When a
question below has no answer, look at how VectorEffects solved it first.

---

## What this is

PolarExplorer is a Tauri desktop app that builds sailing polars in independent boat tabs within a project.
It blends ORC polars, imported Expedition/Adrena polars, and polar segments
derived from historical race tracks. Each track position is matched with
reanalysis wind, waves and current, and the result is exported as an
Expedition or Adrena polar. Rust owns the entire domain: document model,
imports, scraping, reanalysis sampling, polar math, blending and export.
React/TypeScript is a view layer only.

---

## Hard invariants

Violating any of these is a design regression. If a task seems to require it,
stop and raise it rather than working around it.

1. **Sources are immutable; edits are overlays.** An imported polar, an ORC
   record, and a track's raw fixes are stored exactly as imported. Every user
   change is stored beside the source as an overlay: an excluded sample, a cell
   override, a filter, a colour or a weight. Removing every overlay must give
   back the source exactly. Nothing silently rewrites imported data.
2. **The blend is derived, never stored as truth.** The blended polar, the
   per-track polar segments, and the comparison surfaces are recomputed from
   sources plus overlays. They may be cached in memory, never persisted as
   project data. Export always recomputes.
3. **Fetched environment data is project data, rendered views are not.**
   Wind, wave and current values interpolated at each track position are
   saved in the project with the dataset name and version they came from —
   only those values, kilobytes per track. Open Data downloads live in
   memory for the session (D27). At the user's request on 2026-10-07,
   Whirlwind also keeps validated compressed Zarr inner chunks in a bounded
   disk cache, in the selected cache directory. It is disposable: clearing
   it never removes project weather, and only owned cache files may be deleted.
4. **Nothing is fetched that the user did not ask for, and nothing reaches
   in.** No CDN fonts, no map tiles, no telemetry, no remote schema fetches.
   The webview's CSP stays `'self'`-only. Only two crates may use the network:
   `pe-env` (reanalysis archives) and `pe-trackers` (YellowBrick, Geovoile,
   Blue Water Tracks, and the certificate catalogues' scraping: ORR on
   `www.regattaman.com`, ORC on `data.orc.org`), each for its allow-listed
   hosts only. A catalogue scrape runs when the person starts it, or at
   startup or shutdown once they have chosen that schedule for it in
   Settings, and then at most once a day (spec §5.4).
   The SYRF track library is files the person points Settings at (asked
   2026-10-04). Its race scraper, through `pe-trackers`, saves finished
   races only, into those files, when the person starts it or on the
   startup or shutdown schedule they chose. The one database connection
   (D34, asked 2026-10-06) is `pe-app`'s `library::database`: it reads the
   person's SYRF PostgreSQL server into `boat-metadata.json` only when they
   press **Download boat metadata**. It has no schedule, and its session and
   transaction are read-only on the server's side. It never writes, and no
   other crate has a database client.
   `npm run check:offline` enforces this. The ORC catalogue is bundled at
   build time; what a scrape adds is kept beside it on this computer, and
   searching or adding a certificate never fetches.
   **The invariant runs both ways** (D25): an *inbound* socket that drives the
   application is the same promise broken from the other side. `pe-app`'s
   optional `webdriver` feature compiles in a WebDriver endpoint on loopback
   for agent-driven UI tests; it is off by default, `npm run build` does not
   pass it, and `tests/webdriver_optional.rs` reads every workspace manifest
   (optional in every dependency table, no default reaching it, named by no
   other crate), `package.json`'s build scripts and the Tauri configs to hold
   it that way. `npm run check:offline` **cannot** see a listener — it reads
   source URLs, remote references in the built bundle and the CSP — so for
   anything inbound the enforcement is that the dependency is not compiled in.
   Its seams (`PE_AUTOMATION_ROOT`, `PE_DRIVER_YELLOWBRICK`,
   `PE_DRIVER_HINDSIGHT_PORT`: a driven run asks only that port for Hindsight
   Sync, or none) are compiled only with the feature; the frontend's (`ui/src/automation.ts`) only in
   development builds. Never ship the WebDriver feature.
   **And one inbound exception (D29):** the MCP service, `pe-app`'s `mcp`
   module (spec §3.7), listens on `127.0.0.1` only while the person has
   switched it on in Settings and only for the token that switch issued. It
   is always compiled in, and creates no socket, thread or task while off.
   `tests/mcp.rs`'s `off_means_no_socket_and_stop_releases_the_port` is the
   enforcement; `check:offline` still cannot see a listener, and admits in
   `pe-app` exactly the service's server crates with exactly their server
   features, each on one line (`hyper`: server, http1; `hyper-util`: tokio;
   `rmcp` without default features: server, macros,
   transport-streamable-http-server; `reqwest` in dev-dependencies only). The WebDriver rule is unchanged: that endpoint is unauthenticated,
   drives the interface rather than the domain, and has no switch. A second
   inbound socket would need the same four properties (off until switched
   on, a token that switch issues, loopback names only, the domain through
   the interface's own commands) and its own decision.
5. **Export is deterministic and byte-reproducible** across machines and
   platforms: the same project gives the same `.txt`, `.pol`, `.csv` and
   `.grib2` bytes.
6. **Interaction stays fast; imports and fetches may be slow.** Long work
   (scraping, reanalysis sampling, GRIB writing) runs off the UI thread,
   reports progress, and can be cancelled. Editing a polar updates every open
   view within the budget in spec §13.
7. **Every string is translatable and every control is findable.** No
   user-visible text bypasses `t()`/`msg()`, and every interactive control has
   a `data-feature` id registered in the help search.

---

## Repository layout

```
crates/
  pe-core/      Document model (Project, sources, overlays), undo/redo,
                IDs, geodesy, .wpsproj I/O, migrations, canonical floats
  pe-polar/     Polar grid type, interpolation, Expedition/Adrena readers
                and writers, track-sample binning, blending, comparison
  pe-orc/       The embedded ORC catalogue (generated at build time), the
                store of scraped certificates merged into it, and the
                search index
  pe-tracks/    Track model, GeoJSON/CSV import, heading/speed derivation,
                sample filters. No network
  pe-trackers/  Network: YellowBrick, Geovoile (hwx decoder), Blue Water
                Tracks clients and decoders; the ORR and ORC certificate
                scrapers
  pe-env/       Network: zarr readers for WeatherBench2 wind, ARCO-ERA5
                waves, and ocean currents; pure-Rust blosc/LZ4 with block
                reads by HTTP Range; in-memory block LRU; space-time sampling
  pe-grib/      GRIB2 writer (fixed-layout message template, as in
                VectorEffects' ve-grib), regional grids
  pe-app/       Tauri shell: IPC commands, state, jobs, autosave, settings;
                `src/library/` the SYRF track library (scraper, search,
                the read-only database metadata download);
                `src/mcp/` the MCP service (listener, tools, client
                registration, the Claude Desktop bridge)
ui/             React + TypeScript + Vite front end
  src/project/  Start screen, project menu, save guard, dialogs
  src/map/      WebGL2 world map with tracks
  src/polar/    2D polar plot, 3D polar view (three.js), compare view
  src/panels/   Left navigation (ORC / Polar files / Tracks), source list
  src/settings/ Settings dialog, themes, language picker, MCP section
  src/mcp/      The interface following the MCP service; the stage capture
  src/help/     Feature registry, search, highlight, help topics
  src/i18n/     Catalogues (en, fr, de, es, it, nl, zh, ja, ar), glossaries,
                coverage tests
  src/generated/ ts-rs bindings. Never edit by hand
assets/         Natural Earth basemap, ORC catalogue build input, samples
tools/          orc-catalogue-builder, basemap-builder, check-offline.sh,
                webdriver/ (driver client, CLI, MCP server, ux/ suite)
docs/           AGENT-UI-TESTING.md
```

**Dependency direction:** `pe-app` → everything. `pe-polar`, `pe-tracks`,
`pe-orc` and `pe-grib` → `pe-core`. `pe-trackers` → `pe-tracks`. `pe-env` →
`pe-core`. Never the reverse, and `pe-core` depends on no sibling.

---

## Commands

Everything runs from the **repository root**. The Tauri CLI only resolves the
app crate from here.

```bash
npm install                 # root npm workspace; installs ui/ too
npm run dev                 # tauri dev
npm run ui:dev              # frontend only, no Rust backend

# Checks: run all of these before declaring work done
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
npm run ui:typecheck
npm run ui:test
npm run ui:perf             # timed tests, serially: the spec §13 budgets
npm run check:offline       # invariant 4

# Regenerate TS bindings after changing any IPC-facing Rust type
npm run bindings            # cargo run -p pe-app --example export_bindings

# Rebuild the embedded ORC catalogue from a jieter/orc-data git checkout
# (read from its HEAD commit; the build date is that commit's date unless
# SOURCE_DATE_EPOCH is set, so rebuilding the same commit is byte-identical)
cargo run -p orc-catalogue-builder -- ../orc-data/site/data \
    crates/pe-orc/data/catalogue.bin

# Tests that use the network on purpose (never in the default suite)
PE_TEST_LIVE=1 cargo test -p pe-env --test live -- --ignored --nocapture
PE_TEST_LIVE=1 cargo test -p pe-trackers --test live -- --ignored --nocapture

npm run build               # tauri build for the host target
npm run dev:webdriver       # the app with the WebDriver endpoint (macOS).
                            # NEVER shipped: see `pe-app`'s [features]

# Driving the running application (D25; docs/AGENT-UI-TESTING.md). An MCP
# server, `pe-driver` in `.mcp.json`, and the same client from a shell.
# MCP servers load when the client starts, so a session that adds one cannot
# use it until it restarts. Every driven run has its own Vite port and its
# own data root; it never touches the person's `tauri dev` or settings.
npm run ux                  # the UX suite: pictures in target/ux-shots/<test>/
npm run ux -- tracker       # only the tests whose file name contains "tracker"
node tools/webdriver/cli.mjs shot NAME     # start, capture, stop; prints the path
node tools/webdriver/cli.mjs serve         # start and hold, printing the port
PE_DRIVER_PORT=<port> node tools/webdriver/cli.mjs click|type|text|wait|dialog|eval|shot …
PE_DRIVER_OPEN=path/to/project.wpsproj     # open a project before `shot`
npm run tools:test                         # the driver's parsing and the MCP handshake
```

---

## Conventions

| Topic | Rule |
|---|---|
| Speeds | Boat speed and wind speed are stored in **knots** (the unit of every polar format and of ORC data). Reanalysis m/s is converted once, on ingest, in `pe-env`. |
| Angles | Degrees. TWA in [0, 180] in symmetric mode; [0, 360] with independent port and starboard in asymmetric mode. Full-circle imports preserve both sides. Headings and directions in [0, 360). |
| Direction sense | Wind and waves use the meteorological **"from"** direction. Currents use the oceanographic **"toward"** direction. Name the field accordingly (`twd_from`, `current_toward`). |
| Components | `u` east, `v` north, as stored by ERA5 and CMEMS. `speed = hypot(u, v)`. |
| Longitude | [−180, 180) everywhere except inside `pe-env` and `pe-grib`, which use the archive's 0–360 grid. Every geodesy function has antimeridian tests. |
| Time | UTC only, `i64` epoch seconds. Local race time zones are display only. |
| Earth | Spherical, radius 6,371,229 m (the ERA5 and GRIB shape of earth). |
| Floats | Every `f64` in project JSON goes through the `canonical` serde helpers, as in VectorEffects. |
| Determinism | No `HashMap` iteration anywhere that feeds blending, export or the saved JSON. Use `BTreeMap` or sorted `Vec`. |
| IDs | `SourceId`, `TrackId`, `SampleId` are newtype u64s allocated by the project. They are never reused. |
| Colours | Stored as `#rrggbb` on the source, never derived from list order. |

---

## Recipes

**Adding interface text.** Write English in `t("…")` or `msg("…")`. Add the
key to `ui/src/i18n/locales/{fr,de,es,it,nl,zh,ja,ar}/<area>.ts` with a real
translation (use the glossaries in `ui/src/i18n/GLOSSARY*.md`). `npm run
ui:test` fails on a missing or unused key. A help topic change goes into
`ui/src/help/topics.ts` and every `ui/src/help/locales/<lang>.ts`. Arabic is
laid out right to left (`dir="rtl"`); anything that reads as a scale (a
slider, numbers with units over a plot) stays left to right in `styles.css`.

**Adding a control.** Give it `data-feature="<area>:<name>"` and a registry
entry in `ui/src/help/features/<area>.ts` (label, description, keywords,
reveal steps). `features.test.ts` checks tags and registry match both ways.

**Adding a field to the document.** Add it in `pe-core` with a serde default.
Bump `SCHEMA_VERSION` and add a migration if old files need a value. Add a
save → load → save byte-identity test and, if it is user-editable, a `Command`
with an undo inverse test. Run `npm run bindings`.

**Adding a tracker.** A new module in `pe-trackers` implementing
`TrackerClient` (`resolve` a pasted URL to an `EventRef` without network;
`list` reads boat metadata without positions or returns `None` when only a
combined response exists; `fetch_listed` reads the whole `TrackerEvent` — title, dates, every boat and its
fixes — through the shared `Fetcher`, handing the boat list to `listed`
first when the tracker names the boats in a response of its own, and
starting independent requests at once with `Fetcher::spawn`; never any
weather, D24),
and a line in `event::client`. The dialog, session cache and import in
`pe-app/src/trackers.rs` and `TrackerImportDialog.tsx` are shared. Add its hosts to the `pe-trackers` allow-list in
`tools/check-offline.sh` and to `net::allowed_host` (which also bounds
redirects); any address taken from a response is checked against it before
it is requested. Decoders get fixture tests from a recorded response
in `crates/pe-trackers/tests/fixtures/`. No live request in the default suite.

**Adding a reanalysis variable.** Add it to `pe-env`'s variable table with
its dataset, units, direction sense and fill behaviour (land is NaN for waves
and currents). Add a fixture chunk test and record the dataset version on
each sample.

**Adding an MCP tool.** In `crates/pe-app/src/mcp/tools/<group>.rs`, inside
the group's `#[tool_router]` block: a parameter struct deriving `Deserialize`
and `JsonSchema` with a doc comment on every field (the comment is the
description a client sees; `tests/mcp.rs` refuses a property without one),
and a tool that calls the **`#[tauri::command]` function itself** with
`app.state::<AppState>()` — never a copy of its logic — through `run` (a
read) or `write` (an edit: it emits `document://changed`, so the interface
follows). Every tool that reads or edits a boat takes the optional `boat`
id and passes it as the command's `boat_context`. A structured parameter
that is one of the interface's IPC types is a `serde_json::Value` read with
`typed`, so a client's string of JSON is read and a bad value is a refusal
in words. A parameter that holds "only what to change" is written over the
current value with `patch_over`, which refuses a key that is no field. A
tool that writes a file where the caller says takes `overwrite` and asks
`may_write` first, and its command goes into `invoke`'s `EXCLUDED`. Answer
with `json(&value)`. A long tool watches `ctx.ct`: `rmcp` cancels that
token, it does not drop the future. Report the call through `run` or
`note`: that is what counts a client that keeps no session, and so what
shows the person the badge. Name the tool in
`tools/guide.rs` (the suite fails on a tool the guide never mentions, and
on a name in the guide that is no tool), and add an integration test in
`tests/mcp.rs` that drives it over HTTP and checks the document through the
interface's own read. A new IPC command also goes into `mcp/invoke.rs`:
its `TABLE`, or `EXCLUDED` with the reason.

**Changing export output.** Formats are pinned by golden files in
`crates/pe-polar/tests/golden/`. Change the golden file in the same commit and
say why in the commit message. GRIB output is checked with the test reader
and, in CI, ecCodes.

---

## Environment gotchas

- **Driving the app** (details in `docs/AGENT-UI-TESTING.md`): a driver
  script hands its answer to `done(…)` — never `window.__WEBDRIVER__.resolve`,
  and never longer than 30 s (either bricks the endpoint for the life of the
  process); start long work and poll. Stop a driven app by its process group
  or pid, **never `pkill -f pe-app`**: the person very likely has their own
  `tauri dev` running. Editing Rust restarts *their* `tauri dev` app too (its
  watcher rebuilds), so batch Rust edits.

- **Keep incremental compilation disabled.** The workspace dev profile sets
  `incremental = false` for ordinary `npm run dev` and tests. Rust 1.97.1 on
  Intel macOS can otherwise leave unresolved internal `.llvm.*` symbols in
  optimised workspace crates. Do not set `CARGO_INCREMENTAL=1`; the test driver
  and CI explicitly use `CARGO_INCREMENTAL=0`.

- The whole back end is Rust. No Python, Node or browser sidecars, and no
  headless browser for scraping. Tracker formats are decoded in Rust. The
  one JavaScript file in `pe-app`, `src/mcp/bridge.js`, is not run by the
  application: it is packed into the Claude Desktop extension and run by
  Claude Desktop's own Node. It may `require` only `node:` built-ins
  (nothing is installed beside it) and writes only JSON-RPC to stdout.
- The workspace builds `reqwest` with `rustls-no-provider`, so even a
  loopback test client needs a provider installed first
  (`rustls::crypto::ring::default_provider().install_default()`), as
  `tests/mcp.rs`'s `http()` does.
- Use `rustls` with the `ring` provider and `reqwest` blocking with
  `default-features = false` and the `rustls-no-provider` feature (reqwest's
  plain `rustls` feature pulls in `aws-lc-rs`, a C library). No OpenSSL, no
  `zarrs_http`. This keeps the Windows ARM64 and cross-compiled builds
  working.
- `zarrs` builds `libz-sys` as a build-time dependency of its build script
  only; it runs on the build host and is not linked into the app. That is
  the one accepted C build dependency. Anything linked into the app stays
  pure Rust.
- Blosc is decoded by our own pure-Rust decoder over `lz4_flex`; never pull
  `blosc-src` or any other C library.
- Geovoile hwx seeds differ per site. Parse them from the viewer HTML; never
  hard-code one set. They may be spread over several `data:image/png`
  sources (Route du Rhum 2018), and some sites answer an empty versions
  file (a version is only a cache-buster; 0 serves).
- The ARCO-ERA5 time axis is preallocated past today. Coverage comes from
  the root `.zattrs` (`valid_time_start`, `valid_time_stop`,
  `valid_time_stop_era5t`), and a 404 chunk means missing, not an error.
- ORC's service (`data.orc.org`, `DownRMS`) serves only the current VPP
  year: any other year, and any country code it does not know, answers no
  certificates and no error. Every answer carries the country list, opens
  with a UTF-8 byte-order mark, and is sent uncompressed (5–10 MB a
  country). Its `RefNo` is the certificate's identity, and the only one:
  a boat can hold two valid certificates in a year (crewed and
  double-handed), so never merge scraped certificates by boat. The embedded
  catalogue has no numbers, so a scraped certificate meets its embedded
  twin by boat, sail number and certificate year
  (`pe_orc::Catalogue::with_scraped`). Catalogue ids must stay put across
  scrapes: embedded entries keep their places, scraped ones are only ever
  appended, and one ORC has taken back is withdrawn in place, not removed.
- The YellowBrick `RaceSetup` JSON is ISO-8859-1, not UTF-8.
- Blue Water Tracks answers an unknown slug with HTTP 200, `race` an empty
  array rather than an object; check for that, not only for a 404.
- Never bundle credentials, device ids or cookies from reference repositories.

---

## Testing rules

- Assert against an independent reference, never "what the code currently
  does": hand-computed values, a published polar, a recorded vendor
  response, ecCodes for GRIB.
- Geodesy and heading derivation need antimeridian, polar and
  stationary-boat cases.
- Every document change needs a round-trip test and an undo inverse test.
- Parsers get malformed-input tests: they return an error naming the line and
  column, never panic.
- Performance changes need before and after numbers against spec §13.

## Style

- `thiserror` in library crates, `anyhow` only at the app boundary, a
  serialisable `AppError { kind, message }` over IPC.
- No `unwrap` or `expect` outside tests (clippy denies them). `unsafe_code` is
  forbidden workspace-wide.
- Doc comments explain why, not what.
- Frontend types that cross IPC are always generated by ts-rs.
- `ui/src/ipc.ts` is the only place `invoke` is called.

## Definition of done

fmt, clippy, all tests and `check:offline` pass. A change to the UI is
exercised in the real app with the driver: a UX test in
`tools/webdriver/ux/`, or a screenshot the agent has opened and looked at
(`docs/AGENT-UI-TESTING.md`). The relevant recipe is
followed. `spec.md` is updated in the same commit as any behaviour change.
Every new string is translated into all eight languages. Every new control is
in the help search. `plan.md` milestone status is updated.

## Ask, don't guess

Ask before: adding a network host, adding a C or C++ dependency, changing
the `.wpsproj` layout, changing an export format, or changing how the blend
weights sources.
