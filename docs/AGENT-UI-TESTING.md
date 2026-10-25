# Driving PolarExplorer from an agent

Any Claude session or subagent working in this repository can start the real
application — the Tauri shell, the Rust back end and the system WebKit
webview — click and type into it, read it, photograph it, and run scripted
UX tests, with no human in the loop (D25). This page says how, and what
bites.

## What is running

`npm run dev:webdriver` is `tauri dev --features webdriver`. The feature
compiles `tauri-plugin-webdriver-automation` into `pe-app`: a small REST
endpoint on `127.0.0.1` at a random port, announced on stdout as
`[webdriver] listening on port N`. It is **never shipped** (CLAUDE.md
invariant 4; `crates/pe-app/tests/webdriver_optional.rs`).

Everything in `tools/webdriver/` talks to it through one client,
`client.mjs`. A driven run:

- runs the **development build** (Vite, React StrictMode), because that is
  where bugs like a8cf1dd's hanging tracker dialog live;
- starts Vite on **its own free port** (`PE_DEV_PORT`) with its own
  pre-bundle cache (`node_modules/.vite-driver`), so it starts beside the
  person's own `tauri dev` on 5173 without touching it;
- uses **its own data root** (`PE_AUTOMATION_ROOT`, a fresh temporary
  directory, removed afterwards): settings, recent projects, autosave and
  the old chunk cache never touch the person's own;
- opens a **1440 × 900 window that does not take focus**, so pictures
  compare across machines and the person's keyboard is not stolen. It does
  appear on screen.

## Three ways in

**The MCP server** (`pe-driver`, registered in `.mcp.json`): tools
`screenshot`, `click`, `type`, `key`, `text`, `wait`, `dialog`, `evaluate`,
`open`, `stop`. The app starts on the first call and is held until `stop`;
`stop` during a start cancels it, and the client going away stops whatever
is running or starting (`holder.mjs`), so no detached app is left behind.
MCP servers are loaded when a Claude session starts, so a session that added
or changed the server cannot use it until it restarts — use the CLI
meanwhile.

**The CLI**, for a shell:

```bash
node tools/webdriver/cli.mjs shot start            # start, capture, stop; prints the PNG path
node tools/webdriver/cli.mjs serve > /tmp/pe.port & # start and hold; prints the port
export PE_DRIVER_PORT=$(cat /tmp/pe.port)
node tools/webdriver/cli.mjs click '[data-feature="new:create"]'
node tools/webdriver/cli.mjs wait '[data-feature="stage:3d"]'
node tools/webdriver/cli.mjs dialog "$PWD/polar_examples/polars/Farr 40.txt"
node tools/webdriver/cli.mjs click '[data-feature="polar-files:import"]'
node tools/webdriver/cli.mjs type '[data-feature="shell:search"]' 'Fit the world'
node tools/webdriver/cli.mjs text '.polar-file-list li'
node tools/webdriver/cli.mjs eval 'done(document.title)'
node tools/webdriver/cli.mjs shot after-import     # -> target/ux-shots/cli/after-import.png
kill %1                                            # stops the held app, its Vite and its data root
```

`PE_DRIVER_OPEN=path.wpsproj` opens a project before `shot`;
`PE_DRIVER_VERBOSE=1` prints the app's log.

**The UX suite**, `npm run ux` (or `npm run ux -- help tracker` for tests
whose file names contain those words). It builds once, then gives every test
a fresh application and data root. Pictures land in
`target/ux-shots/<test>/NN-step.png`; a failing test also leaves
`failure.png`. About 4–5 minutes for the seven tests on an Intel i9, almost
all of it application start-up.

## Finding controls

Every control has a `data-feature` id (CLAUDE.md invariant 7), and the help
registry in `ui/src/help/features/` lists them all with labels. Prefer
`[data-feature="…"]` selectors over classes or text: they are stable across
languages and restyling. Text matching (`click(sel, { text })`) is for rows
and results.

## Writing a UX test

Add `tools/webdriver/ux/NN-name.test.mjs`:

```js
import { assert, newProject } from "./harness.mjs";

export default {
  name: "what it proves",
  // optional: runs before the app starts; may return { env, teardown }
  async setup() { return { env: {}, teardown: async () => {} }; },
  async run(t) {
    const d = t.driver;                       // the client's Driver
    await newProject(d, "My test");           // start screen -> 3D stage
    await d.click('[data-feature="stage:3d"]');
    await d.waitFor("canvas.view3d-canvas", { visible: true });
    assert.ok(await d.exists(".view3d-labels"));
    await t.shot("3d-stage");                 // target/ux-shots/NN-name/01-3d-stage.png
  },
};
```

The client's helpers: `click`, `type` (sets a value the way React sees it),
`key`, `text`, `texts`, `count`, `exists`, `waitFor`, `waitGone`,
`queueDialog`, `invoke` (a Tauri command), `run` (a script with `done`),
`open`, `screenshot`. The harness adds `newProject`, `colourPixels` and
`distinctColours` (assert on what a canvas actually shows),
`setLanguageInSettings` and `toHex`. `fakeHindsightSync(archives)` serves
Hindsight Sync's archive API on a loopback port and returns `{ port, listen(), close() }`;
a driven app asks Hindsight Sync only on `PE_DRIVER_HINDSIGHT_PORT` (none when unset).

Rules:

- **Assert on what the person would see**, not only on the DOM: a curve's
  colour in the plot canvas, the flash's rectangle around the control, the
  text of a label after a language switch.
- **Drive the application's own controls.** A raw `invoke` moves the back end
  and leaves the interface where it was; the only shortcuts are the ones
  below, which skip what a WebDriver cannot reach.
- **Look at the pictures.** Open the PNGs (the Read tool shows them) before
  saying a UI change works. That is part of the definition of done.
- **No live network.** Serve recorded responses from a local server, as
  `04-tracker-dialog.test.mjs` does for YellowBrick.

The offline `09-polar-analysis` test opens `fixtures/analysis.wpsproj`. Rebuild
that fixture with `CARGO_INCREMENTAL=0 cargo run -p pe-app --example analysis_fixture`.
It covers catalogue pagination/imports, asymmetric plots, blend corrections,
individual/global filters, timestamp units and priority groups without fetching.
`11-wave-display-ranges` uses the same fixture to check bottom-centred dual-handle
sliders, pointer reachability when handles meet, centring as panels toggle, live redraws,
dragging the selected middle section with native blend updates before release,
combined height/angle/period bounds, missing measurements, updated native sample
counts and reset alongside other analysis filters. `12-live-analysis` imports an
offline instrument CSV, switches supplied/downloaded wind, checks every change
filter without blurring inputs, and hovers a rendered dot to inspect its details. `10-track-library` sets the read-only library's two folders, then imports two local search results
consecutively and checks that the query remains visible while each successfully
imported result is removed and the remaining count decreases.
It also selects both imported tracks with Select all and starts weather directly,
using an invalid test cache path to stop before any remote weather request.
`13-boat-tabs` opens a four-boat fixture, checks linked camera gestures and
hover, renaming, adding a boat, export-all, and returning to an independent
single view. `14-tracker-project` serves a recorded YellowBrick event and
checks that identical boat names cannot import another boat's polars without
matching model evidence.

## The seams, and why each exists

| Seam | Where | Only in | Why |
|---|---|---|---|
| `PE_AUTOMATION_ROOT` | `pe-app/src/paths.rs` | `webdriver` feature | A run must not write the person's settings or recovery files |
| `PE_DRIVER_LOCALES=fr-CA:en-US` | `pe-app/src/settings.rs` | `webdriver` feature, with `PE_AUTOMATION_ROOT` | Tests initial language selection without changing the person's OS preferences; the driver defaults to `en-US` |
| `PE_DRIVER_YELLOWBRICK=http://127.0.0.1:<port>` | `pe-app/src/trackers.rs` | `webdriver` feature, loopback only | The tracker dialog end to end without the network |
| `window.__peDialogAnswers` | `ui/src/automation.ts`, `project/dialogs.ts` | dev builds | A native file picker is outside the webview |
| `window.__peOpen(path)` | `ui/src/App.tsx` | dev builds | Opening through the app's own path, not a bare `open_project` |
| `canvas.__peRedraw()` | `automation.ts`, `MapView`, `PolarView`, `CompareView` | dev builds | A WebGL canvas reads back empty after its frame is shown |
| `canvas.__peInspectPolar()` | `automation.ts`, `PolarView` | dev builds | Read-only camera and projected-point observations for linked-view tests |

Adding one: gate Rust on `#[cfg(feature = "webdriver")]`, TypeScript on
`import.meta.env.DEV`, add no user-visible text, and add it to this table.

## Gotchas

- **Screenshots are composited in the page.** The plugin's `/screenshot`
  rasterises the DOM through an SVG `foreignObject`, which leaves every canvas
  blank, and `screencapture` returns only the wallpaper without the macOS
  Screen Recording permission (the terminal running the agent does not have
  it, and it is not worth granting). `Driver.screenshotPng` serialises the DOM
  the same way, then paints each canvas clipped to itself and draws whatever
  is positioned over it (dialogs, popups, the help flash, 3D labels) again on
  top. Animations are shown at rest. Fonts fall back to the system's inside
  the SVG. A scrolled container is drawn at its scroll offset: its cloned
  content is wrapped and moved by `scrollTop`/`scrollLeft` (M17b), so a
  control scrolled into view appears where the flash is drawn.
- **A script hands its answer to `done(…)`** (`run` binds it; `evaluate`
  takes `arguments[arguments.length - 1]`). Never call
  `window.__WEBDRIVER__.resolve` yourself, and never run longer than 30 s:
  either makes the plugin panic while holding its mutex, and every later call
  fails for the life of the process. Start long work and poll for it, as the
  screenshot does.
- **The endpoint's `/element/send-keys` does not reach React** (it sets
  `el.value` through the instance, which React's value tracker ignores). Use
  `type`.
- **Stop a driven app by its process group or pid.** `npm run dev:webdriver`
  wraps the Tauri CLI, which runs Vite and cargo; the client kills the whole
  group. **Never `pkill -f pe-app`**: the person very likely has their own
  `tauri dev` running, and a pattern kill takes it with yours.
- **Editing Rust restarts the person's own `tauri dev` app** (its watcher
  rebuilds). Batch Rust edits. Building with and without the feature both
  write `target/debug/pe-app`; cargo keeps both sets of artifacts, so
  switching costs one link, not a rebuild (the first `--features webdriver`
  build recompiles Tauri once, about 4 minutes).
- **Incremental compilation is disabled for development and tests** in the
  workspace `Cargo.toml`. Rust 1.97.1 on Intel macOS can otherwise fail to link
  optimised workspace crates with undefined internal `.llvm.*` symbols. The
  driver also sets `CARGO_INCREMENTAL=0` (`client.mjs` for the app it starts,
  `ux/run.mjs` for its prebuild); keep that setting on cargo commands. Ordinary
  `npm run dev` now uses the same setting without an environment variable.
  Do not override it with `CARGO_INCREMENTAL=1`: that reintroduces the linker
  failure and large incremental caches on this machine's tight disk.
- **A window that is not composited throttles `requestAnimationFrame` and
  timers.** The app's draws are scheduled on frames; the screenshot redraws
  synchronously, and a gesture must be dispatched synchronously too.
- **The plugin announces its port before the window exists**, and the window
  exists before React has mounted: `ready()` waits for `#root` to have
  children.
