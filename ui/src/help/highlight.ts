/**
 * Showing a person where a feature is (spec.md 3.6): bring it on screen,
 * then flash a rectangle around it.
 *
 * Bringing it on screen belongs to whoever owns the state that hides it — the
 * map selects a tool, the shell opens a panel, Settings opens itself — so this
 * module only keeps a table of those handlers (`onReveal`) and runs a
 * feature's steps in order. Then it waits for the element to be laid out,
 * because the handler's state change renders on a later frame.
 */

import type { Feature } from "./features";

type Reveal = (step: string) => void | Promise<void>;

const handlers = new Map<string, { handler: Reveal; available: () => boolean }[]>();

/**
 * Handles a reveal step: an exact step (`"panel:left"`) or every step with a
 * prefix (`"tool:"`). Returns the unregistration, for an effect's cleanup.
 */
export function onReveal(step: string, handler: Reveal, available: () => boolean = () => true): () => void {
  const entry = { handler, available };
  handlers.set(step, [...(handlers.get(step) ?? []), entry]);
  return () => {
    const remaining = (handlers.get(step) ?? []).filter(item => item !== entry);
    if (remaining.length) handlers.set(step, remaining); else handlers.delete(step);
  };
}

function handlerFor(step: string): Reveal | undefined {
  const eligible = (key: string) => [...(handlers.get(key) ?? [])].reverse().find(entry => entry.available())?.handler;
  const exact = eligible(step);
  if (exact) return exact;
  const colon = step.indexOf(":");
  return colon >= 0 ? eligible(step.slice(0, colon + 1)) : undefined;
}

const frame = () => new Promise<void>(resolve => {
  // A frame, or a moment when frames are suspended (WKWebView occluded).
  let done = false;
  const finish = () => { if (!done) { done = true; resolve(); } };
  requestAnimationFrame(finish);
  setTimeout(finish, 50);
});

/** The element for a feature, if it is laid out and visible. */
export function elementFor(id: string): HTMLElement | null {
  const elements = [...document.querySelectorAll<HTMLElement>(`[data-feature="${CSS.escape(id)}"]`)];
  elements.sort((a,b) => Number(Boolean(b.closest(".boat-pane.active"))) - Number(Boolean(a.closest(".boat-pane.active"))));
  for (const element of elements) {
    if (element.closest("[hidden]")) continue;
    const rect = element.getBoundingClientRect();
    if (rect.width > 0 && rect.height > 0) return element;
  }
  return null;
}

/**
 * Reveals a feature and flashes it. Resolves false when it could not be
 * found on screen, so the caller can fall back to its help page.
 */
export async function locateFeature(feature: Feature): Promise<boolean> {
  for (const step of feature.reveal ?? []) {
    await handlerFor(step)?.(step);
    await frame();
  }
  // A control inside a dialog that needs a choice first lands on the
  // control that opens it.
  const target = feature.landing ?? feature.id;
  for (let attempt = 0; attempt < 20; attempt += 1) {
    const element = elementFor(target);
    if (element) {
      element.scrollIntoView({ block: "nearest", inline: "nearest" });
      flash(element);
      return true;
    }
    await frame();
  }
  return false;
}

/** Reveal a source in the active tab without changing its visibility or blend weight. */
export async function locateSource(boat: number, source: number): Promise<boolean> {
  for (const step of ["panel:right", "section:sources", "sources:reset-search"]) {
    await handlerFor(step)?.(step);
    await frame();
  }
  for (let attempt = 0; attempt < 20; attempt += 1) {
    const element = document.querySelector<HTMLElement>(`.boat-pane.active[data-boat-id="${boat}"] [data-source-id="${source}"]`);
    if (element && !element.closest("[hidden]") && element.getBoundingClientRect().height > 0) {
      element.scrollIntoView({ block: "nearest", inline: "nearest" });
      element.focus({ preventScroll: true });
      flash(element);
      return true;
    }
    await frame();
  }
  return false;
}

let current: (() => void) | null = null;

/** How long the rectangle flashes for, in ms. Three pulses. */
export const FLASH_MS = 2400;

/**
 * Draws a flashing rectangle around an element, following it if it moves,
 * and removes it after `FLASH_MS` or on the next press anywhere.
 *
 * The rectangle is orange in every theme (`--flash`, spec.md 3.6): it has to
 * stand out from whatever the theme's own accent is. The stylesheet pulses
 * it, and holds it steady under `prefers-reduced-motion`.
 */
export function flash(element: HTMLElement): void {
  current?.();
  const box = document.createElement("div");
  box.className = "feature-flash";
  box.style.borderColor = "var(--flash)";
  box.setAttribute("aria-hidden", "true");
  document.body.appendChild(box);
  let raf = 0;
  const place = () => {
    const rect = element.getBoundingClientRect();
    const pad = 4;
    box.style.left = `${rect.left - pad}px`;
    box.style.top = `${rect.top - pad}px`;
    box.style.width = `${rect.width + pad * 2}px`;
    box.style.height = `${rect.height + pad * 2}px`;
    raf = requestAnimationFrame(place);
  };
  place();
  const stop = () => {
    cancelAnimationFrame(raf);
    clearTimeout(timer);
    box.remove();
    window.removeEventListener("pointerdown", stop, true);
    if (current === stop) current = null;
  };
  const timer = setTimeout(stop, FLASH_MS);
  window.addEventListener("pointerdown", stop, true);
  current = stop;
}
