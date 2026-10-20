# Hindsight Sync as the reanalysis source — implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** PolarExplorer reads reanalysis weather from whichever archive the user picks out of the ones Hindsight Sync lists, and nothing else: Open Data and the three built-in Whirlwind endpoints go.

**Architecture:** `pe-env` keeps one reader, the sharded-Zarr reader that today reads the Whirlwind S3 archive, generalised to any location Hindsight Sync describes (an S3-compatible bucket with its credentials, or a folder) and to both of Hindsight Sync's layouts. `pe-env` also gains the loopback client that asks Hindsight Sync for its archive list. `pe-app` holds the settings (port, connected flag, the chosen archive's display details in `settings.json`; its location and credentials in a separate 0600 file), the probe/scan commands, and builds the provider from the remembered location. The React Settings dialog gets a **Reanalysis** section in three states.

**Tech Stack:** Rust (Tauri 2, reqwest blocking + async with rustls/ring, tokio, serde, ts-rs), React/TypeScript (Vite, vitest), the WebDriver UX harness under `tools/webdriver/`.

**Spec:** `docs/superpowers/specs/2026-10-19-hindsight-sync-design.md` (read it first; this plan argues from it). Project rules: `CLAUDE.md`, `spec.md` §3.4 and §7.5.

## Global Constraints

- Network crates stay `pe-env` and `pe-trackers`; `pe-app` keeps no HTTP client (the Hindsight Sync client lives in `pe-env`).
- `pe-env` may reach only `http://127.0.0.1:<port>` and the archive location Hindsight Sync handed it. The only literal hosts left in `pe-env` source are `127.0.0.1` and the AWS template `https://{bucket}.s3.{region}.amazonaws.com`.
- Hindsight Sync API: `GET http://127.0.0.1:<port>/v1/archives/cloud/reanalysis` and `/v1/archives/local/reanalysis`; answer `{ api_version: 1, archives: [...], errors: [...] }`; detection = HTTP 200 with `api_version: 1` within 2 s; no `Origin` header sent; no redirect followed.
- Default port `8765`; port range 1–65535. Scan order: saved port, `8765`, the port in `<BaseDirs::config_dir()>/com.historysyncer.desktop/settings.json` key `archive_api.port` (absent = 8765).
- The warning sentence, exactly: `PolarExplorer needs Hindsight Sync to download the reanalysis.` The link text `hindsight-sync.whirlwindai.com`, opening `https://hindsight-sync.whirlwindai.com` in the system browser, never fetched.
- Cloud archives are read only with the credentials Hindsight Sync hands out: an archive listed with `credentials: null` cannot be chosen (`set_reanalysis_archive` refuses it with its `credential_error`).
- Archive rows read `<name> — <place> — <chunk_degrees>° × <chunk_hours> h — <products joined by ", ">`; `place` is `endpoint_url`'s host, or `<bucket>.s3.<region>.amazonaws.com` when `endpoint_url` is null, or the folder path for local archives.
- Provenance (`DatasetRecord`): `name` = the archive identity (cloud: `uri` verbatim, e.g. `s3://…`, `r2://…`, `tigris://…`; local: `source_uri` when present, else the `file:` `uri`); `version` = `hindsight-layout-<layout_version>`; `has_tide = Some(true)`. No `.wpsproj` schema change.
- The reader accepts `historysyncer_layout_version` 2 (float16, the seven parameters) and 3 (float32, eleven parameters), locating `u10 v10 ucur vcur wave_direction wave_height wave_period` by name. Inner-chunk bound: at most `4 << 20` values (so 16 MiB decoded for float32); the disk cache entry cap and the S3 read limit follow (`16 << 20` bytes).
- Fetch with no archive chosen is refused: `Choose a reanalysis archive in Settings before fetching weather.` An authentication failure's message ends with `Press Scan in Settings to refresh the archive's credentials.`
- Every string through `t()`/`msg()` with fr, de, es, it, nl, zh, ja, ar translations; every control has `data-feature` and a registry entry; `ui/src/ipc.ts` is the only `invoke` caller; new commands go into `lib.rs`'s `generate_handler!`, `mcp/invoke.rs` `TABLE` and `mcp/tools/effects.rs`; `spec.md` changes land in the same commit as the behaviour; `CARGO_INCREMENTAL=0`; never `pkill -f pe-app`; commits end with `Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>`.
- **Test baseline at main (ede8a99), not this work's to fix:** `cargo test --workspace` already fails pe-app `every_history_label_is_listed_for_translation`, `the_fixed_project_exports_the_same_bytes_on_every_platform`, `the_frontend_knows_every_failure_reason` and six pe-polar format/golden tests; `npm run ui:test` fails 54 tests. The gate for every task is **no new failure and no new key in the i18n coverage tests' missing/unused lists**, compared by diffing the output before and after.

## Review Focus

1. Hindsight Sync answers but lists no reanalysis archive at all → the dropdown shows only *Choose an archive…* and a muted *Hindsight Sync lists no reanalysis archives.*; nothing throws. (Task 8 test.)
2. Hindsight Sync answers HTTP 200 with a non-JSON body, or `api_version: 2`, or a 403 `local_applications_only` → "not detected", no panic, no partial listing. (Task 3 test.)
3. The chosen cloud archive's credentials were rotated in Hindsight Sync → a fetch fails with the Scan hint; the next successful probe refreshes the remembered location so the fetch after it works. (Task 4 tests.)
4. The chosen local archive's folder was deleted → the fetch fails with an error naming the path; Settings still shows the archive. (Task 6 test.)
5. The remembered archive is no longer in Hindsight Sync's list → it stays chosen, shown as a last option marked *(no longer listed)*, and fetches still read it. (Task 8 test.)

---

### Task 1: Remove Open Data

Everything only the Open Data path used goes; the Whirlwind reader stays and becomes the one `Reanalysis`. The three fixed Whirlwind endpoints survive this task (they go in Task 4).

**Files:**
- Delete: `crates/pe-env/src/http.rs`, `blosc.rs`, `codec.rs`, `parallel.rs` (if nothing but the Open Data tiers used it), `crates/pe-env/tests/fixtures.rs`, `tests/ranges.rs`, `tests/sampler.rs`, `tests/live.rs`, `crates/pe-env/tests/fixtures/{arco-swh-chunk,cmems-merged-geo,currents-crop,wb2-crop}`
- Modify: `crates/pe-env/src/lib.rs`, `dataset.rs` (keep `Dataset` with only the three Whirlwind variants, `Cell`, and whatever `whirlwind/` still imports; delete the URLs, `Variable`, `vars`, `OpenVariable`, `Coverage`, the readers), `store.rs` (keep `TimeAxis` only), `sampler.rs` (delete `Access`, the tiers, `Reanalysis::{http,new,with_memory}`, `era5_*`, `estimate`'s Open Data body, `estimate_export`, `Area`, `ExportEstimate`; `Reanalysis` holds `whirlwind: Whirlwind` not `Option`), `net.rs` (keep `s3_client`, `tls_config`, `redirect_allowed`, `client`), `Cargo.toml` (drop `zarrs`, `zarrs_storage`, `lz4_flex`; add `bytes = "1"` and replace `zarrs_storage::Bytes` with `bytes::Bytes`), `whirlwind/mod.rs` (pull `NetStats`, `RETRIES` into `whirlwind/s3.rs` or a small `whirlwind/stats.rs`)
- Create: `crates/pe-env/src/whirlwind/testing.rs` — `#[cfg(any(test, feature = "testing"))] pub mod testing` exposing the fixture archive and the stub server now private in `whirlwind/tests.rs`; `[features] testing = []` in `pe-env/Cargo.toml`; `pe-app`'s `[dev-dependencies]` gets `pe-env = { workspace = true, features = ["testing"] }`
- Modify: `crates/pe-app/src/settings.rs` (`DataSource` loses `OpenData`; default `Whirlwind`), `env.rs` (`provider_for_source` always Whirlwind; `take` batches always; span `72 * 3600`, limit `10_000` always; `different_source` simplified), `grib.rs` (`preview_of` takes `&dyn Provider`-free input: see Produces; `automation_provider` and `PE_DRIVER_REANALYSIS` deleted, with the comment in `crates/pe-app/Cargo.toml:94`), `crates/pe-app/tests/env_jobs.rs` (every `Dataset::{Wb2Era5Hourly,ArcoEra5,CmemsNwsMy,GlobCurrentMy}` → `Dataset::WhirlwindHindsight`; `DataSource::OpenData` cases deleted), `tests/settings_commands.rs`, `tests/grib_export.rs` (the stub Open Data archives replaced by the `pe_env::whirlwind::testing` fixture served over the stub; the golden GRIB bytes regenerate because the fixture data changed, not the format: say so in the commit message)
- Modify: `tools/check-offline.sh` (`ENV_HOSTS` keeps only the three Whirlwind hosts), `ui/src/settings/SettingsDialog.tsx` (drop the `open_data` option and the sentence "Open Data downloads are kept in memory for this session."), `ui/src/i18n/locales/*/settings.ts` (drop `Open Data (Slow)` and retranslate the shortened sentence), `ui/src/help/features/settings.ts` (drop keyword "Open Data"), `ui/src/help/topics.ts` + `ui/src/help/locales/*.ts` (the `environment` topic's "Open Data: …" paragraph becomes one about the Whirlwind archive; the `settings` topic's "Data sources" row loses "Open Data uses the public archives."), `ui/src/App.test.tsx:268` (`data_source: "whirlwind"`), `tools/webdriver/ux/16-data-sources.test.mjs` (default `whirlwind`, three options), `spec.md` (§3.4 "Data sources" bullet: Open Data removed, default Whirlwind S3; the "Open Data downloads stay in memory…" and "Open Data does not use the disk cache" sentences; §7.5 "With Open Data (Slow) selected:" block and its table through the end of §7.5.1 deleted; §7.5's first Whirlwind paragraph no longer says "With Whirlwind (Fast) S3 selected")

**Interfaces:**
- Produces: `pe_env::Reanalysis::whirlwind(source, credentials, timeout, memory) -> Result<Reanalysis>` unchanged; `Reanalysis::estimate(&self, points: &[Point]) -> Result<Estimate>` (the Whirlwind accounting only); `pe_env::whirlwind::testing::{archive() -> Archive, fixture() -> BTreeMap<String, Vec<u8>>, serve(objects, status) -> Server, Server::endpoint()}` with `Server` holding the stub's base URL; `pe_app::grib::preview_of(source_id, label, plan, provider: &Reanalysis) -> Result<GribPreview>` computing its download figures from `provider.estimate(&points)` where `points` are the plan's grid nodes at each plan time, thinned uniformly to at most 200 000 points.
- Consumes: nothing new.

- [ ] **Step 1: Delete the Open Data readers and their tests, and make the crate compile**

Run: `CARGO_INCREMENTAL=0 cargo build -p pe-env && cargo clippy -p pe-env --all-targets -- -D warnings`
Expected: clean. `grep -rn "googleapis\|cloudferro\|Wb2Era5\|ArcoEra5\|Cmems\|GlobCurrent" crates/pe-env/src` prints nothing.

- [ ] **Step 2: Move the fixture builders and stub server into `whirlwind/testing.rs`; `whirlwind/tests.rs` uses them**

Run: `CARGO_INCREMENTAL=0 cargo test -p pe-env`
Expected: every remaining whirlwind test passes (the two `live_*` ignored tests stay ignored).

- [ ] **Step 3: Adapt `pe-app`: settings default, env.rs, grib.rs preview, tests**

Write first, in `crates/pe-app/tests/grib_export.rs`, the replacement for `provider(origin)`: a `Reanalysis::whirlwind(Source::S3, None, …)` pointed at a `testing::serve(testing::fixture(), 200)` stub (the reader's `#[cfg(test)]` endpoint override is not reachable from an integration test: give `testing` a `pub fn reader_at(endpoint: &str, …) -> Reanalysis` that sets it). Regenerate `crates/pe-app/tests/golden/*.grib2` goldens with the fixture and keep the ecCodes/test-reader checks.

Run: `CARGO_INCREMENTAL=0 cargo test -p pe-app --test grib_export --test env_jobs --test settings_commands`
Expected: pass.

- [ ] **Step 4: Trim the offline check, the UI option, the help text, the spec**

Run: `npm run check:offline && npm run ui:typecheck && npm run ui:test 2>&1 | tail -40`
Expected: offline passes; typecheck clean; `ui:test` shows the same 54 baseline failures and no new missing/unused key in the coverage lists (compare with a run on the previous commit).

- [ ] **Step 5: Full gates**

Run: `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && CARGO_INCREMENTAL=0 cargo test --workspace 2>&1 | grep -E "^test result|FAILED|failed"`
Expected: only the nine baseline failures.

- [ ] **Step 6: Commit**

```bash
git add -A crates/pe-env crates/pe-app tools/check-offline.sh ui spec.md
git commit -m "Remove Open Data: the Whirlwind reader is the one reanalysis reader

The GRIB export goldens change because the test archive is now the
Whirlwind fixture; the GRIB format is unchanged."
```

---

### Task 2: An archive location instead of three fixed endpoints

**Files:**
- Create: `crates/pe-env/src/whirlwind/location.rs`
- Delete: `crates/pe-env/src/whirlwind/source.rs`
- Modify: `crates/pe-env/src/whirlwind/mod.rs`, `s3.rs`, `cache.rs` (key by identity), `tests.rs`, `testing.rs`, `dataset.rs` (`Dataset` becomes the single variant `Hindsight`, `id() = "hindsight"`, `has_tide() = Some(true)`, no `url`/`version`), `sampler.rs` (`Reanalysis::open`), `crates/pe-app/src/settings.rs` (temporary: `DataSource::archive(self) -> ArchiveRef` builds the three fixed S3 locations; the R2 and Tigris credentials move here from `source.rs` for Task 4 to delete), `env.rs`, `tests/env_jobs.rs`, `tests/grib_export.rs`

**Interfaces:**
- Produces, in `pe_env::whirlwind`:
  ```rust
  #[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
  pub struct Credentials { access_key_id: String, secret_access_key: String, #[serde(default)] session_token: Option<String> }  // Debug prints "Credentials([redacted])"; `Credentials::new(id, secret) -> Self` is pub
  #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
  #[serde(tag = "protocol", rename_all = "lowercase")]
  pub enum ArchiveLocation {
      S3 { bucket: String, prefix: String, endpoint_url: Option<String>, region: String, force_path_style: bool, credentials: Option<Credentials> },
      File { path: PathBuf },
  }
  impl ArchiveLocation {
      /// The object root: no endpoint → `https://{bucket}.s3.{region}.amazonaws.com/{prefix}`;
      /// endpoint + path style → `{endpoint}/{bucket}/{prefix}`; endpoint without path style →
      /// `https://{bucket}.{endpoint host}/{prefix}`. A trailing `/` on prefix or endpoint is dropped. `File` → Err(EnvError::Open).
      pub fn base_url(&self) -> Result<String>;
      /// The place a row shows: the endpoint host (or the AWS host), or the folder path.
      pub fn place(&self) -> String;
  }
  #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
  pub struct ArchiveRef { pub identity: String, pub location: ArchiveLocation }
  pub fn Whirlwind::new(archive: ArchiveRef, timeout: Duration, memory: Arc<BlockCache>, concurrency: usize) -> Result<Whirlwind>
  pub fn Reanalysis::open(archive: ArchiveRef, timeout: Duration, memory: Arc<BlockCache>) -> Result<Reanalysis>   // replaces `whirlwind(…)`
  pub fn Reanalysis::archive(&self) -> &ArchiveRef
  ```
  Memory namespaces and `DiskCache::key` use `archive.identity` where they used `source.url()`. The S3 transport signs with the location's `region` when credentials are present; the `#[cfg(test)] endpoint` override goes: tests pass `endpoint_url: Some(stub)` with `force_path_style: true`, and `testing::serve` strips `/{bucket}/{prefix}` from request paths. `ArchiveLocation`'s JSON is exactly Hindsight Sync's `location` object plus the tag, so `serde_json::from_value(api["location"])` parses it (S3 objects there carry no `protocol: "s3"`? They do: `"protocol": "s3"` and `"protocol": "file"`, see `/Users/jon/HistorySyncer/src/api/catalog.rs`).
- Consumes: Task 1's `testing` module.

- [ ] **Step 1: Write the failing tests in `whirlwind/tests.rs`**

```rust
#[test] fn base_url_follows_the_provider_rules() {
    // AWS: endpoint None, region "us-east-1", bucket "whirlwind-hindsight", prefix "hindsight-routing"
    //   → "https://whirlwind-hindsight.s3.us-east-1.amazonaws.com/hindsight-routing"
    // R2: endpoint "https://acct.r2.cloudflarestorage.com", path style → "https://acct.r2.cloudflarestorage.com/whirlwind-hindsight/hindsight-routing"
    // File → Err
}
#[test] fn signed_reads_carry_the_location_region_and_never_the_secret() { /* stub records the Authorization header: contains "/auto/s3/aws4_request" for region "auto"; no header value contains the secret */ }
#[test] fn two_identities_never_share_cached_chunks() { /* same bytes served at two locations with different identities: second reader downloads again (stub request count) */ }
#[test] fn the_location_json_round_trips_hindsight_syncs_shape() { /* parse the S3 and File JSON shapes from the design's API section; serialise back equal */ }
```

- [ ] **Step 2: Run them to see them fail**

Run: `CARGO_INCREMENTAL=0 cargo test -p pe-env base_url_follows signed_reads_carry two_identities the_location_json`
Expected: compile errors (no `ArchiveLocation`).

- [ ] **Step 3: Implement `location.rs`, rewire `s3.rs`, `mod.rs`, `cache.rs`, `sampler.rs`, delete `source.rs`**

- [ ] **Step 4: Adapt `pe-app` to `ArchiveRef` (temporary `DataSource::archive`), fix its tests**

In `env.rs`, `record` writes `DatasetRecord { name: archive.identity, version: "hindsight-v3" /* until Task 4 */, has_tide: Some(true) }` and `different_source(meta, identity: &str)` is "any recorded name differs".

- [ ] **Step 5: Gates**

Run: `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && CARGO_INCREMENTAL=0 cargo test -p pe-env -p pe-app 2>&1 | grep -E "^test result|FAILED|failed" && npm run check:offline`
Expected: only the three pe-app baseline failures; offline passes (the three Whirlwind hosts are now in `pe-app/src/settings.rs`, so `check-offline.sh` layer 1 flags them: move them to `ENV_HOSTS`'s sibling exception for that one file with a comment "temporary until Hindsight Sync supplies locations (Task 4)").

- [ ] **Step 6: Commit**

```bash
git add -A crates/pe-env crates/pe-app tools/check-offline.sh
git commit -m "Read any Hindsight Sync archive location, not three fixed endpoints"
```

---

### Task 3: The Hindsight Sync client

**Files:**
- Create: `crates/pe-env/src/hindsight.rs` (+ `pub mod hindsight;` in `lib.rs`)
- Test: inline `#[cfg(test)] mod tests` in `hindsight.rs`, serving recorded JSON on a `TcpListener` stub (reuse `testing::serve`'s thread pattern, or a ten-line one of its own)

**Interfaces:**
- Produces:
  ```rust
  pub const DEFAULT_PORT: u16 = 8765;
  #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)] #[serde(rename_all = "lowercase")]
  pub enum Storage { Cloud, Local }
  #[derive(Debug, Clone, PartialEq, Eq)]
  pub struct ListedArchive { pub id: String, pub name: String, pub storage: Storage, pub uri: String, pub source_uri: Option<String>,
      pub mode: String, pub start: String, pub stop: Option<String>, pub layout_version: u32, pub chunk_degrees: u32, pub chunk_hours: u32,
      pub products: Vec<String>, pub location: ArchiveLocation }
  #[derive(Debug, Clone, PartialEq, Eq)]
  pub struct Unavailable { pub id: String, pub name: Option<String>, pub storage: Storage, pub reason: String }
  #[derive(Debug, Clone, Default, PartialEq, Eq)]
  pub struct Listing { pub archives: Vec<ListedArchive>, pub unavailable: Vec<Unavailable> }
  /// Both endpoints at `http://127.0.0.1:{port}`. `Ok(None)` when not detected (refused, timed out, not 200,
  /// no `api_version: 1`, unparsable). Cloud first, then local, each in the server's order. An `errors` entry,
  /// and a cloud archive whose `credentials` is null, go to `unavailable` with its `error` / `credential_error`.
  /// Fields the API does not send are missing data, never a panic; unknown fields are ignored.
  pub fn probe(port: u16, timeout: Duration) -> Result<Option<Listing>>
  impl ListedArchive { pub fn identity(&self) -> &str /* source_uri or uri */; pub fn place(&self) -> String { self.location.place() } }
  ```
  The client is a `reqwest::blocking::Client` built in `hindsight.rs` with `redirect(Policy::none())`, no default headers, the crate's `tls_config` not needed (plain HTTP). It never sends `Origin`.
- Consumes: `ArchiveLocation` (Task 2).

- [ ] **Step 1: Write the failing tests**

Use the shapes in `/Users/jon/HistorySyncer/src/api/catalog.rs` and its `tests.rs`: one cloud S3 archive (endpoint null, region `us-east-1`, credentials present), one R2 archive (endpoint set, `force_path_style: true`, region `auto`), one cloud archive with `credentials: null` and `credential_error: "no AWS key"`, one local archive (`protocol: "file"`, `source_uri: "s3://weather/source"`), one `errors` entry `{ id, storage: "cloud", error: "…" }`.

```rust
#[test] fn a_listing_keeps_cloud_then_local_and_sends_no_origin()      // 4 archives → 3 listed + 1 unavailable (credentials) + 1 unavailable (errors); request headers hold no "origin"
#[test] fn identity_prefers_the_local_copys_source()                   // local: identity() == "s3://weather/source"; cloud: == uri
#[test] fn not_detected_when_refused_wrong_version_or_not_json()       // closed port → Ok(None); {"api_version":2} → Ok(None); "<html>" → Ok(None); 403 {"error":"local_applications_only"} → Ok(None)
#[test] fn a_slow_server_is_not_detected_within_the_timeout()           // stub sleeps 3 s; probe(port, 500 ms) → Ok(None) in under 2 s
```

- [ ] **Step 2: Run to see them fail** — `CARGO_INCREMENTAL=0 cargo test -p pe-env hindsight` → compile error.

- [ ] **Step 3: Implement `hindsight.rs`**

- [ ] **Step 4: Run to see them pass; clippy clean**

- [ ] **Step 5: Commit** — `git commit -m "Ask Hindsight Sync on loopback for its reanalysis archives"`

---

### Task 4: Reanalysis settings, probe and scan commands, the provider from the remembered archive

This task deletes `DataSource` and the temporary fixed endpoints, and makes the fetch read the remembered archive.

**Files:**
- Create: `crates/pe-app/src/reanalysis.rs` (`pub mod reanalysis;` in `lib.rs`)
- Modify: `crates/pe-app/src/settings.rs` (`data_source` → `reanalysis: ReanalysisSettings`, read field by field in `load`; `set_data_source`/`data_source_set` deleted), `commands.rs` (`AppServices.reanalysis_listing: Mutex<Option<pe_env::hindsight::Listing>>`), `env.rs` (`Task.archive: Arc<CapturedArchive>`, provider key, provenance, refusal), `grib.rs` (same provider), `error.rs` (`NoReanalysisArchive`), `lib.rs` (handlers + `reanalysis::on_startup(app.handle().clone())` beside `catalogues::on_startup`), `mcp/invoke.rs` (TABLE: `reanalysis_scan`, `reanalysis_probe`, `set_reanalysis_port`, `set_reanalysis_archive`; `set_data_source` removed), `mcp/tools/effects.rs`, `examples/export_bindings.rs`, `ui/src/errors.ts` (the new kind), `tools/check-offline.sh` (drop the Task 2 temporary exception; `ENV_HOSTS` becomes `^https?://(127\.0\.0\.1|\{bucket\}\.s3\.\{region\}\.amazonaws\.com)`), `spec.md` §3.4 (the **Reanalysis** bullet replacing **Data sources**, written from the design's "Settings → Reanalysis" and "Settings model" sections) and §7.5 (the opening paragraph: "the chosen archive" in place of "With Whirlwind (Fast) S3 selected", provenance values, the refusal)
- Tests: `crates/pe-app/tests/reanalysis.rs` (new), `tests/settings_commands.rs`, `tests/env_jobs.rs`

**Interfaces:**
- Produces (all `#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]`, exported to `ui/src/generated/`):
  ```rust
  pub struct ReanalysisSettings { pub port: u16, pub connected_once: bool, pub archive: Option<RememberedArchive> }   // Default { 8765, false, None }
  pub struct RememberedArchive { pub id: String, pub name: String, pub storage: ArchiveStorage, pub uri: String, pub source_uri: Option<String>,
      pub mode: String, pub start: String, pub stop: Option<String>, pub layout_version: u32, pub chunk_degrees: u32, pub chunk_hours: u32,
      pub products: Vec<String>, pub place: String }
  pub enum ArchiveStorage { Cloud, Local }   // serde + ts "cloud" | "local"
  pub struct UnavailableArchive { pub id: String, pub name: Option<String>, pub storage: ArchiveStorage, pub reason: String }
  pub struct ScanResult { pub detected: bool, pub port: u16, pub archives: Vec<RememberedArchive>, pub unavailable: Vec<UnavailableArchive> }
  ```
  Not exported, in `reanalysis.rs`:
  ```rust
  /// `reanalysis-archive.json` in the config dir, mode 0600: `{ "id": …, "location": ArchiveLocation }`. Written by `set_reanalysis_archive`
  /// and by a probe that lists the chosen id (credentials rotate); removed when the choice is cleared.
  pub struct LocationFile { pub id: String, pub location: ArchiveLocation }
  pub fn location_path(paths: &AppPaths) -> PathBuf
  /// The settings' archive and its location, or the refusal.
  pub fn captured(state: &AppState) -> Result<CapturedArchive>   // Err(AppError::NoReanalysisArchive { reason }) when none chosen or the location file is missing/mismatched
  #[derive(Debug, Clone, PartialEq)] pub struct CapturedArchive { pub archive: RememberedArchive, pub location: ArchiveLocation }
  impl CapturedArchive { pub fn archive_ref(&self) -> ArchiveRef /* identity = source_uri or uri */; pub fn version(&self) -> String /* "hindsight-layout-{n}" */ }
  pub fn probe_now(state: &AppState) -> Result<ScanResult>            // the saved port only; on detection: connected_once = true (saved), listing stored, chosen id's location refreshed and written
  pub fn scan_now(state: &AppState) -> Result<ScanResult>             // saved port, then 8765, then sibling_port(); the first detected port is saved
  pub fn sibling_port() -> Option<u16>                                 // Hindsight Sync's own settings file; None when unreadable/absent; absent key → Some(8765)
  pub fn port_set(state: &AppState, port: u16) -> Result<Settings>     // 1..=65535 else AppError::BadOption { field: "Port", .. }; then probe_now(state) is attempted and its failure ignored
  pub fn archive_set(state: &AppState, id: Option<String>) -> Result<Settings>   // from the stored listing; an id not listed → AppError::BadOption { field: "Reanalysis archive", value: id }; an id in `unavailable` → AppError::Doing { doing: "choose", what: name, why: reason }
  pub fn on_startup(app: AppHandle)                                    // a thread calling probe_now, emitting "reanalysis://changed" with the ScanResult when detected
  #[tauri::command] pub fn reanalysis_scan(state) -> Result<ScanResult>
  #[tauri::command] pub fn reanalysis_probe(state) -> Result<ScanResult>
  #[tauri::command] pub fn set_reanalysis_port(state, port: u16) -> Result<Settings>
  #[tauri::command] pub fn set_reanalysis_archive(state, id: Option<String>) -> Result<Settings>
  ```
  `AppError::NoReanalysisArchive { reason: String }` → kind `no-reanalysis-archive`, message = reason; reasons: `Choose a reanalysis archive in Settings before fetching weather.` and `The remembered archive's location is missing. Press Scan in Settings.` A `pe_env::EnvError` from a fetch whose text contains `403` or `Forbidden` or `AccessDenied` gets ` Press Scan in Settings to refresh the archive's credentials.` appended in `env.rs`'s outcome mapping.
  `crate::env::provider(state)` builds `Reanalysis::open(captured.archive_ref(), timeout, memory)?.with_whirlwind_cache(…)` for cloud archives (no disk cache for `File` locations); `ProviderKey` carries the `CapturedArchive`.
- Consumes: Tasks 2 and 3.

- [ ] **Step 1: Write the failing tests in `crates/pe-app/tests/reanalysis.rs`**

Serve a recorded Hindsight Sync listing from a loopback stub (as in Task 3) on a free port; the test sets that port with `port_set`.

```rust
#[test] fn a_probe_lists_archives_marks_connected_and_remembers_the_port()   // probe_now: detected, connected_once true in the saved file, archives ordered cloud then local
#[test] fn choosing_writes_display_details_to_settings_and_the_location_to_its_own_file()
    // settings.json contains id/name/place/chunk fields and NO "access_key_id"/"secret_access_key"; reanalysis-archive.json contains them; unix mode 0600
#[test] fn an_unlisted_or_credential_less_archive_is_refused()               // BadOption / Doing
#[test] fn a_later_probe_refreshes_the_chosen_locations_credentials()        // stub changes secret → probe_now → file holds new secret
#[test] fn scan_tries_the_saved_port_then_the_default_then_the_sibling()      // saved port closed, 8765 closed (use a stub elsewhere), sibling file written in a temp HOME-equivalent: the test injects the sibling path via a `pub fn scan_with(state, sibling: Option<u16>)` so no real home dir is read
#[test] fn not_running_keeps_the_remembered_archive_usable()                 // probe → Ok(ScanResult{detected:false,..}); captured(state) still Ok
#[test] fn fetch_without_an_archive_is_refused_in_words()                    // queue_fetch → Err kind "no-reanalysis-archive", message == the sentence
#[test] fn provenance_is_the_identity_and_layout_version()                    // Fake provider drain; DatasetRecord { name: "s3://weather/source" for the local copy, version: "hindsight-layout-3", has_tide: Some(true) }
#[test] fn the_ipc_settings_json_never_holds_a_secret()                      // serde_json::to_string(&settings::current(&state)) contains no key id / secret
#[test] fn a_forbidden_read_fails_the_fetch_with_the_scan_hint()               // stub answers 403 to shard reads; drain → Outcome::Failed(msg) with msg ending "Press Scan in Settings to refresh the archive's credentials."
```
And in `tests/settings_commands.rs`: replace the two data-source tests with one `every_setting_survives_a_restart` that sets the port to 9001 and asserts it back; in `tests/env_jobs.rs`: `changing_data_source_…` becomes `changing_the_archive_replaces_existing_values_and_keeps_queued_choice` using two `CapturedArchive`s with different identities installed through `archive_set` against a stub listing two archives.

- [ ] **Step 2: Run to see them fail** — `CARGO_INCREMENTAL=0 cargo test -p pe-app --test reanalysis` → compile error.

- [ ] **Step 3: Implement `reanalysis.rs`, the settings change, env.rs, error.rs, the registrations, bindings**

Run `npm run bindings` after the types compile.

- [ ] **Step 4: Run the tests; then every gate**

Run: `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && CARGO_INCREMENTAL=0 cargo test --workspace 2>&1 | grep -E "^test result|FAILED|failed" && npm run check:offline && npm run ui:typecheck`
Expected: only the nine baseline failures; `grep -rn "DataSource\|data_source\|whirlwind_source\|Source::S3" crates ui/src --exclude-dir=generated` prints nothing (the `ui/src/generated/DataSource.ts` file is deleted by `npm run bindings`? ts-rs does not delete: remove it by hand).

- [ ] **Step 5: Commit** — `git commit -m "Read weather from the archive chosen among Hindsight Sync's, and remember its location"`

---

### Task 5: Accept both Hindsight Sync layouts

**Files:**
- Modify: `crates/pe-env/src/whirlwind/layout.rs` (`Array { …, bytes_per_value: usize }`, `parse` accepts `"float16"` | `"float32"`, any `shape[1]` equal to the `param` axis length; `parameters()` reads the axis length from `meta["shape"]` instead of `expected.len()`; the bound `inner.product() > 4 << 20` values; `decoded_len = product × bytes_per_value`), `mod.rs` (value extraction by `bytes_per_value`; `index_entry` len bound `16 << 20`), `s3.rs` (read limit `16 << 20`), `cache.rs` (`MAX_ENTRY = (16 << 20) + OVERHEAD`), `testing.rs` (`fixture_with(dtype: Dtype, names: &[&str]) -> BTreeMap<..>`; `fixture()` stays the seven-float16 one), `tests.rs`, `spec.md` §7.5 (the sentence "Values are float16" becomes "Values are float16 (layout 2) or float32 (layout 3), among seven or eleven parameters, located by name")

**Interfaces:**
- Produces: `testing::fixture_with(Dtype::F32, &["mslp","u10","z500","v10","ucur","vcur","wave_direction","wave_height","wave_period","wind_speed","wind_constancy"])`.
- Consumes: Task 2's reader.

- [ ] **Step 1: Failing tests in `whirlwind/tests.rs`**

```rust
#[test] fn a_layout_3_archive_samples_the_same_values_as_layout_2()   // same truth values from the f32/11-parameter fixture as from the f16/7 one
#[test] fn an_archive_missing_a_needed_parameter_is_refused_by_name()  // names without "wave_period" → Err containing "lacks parameter wave_period"
#[test] fn the_inner_chunk_bound_counts_values_not_bytes()            // inner [72, 11, 40, 80] f32 (10.1 MB) parses; inner [96, 11, 80, 80] (6.7M values) is refused
#[test] fn a_float64_archive_is_refused()
```

- [ ] **Step 2: Run to see them fail** — `cargo test -p pe-env layout_3 missing_a_needed inner_chunk_bound float64`.

- [ ] **Step 3: Implement**

- [ ] **Step 4: Run; `cargo test -p pe-env`; clippy** — Expected: pass; the existing `corrupt_indexes_and_decoded_sizes_are_refused` still passes with the new bound.

- [ ] **Step 5: Commit** — `git commit -m "Read Hindsight Sync layout 3: float32 and eleven parameters, located by name"`

---

### Task 6: Local folder archives

**Files:**
- Create: `crates/pe-env/src/whirlwind/file.rs`
- Modify: `crates/pe-env/src/whirlwind/mod.rs` (a `Transport` enum `{ S3(S3), File(FileStore) }` with `get`, `get_versioned`, `stats`; `Whirlwind::new` picks by location; `with_disk_cache` is a no-op for `File`), `tests.rs`, `spec.md` §7.5 (one sentence: a local archive is read from its folder with the same byte ranges and no disk cache)

**Interfaces:**
- Produces: `FileStore::new(root: &Path) -> Result<FileStore>` (Err(Open) naming the path when it is not a readable directory); `FileStore::get(key, range, cancel) -> Result<Option<Object>>` reading `root/key` with `seek` + `read_exact` for `Range::Bytes`, the tail for `Range::Suffix`, the whole file otherwise, bounded by the same `16 << 20` limit; `etag: None`; a missing file → `Ok(None)`.
- Consumes: Task 2's `ArchiveLocation::File`.

- [ ] **Step 1: Failing tests** — `fixture()` written into a temp dir (`testing::write_fixture(dir)`):

```rust
#[test] fn a_folder_archive_samples_like_the_served_one()
#[test] fn a_missing_shard_file_is_missing_data_and_a_vanished_folder_fails_the_open_with_its_path()
#[test] fn folder_archives_use_memory_but_never_the_disk_cache()   // with_disk_cache set: cache dir stays empty after a sample
```

- [ ] **Step 2: Run to see them fail.** — [ ] **Step 3: Implement.** — [ ] **Step 4: Run, clippy.** — [ ] **Step 5: Commit** — `git commit -m "Read local Hindsight Sync archives from their folder"`

---

### Task 7: The product link and the dialog-open probe

**Files:**
- Modify: `crates/pe-app/src/reanalysis.rs` (`pub const SITE: &str = "https://hindsight-sync.whirlwindai.com"; #[tauri::command] pub fn open_hindsight_sync_site() -> Result<()>` — macOS `open <url>`, Windows `cmd /C start "" <url>` with `CREATE_NO_WINDOW`, Linux `xdg-open <url>`, following `mcp/desktop.rs`'s `launch`; the command takes no argument), `lib.rs`, `mcp/invoke.rs` (`EXCLUDED` with reason "opens the person's browser"), `tools/check-offline.sh` (`ALLOW` gains `hindsight-sync\.whirlwindai\.com` with a comment: opened by the browser on request, never fetched)
- Test: `crates/pe-app/src/reanalysis.rs` unit test that `SITE` is the exact URL and that the command's argument list for the current platform is `[.., SITE]` (factor `fn launch_command(url) -> std::process::Command` and inspect `get_args()`).

- [ ] **Step 1: Failing test.** — [ ] **Step 2: Implement.** — [ ] **Step 3: `npm run check:offline`, clippy, test.** — [ ] **Step 4: Commit** — `git commit -m "Open the Hindsight Sync site from Settings"`

---

### Task 8: The Reanalysis section in Settings

**Files:**
- Create: `ui/src/settings/Reanalysis.tsx`, `ui/src/settings/Reanalysis.test.tsx`, `ui/src/assets/hindsight-sync.png` (copied from `/Users/jon/HistorySyncer/ui/assets/hindsight-sync.png`)
- Modify: `ui/src/settings/SettingsDialog.tsx` (the `settings:data-source` select and its surrounding label go; `<section data-section="settings:reanalysis">` with `<Reanalysis …/>` placed where **Data sources** was, inside/above "Downloaded weather" as the design says; `onReveal("settings:reanalysis", show)`; "Whirlwind cache" → "Reanalysis cache"; "Whirlwind downloads up to 64 chunks at once." → "Reanalysis downloads up to 64 chunks at once."), `WeatherCache.tsx` (its two Whirlwind sentences → "Reanalysis keeps downloaded chunks…", "Remove cached reanalysis downloads. Weather saved in projects is kept."), `ui/src/ipc.ts` (`reanalysisScan: () => call<ScanResult>("reanalysis_scan")`, `reanalysisProbe`, `setReanalysisPort(port)`, `setReanalysisArchive(id: string | null)`, `openHindsightSyncSite`; `setDataSource` deleted; `reanalysis_scan` in `LONG_RUNNING`), `ui/src/help/features/settings.ts` (`settings:data-source` deleted; `settings:reanalysis-archive`, `settings:reanalysis-scan`, `settings:reanalysis-port`, `settings:reanalysis-link` added with `reveal: open("reanalysis")`; the cache entries' keyword "Whirlwind" → msg("reanalysis")), `ui/src/App.tsx` (listen for `reanalysis://changed` and refresh settings, the way `document://changed` is handled), `ui/src/App.test.tsx` (settings fixture: `reanalysis: { port: 8765, connected_once: false, archive: null }`), `ui/src/i18n/locales/{fr,de,es,it,nl,zh,ja,ar}/settings.ts`, `ui/src/styles.css` (`.reanalysis-warning` with the 48 px logo beside the sentence; the link styled as the app's links)

**Interfaces:**
- Consumes: Task 4's types and commands; the `ScanResult` event payload.
- Produces: the controls above. English keys (exact): `Reanalysis`, `PolarExplorer needs Hindsight Sync to download the reanalysis.`, `Hindsight Sync logo`, `Scan`, `Scanning…`, `Port`, `Reanalysis archive`, `Choose an archive…`, `Cloud`, `This computer`, `(no longer listed)`, `Hindsight Sync is not running. The remembered archive is still used for fetches.`, `Hindsight Sync is not running. No archive is chosen.`, `Connected to Hindsight Sync on port {port}`, `Hindsight Sync lists no reanalysis archives.`, `{start} to {stop}`, `from {start}, ongoing`, `Look for Hindsight Sync on this computer`, `The port Hindsight Sync's archive API listens on (1–65535)`, `Reanalysis cache`, `Reanalysis downloads up to 64 chunks at once.`, `Reanalysis keeps downloaded chunks on disk for reuse across routes and app restarts. The least recently used chunks are removed when the cache is full.`, `Remove cached reanalysis downloads. Weather saved in projects is kept.` The link text `hindsight-sync.whirlwindai.com` is not translated. Row text is built in code, not translated.

Behaviour: on mount call `api.reanalysisProbe()`; keep the last `ScanResult` in state; the dropdown's value is `settings.reanalysis.archive?.id ?? ""`; options from `result.archives` in two `<optgroup>`s, `result.unavailable` as `disabled` options titled with `reason`; the remembered archive absent from `result.archives` appended last with ` (no longer listed)`. State selection: `!result.detected && !settings.reanalysis.connected_once` → warning; `!result.detected` → quiet line; else the dropdown block. Scan disables the button and shows *Scanning…* until the promise settles. The port field is an `IntegerField` saved on change.

- [ ] **Step 1: Failing vitest tests in `Reanalysis.test.tsx`** (mock `api` as `App.test.tsx` does):

```ts
test("never connected and not detected shows the warning with the logo, the sentence and the link")
test("connected before and not detected shows the quiet line and keeps the remembered archive")   // Review Focus 5 variant
test("detected lists cloud then local, each row name — place — chunking — products, unavailable rows disabled with their reason")
test("choosing an archive calls set_reanalysis_archive with its id and the muted line shows its range")
test("a remembered archive missing from the list is appended as (no longer listed) and stays selected")
test("an empty listing shows the no-archives line")
test("scan disables the button while it runs and the port field saves on change")
test("the link opens the site through ipc, never through window.open")
```

- [ ] **Step 2: Run to see them fail** — `npm run ui:test -- Reanalysis`.

- [ ] **Step 3: Implement the component, the dialog changes, ipc, registry, translations, styles**

- [ ] **Step 4: Gates**

Run: `npm run ui:typecheck && npm run ui:test 2>&1 | tail -60 && npm run check:offline`
Expected: the new tests pass; the baseline 54 failures and no new coverage key (diff the missing/unused lists against the previous commit); `features.test.ts` passes for the new ids.

- [ ] **Step 5: Commit** — `git commit -m "Settings: the Reanalysis section, with Hindsight Sync detection and the archive list"`

---

### Task 9: Help, user guide, glossary

**Files:**
- Modify: `ui/src/help/topics.ts` (`settings` topic: the "Data sources" row → a "Reanalysis" row written from the design's three states; "Whirlwind cache" row → "Reanalysis cache"; the `environment` topic's first paragraph: "The reanalysis archive chosen in Settings, listed by Hindsight Sync, is read…"), `ui/src/help/locales/{fr,de,es,it,nl,zh,ja,ar}.ts` (the same rows and paragraph translated), `docs/USER-GUIDE.md` (under "Fetch weather for tracks", a short paragraph: weather needs Hindsight Sync; Settings → Reanalysis; Scan; the archive list; what a row shows; the warning disappears once connected), `ui/src/i18n/GLOSSARY*.md` (rows: Hindsight Sync (a product name, not translated), reanalysis archive, Scan)

- [ ] **Step 1: Write the topic text and translations; run `npm run ui:test -- topics help` and the coverage tests**
- [ ] **Step 2: Commit** — `git commit -m "Describe the Reanalysis settings in the help and the user guide"`

---

### Task 10: The UX test against a fake Hindsight Sync

**Files:**
- Delete: `tools/webdriver/ux/16-data-sources.test.mjs`
- Create: `tools/webdriver/ux/16-reanalysis.test.mjs`
- Modify: `tools/webdriver/ux/harness.mjs` (a `fakeHindsightSync(archives)` helper: a Node `http` server on `127.0.0.1:0` answering the two paths with a recorded listing, refusing an `Origin` header with 403 like the real one, returning `{ port, close() }`), `docs/AGENT-UI-TESTING.md` (one line naming the helper)

- [ ] **Step 1: Write the test**

The test: open Settings → the warning state (logo element present, sentence text, link) → screenshot `warning`; start the fake, type its port into `settings:reanalysis-port`, press `settings:reanalysis-scan`, wait for `[data-feature="settings:reanalysis-archive"] option` count = 1 + archives → screenshot `detected`; pick the local archive, assert the muted range line and `settings.json`'s `reanalysis.archive.id`, and that `reanalysis-archive.json` exists and `settings.json` holds no `secret_access_key` → close the fake, press Scan, wait for the quiet "not running" line (no logo) → screenshot `not-running`; import `tools/webdriver/fixtures/supplied-wind.csv` and press `tracks:fetch-weather` with the local folder absent: the status line names the folder path (Review Focus 4 end to end).

- [ ] **Step 2: Run it** — `npm run ux -- reanalysis` → passes; open the three screenshots and check the warning shows the logo beside the sentence.
- [ ] **Step 3: Run the neighbours** — `npm run ux -- weather` and `npm run ux -- weather-cache` still pass.
- [ ] **Step 4: Commit** — `git commit -m "Drive the Reanalysis settings end to end against a fake Hindsight Sync"`

---

### Task 11: Spec, CLAUDE.md, plan.md, the design's amendments

**Files:**
- Modify: `spec.md` (a read-through of every mention of Whirlwind as a *source* — §3.4 cache bullet names, §7.5, §7.7, §12/§13 if any — against the design; invariant-4 wording in the spec's own invariants section), `CLAUDE.md` (invariant 4: `pe-env` reads Hindsight Sync's loopback API and the archive location it hands out; the layout table's `pe-env` line; the "Data sources" words anywhere), `plan.md` (decisions log row `D35`: Hindsight Sync is the one source of reanalysis archives, Open Data and the fixed endpoints removed, credentials remembered in a 0600 file beside settings; milestone `M28 — Hindsight Sync` marked complete with today's date), `docs/superpowers/specs/2026-10-19-hindsight-sync-design.md` (two amendments recorded as plan decisions: the location file `reanalysis-archive.json` in place of "settings.json is the only place"; the transport keeps `credentials: Option` and the chooser enforces the rule)

- [ ] **Step 1: Make the edits; `grep -n "Open Data\|Whirlwind (Fast)\|data_source\|Data sources" spec.md CLAUDE.md plan.md docs/USER-GUIDE.md ui/src/help` prints nothing but history lines in plan.md**
- [ ] **Step 2: Full gates one last time** — fmt, clippy, `cargo test --workspace` (nine baseline failures only), `ui:typecheck`, `ui:test` (54 baseline), `ui:perf`, `check:offline`, `tools:test`.
- [ ] **Step 3: Commit** — `git commit -m "Specify Hindsight Sync as the reanalysis source and record D35 / M28"`
