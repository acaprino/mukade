// Small hooks of the screens that draw nothing: the controller as a store, the way a harness is marked and a wait that a quick answer
// never shows.
import { createContext, useContext, useEffect, useState, useSyncExternalStore } from 'react';
import type { Controller } from '../controller.ts';
import type { Store } from '../store.ts';

export const ControllerContext = createContext<Controller | null>(null);

/** A render whenever a store changes. */
export function useStore(store: Store): void {
  useSyncExternalStore(store.subscribe, store.snapshot);
}

/** The controller of the window, and a render whenever its state changes. */
export function useController(): Controller {
  const controller = useContext(ControllerContext);
  if (controller === null) throw new Error('outside the window of Mukade');
  useStore(controller);
  return controller;
}

/** How long a wait is hidden before it is shown: an answer that comes at once never flashes a loading state. */
export const LOADING_DELAY_MS = 250;

/** True once `active` has held for `ms`; false as soon as it does not. */
export function useAfter(active: boolean, ms: number = LOADING_DELAY_MS): boolean {
  const [late, setLate] = useState(false);
  useEffect(() => {
    if (!active) { setLate(false); return; }
    const timer = setTimeout(() => setLate(true), ms);
    return () => clearTimeout(timer);
  }, [active, ms]);
  return active && late;
}
