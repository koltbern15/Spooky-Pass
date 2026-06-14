// Centralized actions: every api.* call is wrapped here so error handling and
// state transitions live in one place. Views call these instead of `api`
// directly.

import { api } from "./api";
import { clearClipboard } from "./platform";
import { store } from "./store";
import type { Route } from "./store";
import type {
  AppError,
  AppErrorKind,
  EntryInput,
  EntryPatchInput,
  EntryView,
  GeneratorOptions,
  Status,
} from "./types";

/** Narrow an unknown rejection into our AppError contract. */
export function toAppError(err: unknown): AppError {
  if (
    typeof err === "object" &&
    err !== null &&
    "kind" in err &&
    typeof (err as { kind: unknown }).kind === "string"
  ) {
    return err as AppError;
  }
  return { kind: "Internal", message: String(err) };
}

/** Human-readable message for an AppError, used for inline form errors. */
export function errorMessage(err: AppError): string {
  const byKind: Partial<Record<AppErrorKind, string>> = {
    WrongPassword: "Wrong master password.",
    Locked: "The vault is locked.",
    VaultAlreadyExists: "A vault already exists.",
    NoVault: "No vault found.",
    EntryNotFound: "That entry no longer exists.",
    InvalidPasswordOptions: "Invalid generator options — enable at least one character class.",
    InvalidVaultFile: "The vault file is invalid or corrupted.",
    Internal: "Something went wrong.",
  };
  return err.message?.trim() || byKind[err.kind] || "Something went wrong.";
}

/**
 * If the error means the session is no longer usable (Locked / NoVault),
 * route back to the appropriate gate screen. Returns true if it handled the
 * error by routing away.
 */
function handleSessionError(err: AppError): boolean {
  if (err.kind === "Locked") {
    store.set({ route: "unlock", selectedId: null });
    return true;
  }
  if (err.kind === "NoVault") {
    store.set({ route: "create", selectedId: null });
    return true;
  }
  return false;
}

export interface Actions {
  go(route: Route): void;
  selectEntry(id: string): void;
  /** Open the edit screen with a blank form (selectedId cleared = "new"). */
  newEntry(): void;
  /** Open the edit screen for the currently selected entry. */
  editSelected(): void;

  refreshStatus(): Promise<Status | null>;
  createVault(masterPassword: string): Promise<void>;
  unlock(masterPassword: string): Promise<void>;
  lock(): Promise<void>;

  loadEntries(): Promise<void>;
  getEntry(id: string): Promise<EntryView | null>;
  addEntry(input: EntryInput): Promise<EntryView | null>;
  updateEntry(id: string, patch: EntryPatchInput): Promise<EntryView | null>;
  deleteEntry(id: string): Promise<boolean>;

  generatePassword(options: GeneratorOptions): Promise<string | null>;
  setIdleTimeout(secs: number): Promise<void>;
}

/**
 * Build the actions object. `onError` is invoked with a user-facing message
 * for errors the caller should surface inline (e.g. wrong password). Session
 * errors (Locked/NoVault) are handled by routing and do NOT call onError.
 */
export function createActions(onError?: (message: string) => void): Actions {
  const report = (err: unknown): AppError => {
    const appErr = toAppError(err);
    if (!handleSessionError(appErr)) {
      onError?.(errorMessage(appErr));
    }
    return appErr;
  };

  return {
    go(route) {
      store.set({ route });
    },

    selectEntry(id) {
      store.set({ selectedId: id, route: "detail" });
    },

    newEntry() {
      store.set({ selectedId: null, route: "edit" });
    },

    editSelected() {
      store.set({ route: "edit" });
    },

    async refreshStatus() {
      try {
        const status = await api.status();
        store.set({ status });
        return status;
      } catch (err) {
        report(err);
        return null;
      }
    },

    async createVault(masterPassword) {
      try {
        const status = await api.createVault(masterPassword);
        store.set({ status, route: "list", selectedId: null });
        await this.loadEntries();
      } catch (err) {
        report(err);
      }
    },

    async unlock(masterPassword) {
      try {
        const status = await api.unlock(masterPassword);
        store.set({ status, route: "list", selectedId: null });
        await this.loadEntries();
      } catch (err) {
        report(err);
      }
    },

    async lock() {
      // Wipe any password still pending on the clipboard when we lock.
      void clearClipboard();
      try {
        const status = await api.lock();
        store.set({ status, route: "unlock", selectedId: null, entries: [] });
      } catch (err) {
        report(err);
      }
    },

    async loadEntries() {
      try {
        const entries = await api.listEntries();
        store.set({ entries });
      } catch (err) {
        report(err);
      }
    },

    async getEntry(id) {
      try {
        return await api.getEntry(id);
      } catch (err) {
        report(err);
        return null;
      }
    },

    async addEntry(input) {
      try {
        const view = await api.addEntry(input);
        await this.loadEntries();
        return view;
      } catch (err) {
        report(err);
        return null;
      }
    },

    async updateEntry(id, patch) {
      try {
        const view = await api.updateEntry(id, patch);
        await this.loadEntries();
        return view;
      } catch (err) {
        report(err);
        return null;
      }
    },

    async deleteEntry(id) {
      try {
        await api.deleteEntry(id);
        await this.loadEntries();
        return true;
      } catch (err) {
        report(err);
        return false;
      }
    },

    async generatePassword(options) {
      try {
        return await api.generatePassword(options);
      } catch (err) {
        report(err);
        return null;
      }
    },

    async setIdleTimeout(secs) {
      try {
        const status = await api.setIdleTimeout(secs);
        store.set({ status });
      } catch (err) {
        report(err);
      }
    },
  };
}
