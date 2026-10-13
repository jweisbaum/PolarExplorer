import { createContext, useContext, useMemo, useRef, type ReactNode } from "react";
import { onReveal } from "../help/highlight";
import { api, boatApi } from "../ipc";

const Context = createContext<{ id: number | undefined; api: typeof api; active: boolean; readOnly: boolean }>({ id: undefined, api, active: true, readOnly: false });

/** Requests retain this boat's identity even after its tab is hidden. */
export function BoatProvider({ id, active = true, readOnly = false, children }: { id: number; active?: boolean; readOnly?: boolean; children: ReactNode }) {
  const scopedApi = useMemo(() => boatApi(id), [id]);
  const value = useMemo(() => ({ id, api: scopedApi, active, readOnly }), [id, active, scopedApi, readOnly]);
  return <Context.Provider value={value}>{children}</Context.Provider>;
}

export function useBoatApi() { return useContext(Context).api; }
export function useBoatReadOnly() { return useContext(Context).readOnly; }
export function useBoatId() { return useContext(Context).id; }

/** Hidden boat workspaces retain their state but never receive help actions. */
export function useBoatReveal() {
  const value = useContext(Context).active;
  const active = useRef(value);
  active.current = value;
  return useMemo(() => (step: string, handler: Parameters<typeof onReveal>[1]) => onReveal(step, handler, () => active.current), []);
}
