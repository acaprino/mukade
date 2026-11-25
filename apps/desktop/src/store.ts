// What the page remembers is held by plain classes with no screen in them: the screens read them through `useStore` and ask them
// for changes, and a class tells the screens when something changed. A store is a number that goes up (the snapshot of
// `useSyncExternalStore`) and the listeners to wake.

export class Store {
  private version = 0;
  private readonly listeners = new Set<() => void>();

  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => { this.listeners.delete(listener); };
  };

  /** What changes whenever the state does: the snapshot of `useSyncExternalStore`. */
  snapshot = (): number => this.version;

  /** Tells the screens that the state changed. */
  protected changed(): void {
    this.version += 1;
    for (const listener of [...this.listeners]) listener();
  }

  /** The same, for a part that holds a store and lets it change from outside (a controller that another one asks to draw). */
  touch(): void {
    this.changed();
  }
}
