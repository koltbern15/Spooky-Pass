// A tiny, framework-free observable store. Holds the whole UI state; views
// subscribe and re-render whenever `set` mutates it.

import type { Status, EntrySummary } from "./types";

export type Route =
  | "create"
  | "unlock"
  | "list"
  | "detail"
  | "edit"
  | "generator"
  | "settings";

export interface AppState {
  status: Status | null;
  entries: EntrySummary[];
  /** id of the currently selected/edited entry; null means "new" in edit. */
  selectedId: string | null;
  route: Route;
}

type Listener = (state: AppState) => void;

const initialState: AppState = {
  status: null,
  entries: [],
  selectedId: null,
  route: "unlock",
};

class Store {
  private state: AppState = { ...initialState };
  private listeners = new Set<Listener>();

  get(): AppState {
    return this.state;
  }

  set(patch: Partial<AppState>): void {
    this.state = { ...this.state, ...patch };
    this.emit();
  }

  subscribe(listener: Listener): () => void {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  }

  private emit(): void {
    for (const listener of this.listeners) {
      listener(this.state);
    }
  }
}

export const store = new Store();
