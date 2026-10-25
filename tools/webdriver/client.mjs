/**
 * A client for the application's automation endpoint (D25). Copied from
 * VectorEffects' `tools/webdriver/client.mjs` and adapted.
 *
 * `tauri-plugin-webdriver-automation` serves a flat REST API on loopback —
 * `POST /element/find`, `/element/click`, `/script/execute-async` and so on.
 * It is *not* W3C WebDriver despite the name, so no off-the-shelf client
 * drives it; this is that client, shared by the MCP server, the CLI and the
 * UX suite.
 *
 * **Screenshots do not come from the endpoint.** Its `/screenshot` serialises
 * the DOM into an SVG `foreignObject` and rasterises that, which leaves every
 * canvas blank — the map, the polar plot and the 3D view, which are the things
 * worth a picture. `screenshot()` does the same serialisation but first reads
 * each canvas back (redrawing a WebGL one synchronously through the dev-only
 * `__peRedraw`, `ui/src/automation.ts`) and puts it into the picture as an
 * image, in place, so dialogs and the help flash over a canvas still show.
 * macOS `screencapture` would be simpler and is not usable: without the
 * Screen Recording permission it returns the wallpaper.
 *
 * **Typing does not use the endpoint's `/element/send-keys`** either: it sets
 * `el.value` through the instance, which React's value tracker swallows, so a
 * controlled input never sees the change. `type()` goes through the native
 * setter and dispatches `input`, which is what React listens for.
 */

import { spawn } from "node:child_process";
import { once } from "node:events";
import { mkdtemp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { createInterface } from "node:readline";
import { fileURLToPath } from "node:url";

/** The repository root. */
export const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..", "..");

/** Where the plugin announces itself. The only place it does. */
const PORT_LINE = /^\[webdriver] listening on port (\d+)$/;

/**
 * The port from a line of the application's output, or null.
 *
 * Anchored, because the app's log is full of lines that merely mention ports
 * and one of them matching would point the driver at nothing.
 */
export function portFromLine(line) {
  const found = PORT_LINE.exec(line.trim());
  return found ? Number(found[1]) : null;
}

/**
 * A file name from a caller's label: letters, digits, dashes and
 * underscores only, so a label can never climb out of the shots directory.
 */
export function safeName(name, fallback = "shot") {
  const safe = String(name ?? "").replace(/[^A-Za-z0-9_-]/g, "-").replace(/-+/g, "-").replace(/^-|-$/g, "");
  return safe || fallback;
}

/** How long to wait for the application to announce its port (a cold build is minutes). */
const START_TIMEOUT_MS = 600_000;

/** A loopback port nothing is using, released again before it is handed out. */
export async function freePort() {
  const server = createServer();
  await new Promise((done, fail) => {
    server.once("error", fail);
    server.listen(0, "127.0.0.1", done);
  });
  const { port } = server.address();
  await new Promise((done) => server.close(done));
  return port;
}

/** The window size every driven run uses, so pictures compare across machines. */
export const WINDOW = { width: 1440, height: 900 };

/**
 * The Tauri config the driver merges over the app's own: the dev server on
 * the driver's port, and the main window at a fixed size, not maximised and
 * not taking focus, so a run neither covers the person's screen nor pulls
 * their keyboard away. (A merge patch replaces arrays whole, so the window
 * is the app's own entry with those fields changed.)
 */
export async function driverConfig(cwd, devPort) {
  const conf = JSON.parse(await readFile(join(cwd, "crates/pe-app/tauri.conf.json"), "utf8"));
  const windows = conf.app.windows.map((w) =>
    w.label === "main" ? { ...w, ...WINDOW, maximized: false, focus: false } : w,
  );
  return { build: { devUrl: `http://localhost:${devPort}` }, app: { windows } };
}

/**
 * Signals a child's whole process *group*, falling back to the child alone.
 *
 * The negative pid is the group, which is why `launch` detaches: `npm run
 * dev:webdriver` is a wrapper around the Tauri CLI, which starts Vite as its
 * `beforeDevCommand` and then cargo, so signalling the wrapper alone leaves
 * Vite holding its port. **Never a pattern kill** (`pkill -f pe-app`): the
 * person whose machine this is very likely has their own `tauri dev` running.
 */
function signalGroup(child, name) {
  try {
    process.kill(-child.pid, name);
  } catch {
    try {
      child.kill(name);
    } catch {
      /* gone */
    }
  }
}

/** Stops the whole tree a `launch` started, and waits for it to go. */
async function stopTree(child) {
  signalGroup(child, "SIGTERM");
  if (child.exitCode === null && child.signalCode === null) {
    await Promise.race([once(child, "exit"), new Promise((r) => setTimeout(r, 5000).unref())]);
  }
  signalGroup(child, "SIGKILL");
}

/**
 * Starts the application with its automation endpoint and waits for the port.
 *
 * The endpoint binds `127.0.0.1:0` and announces the port on stdout and
 * nowhere else, so the driver has to own the process to know where to talk
 * to it.
 *
 * - **Its own Vite port** (`PE_DEV_PORT`, or a free one): the person's own
 *   `tauri dev` very likely holds 5173. Named to Vite and to the Tauri
 *   config's `devUrl` together. Nothing is killed to make room.
 * - **Its own data root** (`PE_AUTOMATION_ROOT`, `paths.rs`, compiled only
 *   with the feature): a fresh temporary directory unless the caller passes
 *   one, so a run never writes the person's settings, recent list or
 *   recovery snapshots. Removed on `close()` when this made it.
 * - **A failure here stops the tree before it rethrows**: the group is
 *   detached, and a caller with no `Driver` has nothing to stop it with.
 */
export async function launch({ cwd = ROOT, onLog, env = {}, timeoutMs = START_TIMEOUT_MS, signal } = {}) {
  signal?.throwIfAborted();
  const devPort = String(env.PE_DEV_PORT ?? process.env.PE_DEV_PORT ?? (await freePort()));
  let madeRoot = null;
  let automationRoot = env.PE_AUTOMATION_ROOT ?? process.env.PE_AUTOMATION_ROOT;
  if (!automationRoot) {
    madeRoot = await mkdtemp(join(tmpdir(), "pe-driver-"));
    automationRoot = madeRoot;
  }
  const child = spawn(
    "npm",
    ["run", "dev:webdriver", "--", "--config", JSON.stringify(await driverConfig(cwd, devPort))],
    {
      cwd,
      stdio: ["ignore", "pipe", "pipe"],
      detached: true,
      env: {
        ...process.env,
        FORCE_COLOR: "0",
        CARGO_INCREMENTAL: process.env.CARGO_INCREMENTAL ?? "0",
        PE_DRIVER_LOCALES: process.env.PE_DRIVER_LOCALES ?? "en-US",
        ...env,
        PE_DEV_PORT: devPort,
        PE_AUTOMATION_ROOT: automationRoot,
        PE_VITE_CACHE_DIR: env.PE_VITE_CACHE_DIR ?? join(cwd, "node_modules", ".vite-driver"),
      },
    },
  );

  let port = null;
  const waiters = [];
  const recent = [];
  const note = (line) => {
    onLog?.(line);
    recent.push(line);
    if (recent.length > 12) recent.shift();
  };
  createInterface({ input: child.stdout }).on("line", (line) => {
    note(line);
    const found = portFromLine(line);
    if (found !== null && port === null) {
      port = found;
      for (const done of waiters.splice(0)) done(found);
    }
  });
  createInterface({ input: child.stderr }).on("line", note);

  const exited = once(child, "exit").then(([code]) => {
    const tail = recent.filter((line) => line.trim()).slice(-6).join("\n  ");
    throw new Error(`the application exited with code ${code} before it was ready:\n  ${tail}`);
  });
  const announced = new Promise((done) => {
    if (port !== null) done(port);
    else waiters.push(done);
  });
  let timer = null;
  const timeout = new Promise((_, reject) => {
    timer = setTimeout(
      () => reject(new Error(`no automation port announced within ${timeoutMs} ms`)),
      timeoutMs,
    );
    timer.unref();
  });
  // `signal` cancels a start in flight (the MCP server's `stop`, the CLI's
  // Ctrl-C): the tree is stopped below like any other failure.
  let onAbort = null;
  const cancelled = new Promise((_, reject) => {
    if (!signal) return;
    onAbort = () => reject(new Error("the launch was cancelled"));
    if (signal.aborted) onAbort();
    else signal.addEventListener("abort", onAbort, { once: true });
  });
  // The losers of the race must not reject later with nobody listening: in
  // a long-lived process (the MCP server) that is an unhandled rejection.
  for (const loser of [exited, timeout, cancelled]) loser.catch(() => {});

  try {
    await Promise.race([announced, exited, timeout, cancelled]);
  } catch (failure) {
    await stopTree(child);
    if (madeRoot) await rm(madeRoot, { recursive: true, force: true });
    throw failure;
  } finally {
    clearTimeout(timer);
    if (onAbort) signal.removeEventListener("abort", onAbort);
  }
  exited.catch(() => {});
  const driver = new Driver(port, child);
  driver.automationRoot = automationRoot;
  driver.madeRoot = madeRoot;
  return driver;
}

/** Connects to an endpoint that is already listening (`PE_DRIVER_PORT`). */
export function connect(port) {
  return new Driver(Number(port), null);
}

const sleep = (ms) => new Promise((done) => setTimeout(done, ms));

/**
 * The part of a canvas the window shows: its box cut by every ancestor
 * that clips (overflow other than visible: a scrolled side panel) and by
 * the window. `clips` are those ancestors' inner boxes, `{ left, top,
 * right, bottom }`; null when nothing of it shows. The composite paints a
 * canvas inside this only, so a plot scrolled under a panel's edge does not
 * reappear over the status bar (M17a). Runs in the page too (inlined into
 * the shot script), so it uses nothing but its arguments.
 */
export function visibleRect(rect, clips, viewport) {
  var left = Math.max(rect.left, 0), top = Math.max(rect.top, 0);
  var right = Math.min(rect.left + rect.width, viewport.width);
  var bottom = Math.min(rect.top + rect.height, viewport.height);
  for (var i = 0; i < clips.length; i++) {
    left = Math.max(left, clips[i].left); top = Math.max(top, clips[i].top);
    right = Math.min(right, clips[i].right); bottom = Math.min(bottom, clips[i].bottom);
  }
  if (right <= left || bottom <= top) return null;
  return { left: left, top: top, width: right - left, height: bottom - top };
}

/**
 * The in-page half of `screenshot()`: started, never awaited, and leaves its
 * answer on `window.__peShot` for the poll (a script that outlives the
 * endpoint's 30 s timeout bricks it — see `evaluate`).
 */
const SHOT_SCRIPT = String.raw`
var done = arguments[arguments.length - 1];
window.__peShot = { state: "running" };
(async function () {
  var root = document.documentElement;
  var width = root.clientWidth, height = root.clientHeight;
  var ratio = window.devicePixelRatio || 1;
  var source = Array.prototype.slice.call(root.querySelectorAll("*"));
  // Every canvas first, each redrawn and read back in the same task: a WebGL
  // drawing buffer is not preserved past the frame that showed it.
  var canvases = [];
  var visibleRect = ${visibleRect.toString()};
  // The inner boxes of the ancestors that clip an element.
  var clipsOf = function (el) {
    var clips = [];
    for (var a = el.parentElement; a && a !== root; a = a.parentElement) {
      var style = getComputedStyle(a);
      if (style.overflowX === "visible" && style.overflowY === "visible") continue;
      var r = a.getBoundingClientRect();
      var l = r.left + a.clientLeft, t = r.top + a.clientTop;
      clips.push({ left: l, top: t, right: l + a.clientWidth, bottom: t + a.clientHeight });
    }
    return clips;
  };
  source.forEach(function (el) {
    if (!(el instanceof HTMLCanvasElement)) return;
    var rect = el.getBoundingClientRect();
    if (rect.width === 0 || rect.height === 0 || el.width === 0 || el.height === 0) return;
    var shown = visibleRect(rect, clipsOf(el), { width: width, height: height });
    if (!shown) return;
    try {
      if (typeof el.__peRedraw === "function") el.__peRedraw();
      var copy = document.createElement("canvas");
      copy.width = el.width; copy.height = el.height;
      copy.getContext("2d").drawImage(el, 0, 0);
      canvases.push({ el: el, rect: rect, shown: shown, pixels: copy });
    } catch (e) { /* a tainted or lost context: left blank */ }
  });
  var imageData = function (img) {
    try {
      var c = document.createElement("canvas");
      c.width = img.naturalWidth; c.height = img.naturalHeight;
      c.getContext("2d").drawImage(img, 0, 0);
      return c.toDataURL("image/png");
    } catch (e) { return null; }
  };
  var positioned = function (el) {
    var p = getComputedStyle(el).position;
    return p === "absolute" || p === "fixed" || p === "sticky";
  };
  var meets = function (a, b) {
    return a.left < b.right && b.left < a.right && a.top < b.bottom && b.top < a.bottom;
  };
  // Whether a positioned element paints over a canvas: after it in document
  // order (the help flash, the 3D view's labels, a dialog), or found above
  // it by hit testing where the two overlap (a title-bar popup that comes
  // earlier in the document but has the higher z-index). The flash takes no
  // pointer events, which is why hit testing alone is not enough.
  var above = function (el, over) {
    if (el === over.el || el.contains(over.el) || over.el.contains(el) || !positioned(el)) return false;
    var r = el.getBoundingClientRect();
    if (!meets(r, over.rect)) return false;
    if (over.el.compareDocumentPosition(el) & Node.DOCUMENT_POSITION_FOLLOWING) return true;
    var left = Math.max(r.left, over.rect.left), right = Math.min(r.right, over.rect.right);
    var top = Math.max(r.top, over.rect.top), bottom = Math.min(r.bottom, over.rect.bottom);
    for (var i = 1; i <= 3; i++) {
      for (var j = 1; j <= 3; j++) {
        var stack = document.elementsFromPoint(left + (right - left) * i / 4, top + (bottom - top) * j / 4);
        for (var k = 0; k < stack.length; k++) {
          if (stack[k] === over.el) break;
          if (el.contains(stack[k])) return true;
        }
      }
    }
    return false;
  };
  // The page as an SVG image. A canvas inside it rasterises blank and a
  // nested image does not load, so the canvases are painted in afterwards.
  // "overlay" names the canvas whose overlays alone are to be shown.
  var render = async function (overlay) {
    // Paired with the live page at the moment of cloning, element by
    // element, and skipped where the two disagree.
    var live = Array.prototype.slice.call(root.querySelectorAll("*"));
    var clone = root.cloneNode(true);
    var copies = Array.prototype.slice.call(clone.querySelectorAll("*"));
    // A picture is a moment, and an animation frozen at its start can be
    // invisible (the help flash pulses from opacity 0): everything is shown
    // at rest, as under prefers-reduced-motion.
    var still = document.createElement("style");
    still.textContent = "*, *::before, *::after { animation: none !important; transition: none !important; }";
    (clone.querySelector("head") || clone).appendChild(still);
    if (overlay) {
      clone.style.setProperty("visibility", "hidden", "important");
      clone.style.setProperty("background", "transparent", "important");
    }
    live.forEach(function (el, i) {
      var copy = copies[i];
      if (!copy || copy.tagName !== el.tagName) return;
      if (el instanceof HTMLInputElement) {
        if (el.type === "checkbox" || el.type === "radio") {
          if (el.checked) copy.setAttribute("checked", ""); else copy.removeAttribute("checked");
        } else if (el.type !== "file") copy.setAttribute("value", el.value);
      } else if (el instanceof HTMLTextAreaElement) {
        copy.textContent = el.value;
      } else if (el instanceof HTMLSelectElement) {
        var mine = el.querySelectorAll("option");
        Array.prototype.forEach.call(copy.querySelectorAll("option"), function (o, k) {
          if (mine[k] && mine[k].selected) o.setAttribute("selected", ""); else o.removeAttribute("selected");
        });
      } else if (el instanceof HTMLImageElement && el.src && el.src.indexOf("data:") !== 0) {
        var data = imageData(el);
        if (data) copy.setAttribute("src", data);
      } else if (el instanceof HTMLScriptElement) {
        copy.remove();
        return;
      }
      if (el === document.body && overlay) copy.style.setProperty("background", "transparent", "important");
      if (overlay && above(el, overlay)) {
        copy.style.setProperty("visibility", "visible", "important");
      }
    });
    // A cloned container starts unscrolled: its content is wrapped and moved
    // up and left by the live scroll offsets, so a scrolled side panel shows
    // what the window shows (M17b: the flash of a control scrolled into view
    // was drawn over the panel's top).
    live.forEach(function (el, i) {
      var copy = copies[i];
      if (!copy || copy.tagName !== el.tagName) return;
      if (el.scrollTop === 0 && el.scrollLeft === 0) return;
      if (el === document.documentElement || el === document.body) return;
      var wrap = document.createElement("div");
      wrap.style.cssText = "position: relative; top: " + (-el.scrollTop) + "px; left: " + (-el.scrollLeft) + "px;";
      // The wrapper lays its children out as the container did.
      var cs = getComputedStyle(el);
      ["display", "flexDirection", "flexWrap", "gap", "alignItems", "justifyContent",
        "gridTemplateColumns", "gridTemplateRows", "gridAutoFlow"].forEach(function (k) { wrap.style[k] = cs[k]; });
      wrap.style.minHeight = el.clientHeight + "px";
      while (copy.firstChild) wrap.appendChild(copy.firstChild);
      copy.appendChild(wrap);
      copy.style.setProperty("overflow", "hidden", "important");
    });
    var xml = new XMLSerializer().serializeToString(clone);
    var svg = '<svg xmlns="http://www.w3.org/2000/svg" width="' + width + '" height="' + height + '">' +
      '<foreignObject width="100%" height="100%">' + xml + "</foreignObject></svg>";
    var picture = new Image();
    picture.src = "data:image/svg+xml;charset=utf-8," + encodeURIComponent(svg);
    await picture.decode();
    return picture;
  };
  var out = document.createElement("canvas");
  out.width = Math.round(width * ratio); out.height = Math.round(height * ratio);
  var ctx = out.getContext("2d");
  ctx.scale(ratio, ratio);
  ctx.fillStyle = getComputedStyle(document.body).backgroundColor || "#000";
  ctx.fillRect(0, 0, width, height);
  ctx.drawImage(await render(null), 0, 0, width, height);
  // Then each canvas in document order, clipped to the part of it the
  // window shows (M17a), with whatever is
  // positioned over it drawn again on top (dialogs, the help flash, labels).
  for (var k = 0; k < canvases.length; k++) {
    var c = canvases[k];
    var over = await render(c);
    ctx.save();
    ctx.beginPath();
    ctx.rect(c.shown.left, c.shown.top, c.shown.width, c.shown.height);
    ctx.clip();
    ctx.drawImage(c.pixels, c.rect.left, c.rect.top, c.rect.width, c.rect.height);
    ctx.drawImage(over, 0, 0, width, height);
    ctx.restore();
  }
  return out.toDataURL("image/png").split(",")[1];
})().then(function (data) {
  window.__peShot = { state: "done", data: data };
}, function (e) {
  window.__peShot = { state: "failed", message: String(e && e.message || e) };
});
done("started");
`;

/** The application's automation endpoint. */
export class Driver {
  constructor(port, child) {
    this.port = port;
    this.child = child;
    this.automationRoot = null;
    this.madeRoot = null;
  }

  /** One call to the endpoint. Every route is a POST with a JSON body. */
  async call(route, body = {}) {
    const response = await fetch(`http://127.0.0.1:${this.port}${route}`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(body),
    });
    const text = await response.text();
    let value;
    try {
      value = text ? JSON.parse(text) : null;
    } catch {
      throw new Error(`${route} answered with something that is not JSON: ${text.slice(0, 200)}`);
    }
    if (!response.ok) {
      throw new Error(`${route} failed (${response.status}): ${JSON.stringify(value)}`);
    }
    return value;
  }

  /**
   * Runs a script and returns what it hands back.
   *
   * **The script hands its answer to the callback passed as its last
   * argument** (`arguments[arguments.length - 1]`), the executeAsync
   * convention. Never `window.__WEBDRIVER__.resolve` directly: the id there
   * is a uuid the caller never sees, and a wrong one makes the plugin panic
   * while holding its pending table's mutex — every later call then panics
   * too, for the life of the process. A script that outruns the endpoint's
   * 30 s timeout does the same, so long work is started and polled for.
   *
   * A result comes back as `{"value": …}`; a failure inside the script comes
   * back in that envelope as `{error, message}`. Both are unwrapped here.
   */
  async evaluate(script, args = []) {
    const body = await this.call("/script/execute-async", { script, args });
    const value = body && typeof body === "object" && "value" in body ? body.value : body;
    if (value && typeof value === "object" && typeof value.error === "string") {
      throw new Error(`script failed: ${value.error} — ${value.message ?? ""}`);
    }
    return value;
  }

  /** Runs a function body with `done` bound to the answer callback. */
  run(body, args = []) {
    return this.evaluate(`var done = arguments[arguments.length - 1]; ${body}`, args);
  }

  /** Calls a Tauri command, as the interface's own `invoke` does. */
  invoke(command, args = {}) {
    return this.run(
      `window.__TAURI_INTERNALS__.invoke(arguments[0], arguments[1]).then(done,
         function (e) { done({ error: "IPC", message: JSON.stringify(e) }); });`,
      [command, args],
    );
  }

  /**
   * Waits until the application has a webview with the interface mounted.
   *
   * The endpoint announces its port as soon as the server binds, which is
   * before the window exists, and the window exists before Vite has served
   * the page and React has rendered into `#root`.
   */
  async ready({ timeoutMs = 120_000, signal } = {}) {
    const deadline = Date.now() + timeoutMs;
    for (;;) {
      signal?.throwIfAborted();
      try {
        const handles = await this.call("/window/handles");
        if (Array.isArray(handles) && handles.length > 0) {
          const mounted = await this.run(
            `var r = document.getElementById("root"); done(!!(r && r.children.length > 0 && document.readyState === "complete"));`,
          );
          if (mounted === true) return handles;
        }
      } catch {
        // Not answering yet; try again below.
      }
      if (Date.now() > deadline) throw new Error("the application did not show its interface");
      await sleep(250);
    }
  }

  /** Whether an element matching `selector` exists (and, with `text`, contains it). */
  exists(selector, { text, visible = false } = {}) {
    return this.run(
      `var want = arguments[1], vis = arguments[2];
       var found = Array.prototype.some.call(document.querySelectorAll(arguments[0]), function (el) {
         if (vis) { var r = el.getBoundingClientRect(); if (r.width === 0 || r.height === 0) return false; }
         return want == null || (el.textContent || "").indexOf(want) >= 0 || (el.value || "") === want;
       });
       done(found);`,
      [selector, text ?? null, visible],
    );
  }

  /** Waits for `selector` (optionally containing `text`) to appear. */
  async waitFor(selector, { text, visible = false, timeoutMs = 30_000 } = {}) {
    const deadline = Date.now() + timeoutMs;
    for (;;) {
      if ((await this.exists(selector, { text, visible })) === true) return;
      if (Date.now() > deadline) {
        throw new Error(`nothing matched ${selector}${text ? ` containing "${text}"` : ""} within ${timeoutMs} ms`);
      }
      await sleep(200);
    }
  }

  /** Waits for `selector` to be gone. */
  async waitGone(selector, { timeoutMs = 30_000 } = {}) {
    const deadline = Date.now() + timeoutMs;
    for (;;) {
      if ((await this.exists(selector)) !== true) return;
      if (Date.now() > deadline) throw new Error(`${selector} was still there after ${timeoutMs} ms`);
      await sleep(200);
    }
  }

  /** Counts the elements matching `selector`. */
  count(selector) {
    return this.run(`done(document.querySelectorAll(arguments[0]).length);`, [selector]);
  }

  /** Clicks the first element matching `selector` (optionally containing `text`). */
  async click(selector, { text } = {}) {
    const clicked = await this.run(
      `var want = arguments[1];
       var el = Array.prototype.find.call(document.querySelectorAll(arguments[0]), function (e) {
         return want == null || (e.textContent || "").indexOf(want) >= 0;
       });
       if (!el) { done(false); return; }
       el.scrollIntoView({ block: "nearest", inline: "nearest" });
       if (el.focus) el.focus();
       el.click();
       done(true);`,
      [selector, text ?? null],
    );
    if (clicked !== true) throw new Error(`nothing to click: ${selector}${text ? ` containing "${text}"` : ""}`);
  }

  /**
   * Types into an input, textarea or select, replacing its value, the way
   * React sees it: the native setter, then `input` and `change`.
   */
  async type(selector, value) {
    const typed = await this.run(
      `var el = document.querySelector(arguments[0]);
       if (!el) { done(false); return; }
       var proto = el instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype
         : el instanceof HTMLSelectElement ? HTMLSelectElement.prototype : HTMLInputElement.prototype;
       if (el.focus) el.focus();
       Object.getOwnPropertyDescriptor(proto, "value").set.call(el, arguments[1]);
       el.dispatchEvent(new Event("input", { bubbles: true }));
       el.dispatchEvent(new Event("change", { bubbles: true }));
       done(true);`,
      [selector, String(value)],
    );
    if (typed !== true) throw new Error(`nothing to type into: ${selector}`);
  }

  /** Sends one key (`Enter`, `Escape`, `ArrowDown`…) to `selector`, or to the focused element. */
  async key(key, selector = null) {
    const sent = await this.run(
      `var el = arguments[0] ? document.querySelector(arguments[0]) : (document.activeElement || document.body);
       if (!el) { done(false); return; }
       var k = arguments[1];
       el.dispatchEvent(new KeyboardEvent("keydown", { key: k, bubbles: true, cancelable: true }));
       el.dispatchEvent(new KeyboardEvent("keyup", { key: k, bubbles: true, cancelable: true }));
       done(true);`,
      [selector, key],
    );
    if (sent !== true) throw new Error(`no element to send ${key} to: ${selector}`);
  }

  /** The text of the first element matching `selector`, or null. */
  text(selector) {
    return this.run(
      `var el = document.querySelector(arguments[0]);
       var field = el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement || el instanceof HTMLSelectElement;
       done(el ? (field ? el.value : el.textContent) : null);`,
      [selector],
    );
  }

  /** The texts of every element matching `selector`. */
  texts(selector) {
    return this.run(
      `done(Array.prototype.map.call(document.querySelectorAll(arguments[0]), function (el) { return (el.textContent || "").trim(); }));`,
      [selector],
    );
  }

  /**
   * Queues the answers the next native file dialogs give (dev builds only,
   * `ui/src/automation.ts`): a path string, an array of paths, or null for
   * a cancel. A native picker is outside the webview, where nothing reaches.
   */
  queueDialog(...answers) {
    return this.run(
      `window.__peDialogAnswers = (window.__peDialogAnswers || []).concat(arguments[0]); done(window.__peDialogAnswers.length);`,
      [answers],
    );
  }

  /**
   * Opens a project through the application's own opening
   * (`window.__peOpen`, dev builds only): invoking `open_project` alone moves
   * the backend and leaves the interface on the start screen.
   */
  async open(path) {
    await this.ready();
    await this.awaitHook("__peOpen", "opening a project");
    return this.run(
      `window.__peOpen(arguments[0]).then(function (name) { done(name || true); },
         function (e) { done({ error: "OpenFailed", message: String(e && e.message || JSON.stringify(e)) }); });`,
      [path],
    );
  }

  /** Waits for one of the application's dev-only hooks to appear. */
  async awaitHook(name, doing, { timeoutMs = 30_000 } = {}) {
    const deadline = Date.now() + timeoutMs;
    for (;;) {
      const present = await this.run(`done(typeof window[arguments[0]] === "function");`, [name]);
      if (present === true) return;
      if (Date.now() > deadline) {
        throw new Error(`no hook for ${doing} appeared (window.${name}) — it exists in dev builds only`);
      }
      await sleep(250);
    }
  }

  /**
   * A picture of the whole window, canvases included, as PNG bytes.
   *
   * Started in the page and polled for (see `SHOT_SCRIPT`).
   */
  async screenshotPng({ timeoutMs = 60_000 } = {}) {
    const started = await this.evaluate(SHOT_SCRIPT);
    if (started !== "started") throw new Error(`the screenshot did not start: ${JSON.stringify(started)}`);
    const deadline = Date.now() + timeoutMs;
    for (;;) {
      const shot = await this.run(
        `var s = window.__peShot || null; if (s && s.state !== "running") window.__peShot = null; done(s);`,
      );
      if (shot && shot.state === "done") return Buffer.from(shot.data, "base64");
      if (shot && shot.state === "failed") throw new Error(`the screenshot failed: ${shot.message}`);
      if (Date.now() > deadline) throw new Error(`the screenshot did not finish within ${timeoutMs} ms`);
      await sleep(150);
    }
  }

  /**
   * Takes a picture and writes it to `path` (made absolute from the
   * repository root), creating the directory. Returns the absolute path.
   */
  async screenshot(path) {
    const png = await this.screenshotPng();
    const absolute = resolve(ROOT, path);
    await mkdir(dirname(absolute), { recursive: true });
    await writeFile(absolute, png);
    return absolute;
  }

  /**
   * Stops the application, if this driver started it — the whole tree —
   * and removes the data root it made. A driver from `connect` belongs to
   * whoever started that application and stops nothing.
   */
  async close() {
    if (!this.child) return;
    await stopTree(this.child);
    if (this.madeRoot) await rm(this.madeRoot, { recursive: true, force: true });
  }
}
