// MV3 service worker: the only context that talks to the native messaging host.
//
// Responsibilities:
//   • Own a (lazily reconnected) native port to "com.spooky_pass.host".
//   • Correlate each outbound request with its response via a client-generated
//     `reqId`, using the FROZEN Request/ResponseEnvelope contract.
//   • Translate internal `ExtRequest`s from content/popup into native requests
//     and unwrap the `IpcResponse` back into `ExtResponse`s.
//   • Surface a "Core not running" (`coreUnavailable`) state cleanly: the SW is
//     ephemeral and the host may be down, so a failed connect/disconnect must
//     not wedge the extension.

import type {
  IpcRequest,
  IpcResponse,
  RequestEnvelope,
  ResponseEnvelope,
} from "./protocol";
import {
  CORE_UNAVAILABLE_MESSAGE,
  type ExtRequest,
  type ExtResponse,
} from "./messages";

const HOST_NAME = "com.spooky_pass.host";

// Drop a connection that produced no traffic; a fresh send reconnects. Keeps us
// off the native host when idle without re-handshaking on every burst.
const IDLE_DISCONNECT_MS = 30_000;

interface Pending {
  resolve: (response: IpcResponse) => void;
  reject: (error: Error) => void;
}

let port: chrome.runtime.Port | null = null;
let nextReqId = 1;
let idleTimer: ReturnType<typeof setTimeout> | null = null;
const pending = new Map<number, Pending>();

/** Fail every in-flight request, e.g. when the port drops. */
function rejectAllPending(error: Error): void {
  for (const [, p] of pending) p.reject(error);
  pending.clear();
}

function clearIdleTimer(): void {
  if (idleTimer !== null) {
    clearTimeout(idleTimer);
    idleTimer = null;
  }
}

function armIdleTimer(): void {
  clearIdleTimer();
  idleTimer = setTimeout(() => {
    if (pending.size === 0 && port) port.disconnect();
  }, IDLE_DISCONNECT_MS);
}

/**
 * Connect (or reuse) the native port. Throws if the host cannot be reached so
 * callers can map that to `coreUnavailable`.
 */
function ensurePort(): chrome.runtime.Port {
  if (port) return port;

  // `connectNative` itself does not throw synchronously when the host is
  // missing — the failure arrives later via `onDisconnect` with
  // `chrome.runtime.lastError`. We handle both paths.
  const p = chrome.runtime.connectNative(HOST_NAME);
  port = p;

  p.onMessage.addListener((raw: unknown) => {
    const envelope = raw as ResponseEnvelope;
    if (
      !envelope ||
      typeof envelope.reqId !== "number" ||
      !envelope.response
    ) {
      // Ignore malformed frames rather than crashing the worker.
      return;
    }
    const entry = pending.get(envelope.reqId);
    if (!entry) return;
    pending.delete(envelope.reqId);
    entry.resolve(envelope.response);
    if (pending.size === 0) armIdleTimer();
  });

  p.onDisconnect.addListener(() => {
    const lastError = chrome.runtime.lastError;
    port = null;
    clearIdleTimer();
    const reason = lastError?.message
      ? `${CORE_UNAVAILABLE_MESSAGE} (${lastError.message})`
      : CORE_UNAVAILABLE_MESSAGE;
    rejectAllPending(new CoreUnavailableError(reason));
  });

  return p;
}

/** Marker error: the native host / Core could not be reached. */
class CoreUnavailableError extends Error {
  override readonly name = "CoreUnavailableError";
}

/**
 * Send one native request and await its correlated response. The Promise is
 * keyed by a fresh `reqId`; the host echoes the id so we can resolve the right
 * caller even with several requests in flight.
 */
function sendNative(request: IpcRequest): Promise<IpcResponse> {
  return new Promise<IpcResponse>((resolve, reject) => {
    let p: chrome.runtime.Port;
    try {
      p = ensurePort();
    } catch {
      reject(new CoreUnavailableError(CORE_UNAVAILABLE_MESSAGE));
      return;
    }

    const reqId = nextReqId++;
    pending.set(reqId, { resolve, reject });
    clearIdleTimer();

    const envelope: RequestEnvelope = { reqId, request };
    try {
      p.postMessage(envelope);
    } catch {
      pending.delete(reqId);
      port = null;
      reject(new CoreUnavailableError(CORE_UNAVAILABLE_MESSAGE));
    }
  });
}

/** Translate an internal request into a native one and unwrap the response. */
async function handleExtRequest(req: ExtRequest): Promise<ExtResponse> {
  try {
    let response: IpcResponse;
    switch (req.kind) {
      case "getStatus":
        response = await sendNative({ type: "getStatus" });
        break;
      case "getMatches":
        response = await sendNative({ type: "getMatches", url: req.url });
        break;
      case "getCredential":
        response = await sendNative({ type: "getCredential", id: req.id });
        break;
    }
    return unwrap(req, response);
  } catch (error) {
    if (error instanceof CoreUnavailableError) {
      return { ok: false, coreUnavailable: true, message: error.message };
    }
    const message = error instanceof Error ? error.message : "Unknown error";
    return { ok: false, coreUnavailable: false, message };
  }
}

function unwrap(req: ExtRequest, response: IpcResponse): ExtResponse {
  if (response.type === "error") {
    const message = response.message
      ? `${response.kind}: ${response.message}`
      : response.kind;
    return { ok: false, coreUnavailable: false, message };
  }

  switch (req.kind) {
    case "getStatus":
      if (response.type === "status") {
        return {
          ok: true,
          coreUnavailable: false,
          kind: "status",
          status: {
            unlocked: response.unlocked,
            hasVault: response.hasVault,
          },
        };
      }
      break;
    case "getMatches":
      if (response.type === "matches") {
        return {
          ok: true,
          coreUnavailable: false,
          kind: "matches",
          matches: response.matches,
        };
      }
      break;
    case "getCredential":
      if (response.type === "credential") {
        return {
          ok: true,
          coreUnavailable: false,
          kind: "credential",
          credential: {
            id: response.id,
            username: response.username,
            password: response.password,
          },
        };
      }
      break;
  }
  return {
    ok: false,
    coreUnavailable: false,
    message: `Unexpected response type "${response.type}" for "${req.kind}"`,
  };
}

// Route messages from content scripts and the popup. Returning `true` keeps the
// message channel open for the async `sendResponse`.
chrome.runtime.onMessage.addListener(
  (message: unknown, _sender, sendResponse) => {
    const req = message as ExtRequest;
    if (
      !req ||
      (req.kind !== "getStatus" &&
        req.kind !== "getMatches" &&
        req.kind !== "getCredential")
    ) {
      sendResponse({
        ok: false,
        coreUnavailable: false,
        message: "Unknown message",
      } satisfies ExtResponse);
      return false;
    }
    handleExtRequest(req).then(sendResponse);
    return true;
  },
);
