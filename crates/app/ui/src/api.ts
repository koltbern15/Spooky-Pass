// The single module that talks to the Tauri backend. Every command call goes
// through here, so the command contract lives in exactly one place.
//
// FROZEN: command names and argument keys must match the #[tauri::command]
// signatures in crates/app/src/commands.rs. Tauri serializes args as a JSON
// object keyed by the (camelCased) parameter name.

import { invoke } from "@tauri-apps/api/core";
import type {
  Status,
  EntrySummary,
  EntryView,
  EntryInput,
  EntryPatchInput,
  GeneratorOptions,
} from "./types";

export const api = {
  status: (): Promise<Status> => invoke("status"),

  createVault: (masterPassword: string): Promise<Status> =>
    invoke("create_vault", { masterPassword }),

  unlock: (masterPassword: string): Promise<Status> =>
    invoke("unlock", { masterPassword }),

  lock: (): Promise<Status> => invoke("lock"),

  listEntries: (): Promise<EntrySummary[]> => invoke("list_entries"),

  getEntry: (id: string): Promise<EntryView> => invoke("get_entry", { id }),

  addEntry: (input: EntryInput): Promise<EntryView> =>
    invoke("add_entry", { input }),

  updateEntry: (id: string, patch: EntryPatchInput): Promise<EntryView> =>
    invoke("update_entry", { id, patch }),

  deleteEntry: (id: string): Promise<void> => invoke("delete_entry", { id }),

  generatePassword: (options: GeneratorOptions): Promise<string> =>
    invoke("generate_password", { options }),

  setIdleTimeout: (secs: number): Promise<Status> =>
    invoke("set_idle_timeout", { secs }),
};
