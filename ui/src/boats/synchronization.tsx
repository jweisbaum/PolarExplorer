import { createContext, useContext, useMemo, type ReactNode } from "react";
import type { View } from "../polar/scene3d";
import type { Layout } from "../polar/geometry3d";

export type HoverPoint = { twa: number; tws: number };
export interface BoatViewState {
  camera: View | null;
  layout: Layout | null;
}
type SyncEvent =
  | { kind: "camera"; boat: number; view: View; layout: Layout }
  | { kind: "hover"; boat: number; point: HoverPoint | null }
  /** The blend cell under the pointer in one pane, by its wind; null when the pointer has left the surface. */
  | { kind: "blend"; boat: number; point: HoverPoint | null };

export function createFleetSync() {
  const listeners = new Set<(event: SyncEvent) => void>();
  let camera: Extract<SyncEvent, { kind: "camera" }> | null = null;
  return {
    get camera() { return camera; },
    publish(event: SyncEvent) {
      if (event.kind === "camera") camera = event;
      for (const listener of listeners) listener(event);
    },
    subscribe(listener: (event: SyncEvent) => void) {
      listeners.add(listener);
      if (camera) listener(camera);
      return () => { listeners.delete(listener); };
    },
  };
}

const Context = createContext<ReturnType<typeof createFleetSync> | null>(null);
export function FleetSyncProvider({ enabled, sync: given, children }: {
  enabled: boolean;
  /** A sync made outside, so a test can publish into it. */
  sync?: ReturnType<typeof createFleetSync>;
  children: ReactNode;
}) {
  const sync = useMemo(() => enabled ? given ?? createFleetSync() : null, [enabled, given]);
  return <Context.Provider value={sync}>{children}</Context.Provider>;
}
export function useFleetSync() { return useContext(Context); }

/** Compare like wind conditions, never by unrelated sample/source ids. */
export function correspondingDot(points: Float32Array, point: HoverPoint): number {
  let best = -1, score = Infinity;
  for (let i = 0; i < points.length; i += 3) {
    const distance = Math.abs(points[i]! - point.twa) % 360;
    const angle = Math.min(distance, 360 - distance);
    const wind = Math.abs(points[i + 1]! - point.tws);
    if (angle > 5 || wind > 1) continue;
    const next = angle * angle / 25 + wind * wind;
    if (next < score) { best = i / 3; score = next; }
  }
  return best;
}

/**
 * The cell of another boat's output grid at the wind of a hovered blend
 * cell: the nearest angle and the nearest wind speed, within what the dots'
 * link allows (5°, 1 kn). Boats need not share a grid, and a cell far from
 * the hovered wind would compare unlike conditions, so it is no cell.
 */
export function correspondingCell(twa: ArrayLike<number>, tws: ArrayLike<number>, point: HoverPoint): { twa: number; tws: number } | null {
  let i = -1, angle = Infinity;
  for (let k = 0; k < twa.length; k++) {
    const distance = Math.abs(twa[k]! - point.twa) % 360;
    const apart = Math.min(distance, 360 - distance);
    if (apart < angle) { angle = apart; i = k; }
  }
  let j = -1, wind = Infinity;
  for (let k = 0; k < tws.length; k++) {
    const apart = Math.abs(tws[k]! - point.tws);
    if (apart < wind) { wind = apart; j = k; }
  }
  return i < 0 || j < 0 || angle > 5 || wind > 1 ? null : { twa: i, tws: j };
}
