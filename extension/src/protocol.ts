// FROZEN: the TypeScript mirror of crates/app-core/src/autofill/contract.rs.
// Keep field-for-field in lockstep (camelCase).
//
// The extension never bundles or consults the Public Suffix List — it sends a
// raw top-level page URL via `getMatches` and the Core computes the registrable
// domain and decides. A password appears in exactly one message: the
// `credential` response, returned only after a user click.

export interface MatchCandidate {
  id: string;
  title: string;
  username: string;
  primaryUrl: string | null;
}

export interface CredentialDto {
  id: string;
  username: string;
  password: string;
}

export interface AutofillStatus {
  unlocked: boolean;
  hasVault: boolean;
}

// Requests: extension -> native-host -> Core. Internally tagged by `type`.
export type IpcRequest =
  | { type: "getStatus" }
  | { type: "getMatches"; url: string }
  | { type: "getCredential"; id: string }
  | {
      type: "saveLogin";
      url: string;
      username: string;
      password: string;
      title?: string;
    };

// Responses: Core -> native-host -> extension. Internally tagged by `type`.
export type IpcResponse =
  | { type: "status"; unlocked: boolean; hasVault: boolean }
  | { type: "matches"; matches: MatchCandidate[] }
  | { type: "credential"; id: string; username: string; password: string }
  | { type: "saved"; id: string }
  | { type: "error"; kind: string; message?: string };

// Native-messaging transport envelope: the background worker tags each request
// with `reqId` to correlate the asynchronous response; the host echoes it back.
export interface RequestEnvelope {
  reqId: number;
  request: IpcRequest;
}

export interface ResponseEnvelope {
  reqId: number;
  response: IpcResponse;
}
