// Content script — the browser-side safety boundary for Spooky-Pass autofill.
//
// This file embodies the hard rules from DESIGN.md §5:
//   • Fill-on-click only. Nothing is filled on page load; the user must click a
//     small 🦇 affordance, which is a real user gesture.
//   • Never auto-submit. We set field values and dispatch input/change events so
//     frameworks notice, but we never call form.submit() or synthesize a submit.
//   • Never fill cross-origin iframes. Any non-top frame is treated as untrusted
//     in v1 and the script self-disables (no detection, no affordance, no fill).
//   • Warn on HTTP. The affordance shows an insecure-page warning.
//   • Honor the lock state. If the vault is locked we show "Unlock Spooky-Pass"
//     instead of a fill action.
//
// The page URL we hand to the Core is always the TOP-LEVEL location, never a
// frame URL — the Core computes the registrable domain and decides matching.
// A password only ever enters this script inside the `credential` response we
// request on a user click, and it is written straight into the password field
// and never logged.

import type { MatchCandidate } from "./protocol";
import {
  CORE_UNAVAILABLE_MESSAGE,
  type ExtRequest,
  type ExtResponse,
} from "./messages";

// ---------------------------------------------------------------------------
// Iframe guard — the first and most important gate.
// ---------------------------------------------------------------------------

/**
 * True when this frame is the top-level frame the user is actually looking at.
 *
 * In v1 we deliberately refuse to do *anything* in a subframe — even a
 * same-origin one — because the simple, auditable rule "only the top frame is
 * trusted" is far harder to get wrong than per-frame origin gymnastics. The
 * content script is injected with `all_frames: true` only so this guard can run
 * and bail; everything past it is top-frame-only.
 *
 * `window.top === window` is the canonical check. Reading `window.top` across an
 * origin boundary does not throw (only reading its properties would), so this is
 * safe to evaluate in any frame.
 */
export function isTrustedTopFrame(win: Window = window): boolean {
  try {
    return win.top === win.self;
  } catch {
    // A thrown SecurityError can only happen for a cross-origin frame, which is
    // by definition not the top frame from our perspective: treat as untrusted.
    return false;
  }
}

// Self-disable in any non-top frame. No listeners, no observers, no affordance.
if (!isTrustedTopFrame()) {
  // Intentionally do nothing. A cross-origin (or any) iframe gets zero
  // autofill affordance in v1.
} else {
  void main();
}

// ---------------------------------------------------------------------------
// Login-form detection.
// ---------------------------------------------------------------------------

interface LoginForm {
  /** The container scope used to find the associated username field. */
  scope: HTMLElement | Document;
  username: HTMLInputElement | null;
  password: HTMLInputElement;
}

const USERNAME_TYPE_HINTS = ["email", "text", "tel"];

/**
 * Heuristically pick the username field associated with a password input.
 *
 * Strategy (most-specific first):
 *   1. Within the password field's form (or, if none, the document), take the
 *      LAST text/email/tel input that appears BEFORE the password field in DOM
 *      order — login forms put username above password, and "last before" beats
 *      a stray search box at the top of the page.
 *   2. Fall back to the first such field anywhere in scope.
 *
 * Returns null when no plausible username field exists (e.g. a lone change-
 * password form) — we can still fill the password in that case.
 */
export function findUsernameField(
  password: HTMLInputElement,
  scope: HTMLElement | Document,
): HTMLInputElement | null {
  const candidates = Array.from(
    scope.querySelectorAll<HTMLInputElement>("input"),
  ).filter(isPlausibleUsernameInput);

  if (candidates.length === 0) return null;

  const before: HTMLInputElement[] = [];
  for (const input of candidates) {
    // Node.DOCUMENT_POSITION_FOLLOWING set => password follows `input`.
    const pos = input.compareDocumentPosition(password);
    if (pos & Node.DOCUMENT_POSITION_FOLLOWING) before.push(input);
  }

  if (before.length > 0) return before[before.length - 1] ?? null;
  return candidates[0] ?? null;
}

function isPlausibleUsernameInput(input: HTMLInputElement): boolean {
  if (input.disabled || input.readOnly) return false;
  const type = (input.getAttribute("type") ?? "text").toLowerCase();
  if (type === "hidden" || type === "password") return false;
  if (!USERNAME_TYPE_HINTS.includes(type)) return false;
  if (isHidden(input)) return false;
  return true;
}

function isHidden(el: HTMLElement): boolean {
  // Cheap visibility check: we avoid getComputedStyle on every input for perf,
  // but offsetParent === null catches display:none / detached subtrees.
  return el.offsetParent === null && el.getClientRects().length === 0;
}

/** Find every visible login form (password field + its scope) on the page. */
export function detectLoginForms(root: Document = document): LoginForm[] {
  const passwords = Array.from(
    root.querySelectorAll<HTMLInputElement>('input[type="password"]'),
  ).filter((p) => !p.disabled && !isHidden(p));

  const forms: LoginForm[] = [];
  for (const password of passwords) {
    const scope: HTMLElement | Document = password.form ?? root;
    forms.push({
      scope,
      password,
      username: findUsernameField(password, scope),
    });
  }
  return forms;
}

// ---------------------------------------------------------------------------
// Background messaging helpers (thin, typed wrappers).
// ---------------------------------------------------------------------------

function send(req: ExtRequest): Promise<ExtResponse> {
  return new Promise<ExtResponse>((resolve) => {
    try {
      chrome.runtime.sendMessage(req, (resp: ExtResponse | undefined) => {
        // A torn-down service worker / no-listener surfaces via lastError.
        const lastError = chrome.runtime.lastError;
        if (lastError || !resp) {
          resolve({
            ok: false,
            coreUnavailable: true,
            message: CORE_UNAVAILABLE_MESSAGE,
          });
          return;
        }
        resolve(resp);
      });
    } catch {
      resolve({
        ok: false,
        coreUnavailable: true,
        message: CORE_UNAVAILABLE_MESSAGE,
      });
    }
  });
}

// ---------------------------------------------------------------------------
// Main flow.
// ---------------------------------------------------------------------------

const AFFORDANCE_ID = "spooky-pass-affordance";
let activeForm: LoginForm | null = null;

async function main(): Promise<void> {
  attachFocusListeners(document);

  // Re-scan when the DOM changes (SPA login modals, lazy forms). We only attach
  // listeners; nothing is filled until a focus + click happens.
  const observer = new MutationObserver(() => attachFocusListeners(document));
  observer.observe(document.documentElement, {
    childList: true,
    subtree: true,
  });
}

const wired = new WeakSet<HTMLInputElement>();

function attachFocusListeners(root: Document): void {
  for (const form of detectLoginForms(root)) {
    const anchor = form.username ?? form.password;
    if (wired.has(anchor)) continue;
    wired.add(anchor);
    anchor.addEventListener("focus", () => {
      void onFieldFocus(form);
    });
  }
}

async function onFieldFocus(form: LoginForm): Promise<void> {
  activeForm = form;

  // Check lock state FIRST. When the vault is locked the Core returns no
  // matches, so we can't learn whether this site has a saved login — but we
  // still want to offer to unlock on a login page (when a vault exists).
  const status = await send({ kind: "getStatus" });
  if (status.coreUnavailable) {
    renderAffordance(form, { kind: "core-unavailable" });
    return;
  }
  if (status.ok && status.kind === "status" && !status.status.unlocked) {
    if (status.status.hasVault) {
      renderAffordance(form, { kind: "locked" });
    } else {
      // No vault yet — nothing to unlock or fill; don't nag.
      removeAffordance();
    }
    return;
  }

  // Unlocked: ask the Core what matches the TOP-LEVEL page URL. The content
  // script never computes registrable domains — it just sends the URL.
  const resp = await send({ kind: "getMatches", url: topLevelUrl() });
  if (resp.coreUnavailable) {
    renderAffordance(form, { kind: "core-unavailable" });
    return;
  }
  if (!resp.ok || resp.kind !== "matches" || resp.matches.length === 0) {
    // No match (or a transient error): don't nag.
    removeAffordance();
    return;
  }

  renderAffordance(form, { kind: "fill", matches: resp.matches });
}

function topLevelUrl(): string {
  // We are guaranteed to be the top frame here (isTrustedTopFrame gate), so
  // location.href is the top-level URL.
  return location.href;
}

// ---------------------------------------------------------------------------
// Affordance UI — a small, self-contained overlay anchored to the field.
// ---------------------------------------------------------------------------

type AffordanceState =
  | { kind: "fill"; matches: MatchCandidate[] }
  | { kind: "locked" }
  | { kind: "core-unavailable" };

function removeAffordance(): void {
  document.getElementById(AFFORDANCE_ID)?.remove();
}

function renderAffordance(form: LoginForm, state: AffordanceState): void {
  removeAffordance();

  const anchor = form.username ?? form.password;
  const rect = anchor.getBoundingClientRect();

  const box = document.createElement("div");
  box.id = AFFORDANCE_ID;
  Object.assign(box.style, {
    position: "absolute",
    zIndex: "2147483647",
    top: `${window.scrollY + rect.bottom + 4}px`,
    left: `${window.scrollX + rect.left}px`,
    background: "#1b0a2a",
    color: "#f3e8ff",
    font: "13px/1.4 system-ui, sans-serif",
    padding: "6px 10px",
    borderRadius: "8px",
    boxShadow: "0 4px 14px rgba(0,0,0,0.4)",
    cursor: "pointer",
    userSelect: "none",
    maxWidth: "320px",
  } satisfies Partial<CSSStyleDeclaration>);

  const insecure = location.protocol === "http:";

  if (state.kind === "core-unavailable") {
    box.textContent = "🦇 Spooky-Pass: Core not running — open the app";
    box.style.cursor = "default";
    document.body.appendChild(box);
    return;
  }

  if (state.kind === "locked") {
    box.textContent = "🦇 Unlock Spooky-Pass";
    box.title = "Open the Spooky-Pass app to unlock your vault";
    // Clicking the locked affordance just dismisses it; unlocking happens in
    // the desktop app. We surface intent but cannot unlock from a web page.
    box.addEventListener("click", () => removeAffordance());
    if (insecure) appendInsecureWarning(box);
    document.body.appendChild(box);
    return;
  }

  // state.kind === "fill"
  if (state.matches.length === 0) {
    removeAffordance();
    return;
  }

  // Single match: the whole affordance is the fill target.
  if (state.matches.length === 1) {
    const match = state.matches[0];
    if (!match) {
      removeAffordance();
      return;
    }
    const label = document.createElement("div");
    label.textContent = `🦇 Fill ${match.username || match.title}`;
    box.appendChild(label);
    if (insecure) appendInsecureWarning(box);
    // Fill-on-click: a single real user gesture triggers the credential request.
    box.addEventListener("click", () => {
      void fillFromMatch(form, match.id);
    });
    document.body.appendChild(box);
    return;
  }

  // Multiple saved logins for this site: render a picker so every one is
  // reachable (the Core returns all candidates). Each row is its own
  // fill-on-click target.
  box.style.cursor = "default";
  const header = document.createElement("div");
  header.textContent = "🦇 Choose a login to fill";
  header.style.marginBottom = "4px";
  box.appendChild(header);

  for (const match of state.matches) {
    const row = document.createElement("div");
    row.textContent = match.username || match.title || "(no username)";
    Object.assign(row.style, {
      padding: "4px 6px",
      borderRadius: "6px",
      cursor: "pointer",
    } satisfies Partial<CSSStyleDeclaration>);
    row.addEventListener("mouseenter", () => {
      row.style.background = "rgba(255,255,255,0.10)";
    });
    row.addEventListener("mouseleave", () => {
      row.style.background = "transparent";
    });
    // Fill-on-click for this specific entry.
    row.addEventListener("click", () => {
      void fillFromMatch(form, match.id);
    });
    box.appendChild(row);
  }

  if (insecure) appendInsecureWarning(box);
  document.body.appendChild(box);
}

function appendInsecureWarning(box: HTMLElement): void {
  const warn = document.createElement("div");
  warn.textContent = "⚠ Insecure page (HTTP) — credentials sent in the clear";
  warn.style.color = "#ffd27f";
  warn.style.fontSize = "11px";
  warn.style.marginTop = "4px";
  box.appendChild(warn);
}

// ---------------------------------------------------------------------------
// The fill itself — only reached via a user click on the affordance.
// ---------------------------------------------------------------------------

async function fillFromMatch(form: LoginForm, id: string): Promise<void> {
  const resp = await send({ kind: "getCredential", id });
  removeAffordance();

  if (!resp.ok) return; // coreUnavailable or error: do nothing, never log secrets.
  if (resp.kind !== "credential") return;

  const { username, password } = resp.credential;

  if (form.username) setFieldValue(form.username, username);
  setFieldValue(form.password, password);

  // Focus the password field so the user can review and submit themselves.
  // We NEVER call form.submit() or dispatch a submit event.
  form.password.focus();
}

/**
 * Write a value into an input the way a real user would, so framework-bound
 * fields (React/Vue/etc.) register the change. We set the value via the native
 * setter and dispatch input + change. We do NOT dispatch keydown/submit.
 */
function setFieldValue(input: HTMLInputElement, value: string): void {
  const proto = Object.getPrototypeOf(input) as object;
  const desc = Object.getOwnPropertyDescriptor(proto, "value");
  if (desc?.set) {
    desc.set.call(input, value);
  } else {
    input.value = value;
  }
  input.dispatchEvent(new Event("input", { bubbles: true }));
  input.dispatchEvent(new Event("change", { bubbles: true }));
}

// Allow the popup's "fill on this page" button to drive the same flow without a
// field focus. This is still a user gesture (the user clicked the popup button).
chrome.runtime.onMessage.addListener(
  (message: unknown, _sender, sendResponse) => {
    const msg = message as { kind?: string };
    if (!msg || msg.kind !== "popupFill") return false;
    if (!isTrustedTopFrame()) {
      sendResponse({ filled: false, reason: "untrusted-frame" });
      return false;
    }
    void handlePopupFill().then(sendResponse);
    return true;
  },
);

async function handlePopupFill(): Promise<{ filled: boolean; reason?: string }> {
  const forms = detectLoginForms(document);
  const form = activeForm ?? forms[0];
  if (!form) return { filled: false, reason: "no-form" };

  const resp = await send({ kind: "getMatches", url: topLevelUrl() });
  if (!resp.ok || resp.coreUnavailable) {
    return { filled: false, reason: "core-unavailable" };
  }
  if (resp.kind !== "matches" || resp.matches.length === 0) {
    return { filled: false, reason: "no-match" };
  }
  const first = resp.matches[0];
  if (!first) return { filled: false, reason: "no-match" };
  await fillFromMatch(form, first.id);
  return { filled: true };
}
