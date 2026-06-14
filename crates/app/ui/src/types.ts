// Canonical TypeScript mirror of the app-core DTO/error contract.
//
// FROZEN: keep this in lockstep with crates/app-core/src/dto.rs and error.rs.
// All field names are camelCase to match serde's `rename_all = "camelCase"`.

export interface Status {
  hasVault: boolean;
  unlocked: boolean;
  idleTimeoutSecs: number;
}

export interface EntrySummary {
  id: string;
  title: string;
  username: string;
  primaryUrl: string | null;
  updated: string;
}

export interface EntryView {
  id: string;
  title: string;
  username: string;
  password: string;
  urls: string[];
  notes: string;
  created: string;
  updated: string;
}

export interface EntryInput {
  title: string;
  username: string;
  password: string;
  urls: string[];
  notes: string;
}

export interface EntryPatchInput {
  title?: string;
  username?: string;
  password?: string;
  urls?: string[];
  notes?: string;
}

export interface GeneratorOptions {
  length: number;
  lowercase: boolean;
  uppercase: boolean;
  digits: boolean;
  symbols: boolean;
  excludeAmbiguous: boolean;
}

// Serialized shape of app-core's AppError: `{ kind, message? }`.
export type AppErrorKind =
  | "WrongPassword"
  | "Locked"
  | "VaultAlreadyExists"
  | "NoVault"
  | "EntryNotFound"
  | "InvalidPasswordOptions"
  | "InvalidVaultFile"
  | "Internal";

export interface AppError {
  kind: AppErrorKind;
  message?: string;
}
