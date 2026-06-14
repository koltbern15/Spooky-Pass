// Internal messages between extension contexts (content/popup -> background)
// over `chrome.runtime.sendMessage`. These are distinct from the FROZEN native
// protocol in `protocol.ts`: the background worker translates these into
// `RequestEnvelope`s on the native port and unwraps the `IpcResponse` back into
// these results. Keeping the boundary explicit means the content script never
// touches the native transport and never sees a password except inside the
// `credential` result it explicitly asked for on a user click.

import type { AutofillStatus, CredentialDto, MatchCandidate } from "./protocol";

/** content/popup -> background */
export type ExtRequest =
  | { kind: "getStatus" }
  | { kind: "getMatches"; url: string }
  | { kind: "getCredential"; id: string };

/**
 * background -> content/popup. Every reply carries `coreUnavailable` so callers
 * can distinguish "the Core answered" from "the Core (or its native host) is not
 * running" — the latter is the cue to offer the user a way to start it.
 */
export type ExtResponse =
  | { ok: true; coreUnavailable: false; kind: "status"; status: AutofillStatus }
  | {
      ok: true;
      coreUnavailable: false;
      kind: "matches";
      matches: MatchCandidate[];
    }
  | {
      ok: true;
      coreUnavailable: false;
      kind: "credential";
      credential: CredentialDto;
    }
  | { ok: false; coreUnavailable: true; message: string }
  | { ok: false; coreUnavailable: false; message: string };

export const CORE_UNAVAILABLE_MESSAGE =
  "Spooky-Pass Core is not running. Start the desktop app to enable autofill.";
