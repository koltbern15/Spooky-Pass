# Spooky-Pass — Design Document

> A scary-good, local-first password manager with browser autofill. 🦇

This document captures the design decisions for Spooky-Pass. It is the output
of a deliberate brainstorming pass: every choice below was made on purpose, and
the "why" matters as much as the "what."

---

## 1. Philosophy & Goals

Spooky-Pass is a **personal, local-first** password manager. The guiding
principle is:

> **Convenience-first everywhere — careful *only* about URL matching.**

We deliberately optimize for a smooth daily-driver experience and consciously
skip heavyweight hardening. The one place we refuse to be casual is deciding
*which site a credential belongs to*, because that is the difference between
"fills my bank login on my bank" and "fills my bank login on a phishing clone."

### Goals
- Store login credentials encrypted **at rest** with real, modern crypto.
- **Autofill** logins into websites based on the page's URL.
- A real **desktop GUI** to view, search, edit, and generate passwords.
- **Capture** new logins as you create them ("save on login").
- Stay out of the way: unlock once, stay unlocked for the session.

### Non-goals (for now)
- Not trying to defend against malware already running as you, memory-scraping,
  or a determined local attacker. (See Threat Model.)
- No cloud account, no server, no multi-device sync in v1.
- No enterprise/team features, sharing, or policy controls.

---

## 2. Threat Model

Spooky-Pass targets a **casual, convenience-first** threat level, with one
upgrade forced by the autofill feature.

| Threat | In scope? | How we handle it |
| --- | --- | --- |
| Vault file is copied / laptop stolen | ✅ Yes | Vault is encrypted at rest; useless without the master password. |
| Filling a credential into the **wrong site** (phishing) | ✅ Yes — the *one* careful thing | Strict registrable-domain matching, fill-on-click, never auto-submit, never fill cross-origin iframes. |
| Malware / another local app snooping memory | ❌ Out of scope | We do not defend against code already running as you. |
| Network attacker | ❌ N/A | Nothing leaves the machine; no sync, no server. |

**Why autofill raises the bar:** a passive "encrypted box" only has to keep a
stolen file opaque. An *active* agent that types credentials into web pages can
leak a secret to the wrong origin if its URL matching is sloppy. That single
risk is treated as a correctness requirement, not optional hardening.

---

## 3. Architecture

A browser extension is **sandboxed**: it cannot read files on disk, and a
native desktop app cannot read the extension's private storage. So the vault
cannot literally live "inside the extension" *and* be visible to a desktop app
without a sync server in the middle. Instead we use **one source of truth that
both clients talk to.**

```
                ┌─────────────────────────────────────┐
                │          Spooky-Pass Core            │
                │  • owns the encrypted vault file     │
                │  • master pass → derives key (Argon2)│
                │  • holds unlocked key in memory      │
                │    ("remember me") + auto-lock timer │
                │  • runs in tray, starts on login     │
                └─────────────────────────────────────┘
                     ▲                          ▲
        native messaging                   in-process
          (length-prefixed JSON              IPC / direct
           over a thin host shim)            calls
                     │                          │
        ┌────────────┴───────────┐   ┌──────────┴────────────┐
        │   Browser extension    │   │     Desktop GUI app    │
        │   (Brave + Vivaldi)    │   │  see / manage / search │
        │  detect URL → fill     │   │  add / edit / generate │
        └────────────────────────┘   └───────────────────────┘
```

### Components

1. **Core** — the vault owner. Holds the encrypted vault file, derives the key
   from the master password, keeps the unlocked key in memory during a
   "remember me" session, enforces the auto-lock timeout, and answers
   credential requests. Runs persistently in the system tray and starts on
   login. This is where all the secrets live at runtime.

2. **Desktop GUI** — the management window. View everything, search, add/edit
   entries, generate passwords, change settings. Talks to the Core in-process
   (same Tauri app/process as the Core, or a thin local IPC).

3. **Browser extension** — the autofill client. Detects login forms, reads the
   page origin, asks the Core "do you have credentials for this registrable
   domain?", and (on click) fills them. Also offers to capture new logins.

Because there is exactly **one vault** owned by the Core, the extension and the
app are always in sync. The extension *feels* like the vault is in the browser,
but it is really a thin client.

### The tradeoff we accepted
The Core must be running for autofill to work. That's why it lives in the
**tray and auto-starts on login** — the same behavior as 1Password / Bitwarden
desktop. If the Core is closed, the management app still opens, but the browser
can't autofill until it's up.

### Core ↔ extension channel
- Transport: **Chrome/Chromium native messaging** (length-prefixed JSON over
  stdio). Works in both Brave and Vivaldi.
- The native messaging host is a **thin shim** that forwards to the
  long-running Core over local IPC (Unix socket / named pipe), so the unlocked
  key lives in the persistent Core, not in a per-connection spawned process.
- Trust: the native messaging host manifest restricts `allowed_origins` to the
  Spooky-Pass extension ID, so arbitrary web pages or other extensions cannot
  impersonate the extension.

---

## 4. Security Design

### Key derivation & encryption
- **KDF:** Argon2id. A per-vault random salt is stored in the vault header.
  Parameters tuned for a desktop (starting point: 64 MiB memory, 3 iterations,
  parallelism 1) and recorded in the header so they can evolve.
- **Cipher:** XChaCha20-Poly1305 (AEAD). The 192-bit nonce makes random nonces
  safe and sidesteps nonce-reuse footguns. (AES-256-GCM is an acceptable
  alternative.)
- **What's encrypted:** the entire vault body. The header (magic, version, KDF
  params, salt, nonce) is plaintext; everything else is one AEAD blob.

### Vault file format (sketch)
```
┌───────────────────────────── header (plaintext) ─────────────────────────────┐
│ magic "SPK1" │ version │ kdf params │ salt │ nonce                            │
├──────────────────────────── body (AEAD-encrypted) ───────────────────────────┤
│ JSON: [ { id, title, username, password, urls[], notes, created, updated } ]  │
└───────────────────────────────────────────────────────────────────────────────┘
```
A single encrypted file is plenty for personal scale and keeps the format dead
simple. (If we ever need partial decryption or huge vaults, revisit SQLCipher.)

### Unlock / session model
- The **master password is never stored.** It derives the key; only the derived
  key is held in memory, and only while unlocked.
- **"Remember me"** keeps the derived key in the Core's memory for the session
  so autofill is instant (without it you'd retype the master password on every
  login form — autofill basically *requires* a live unlocked session).
- **Auto-lock** zeroizes the key after a configurable idle timeout and on
  explicit lock / full quit. Generous default; user-configurable.

---

## 5. URL Matching & Autofill Safety (the careful part)

Two layers. Layer 1 is non-negotiable engineering; Layer 2 is the user-facing
convenience-vs-safety dial.

### Layer 1 — how a site "matches"
- Match on the **registrable domain (eTLD+1)** computed from the
  **Public Suffix List**. So a login saved on `accounts.google.com` also works
  on `mail.google.com` (both → `google.com`), while `google.com.evil.ru` does
  **not** match.
- **Never** use naive substring matching (`url.contains("google.com")`) — that
  is precisely the bug that hands passwords to phishers.
- **Per-entry overrides** for edge cases: exact-host-only, or additional
  allowed domains.

### Layer 2 — how pushy autofill is
- **Default: fill-on-click.** Spooky-Pass surfaces a small 🦇 affordance in the
  username field (or via the toolbar icon); one click fills. You *see* which
  site you're confirming before anything is entered.
- **Hard rules, always:**
  - Never **auto-submit** the form.
  - Never fill into a **cross-origin iframe** (classic autofill leak vector).
  - Prefer **HTTPS**; warn on HTTP.
- **Opt-in per trusted site:** a site can be marked "auto-fill on load" for the
  convenience junkies, but that is never the default.

---

## 6. MVP Scope

**In v1:**
- 🔑 Store login credentials (title, username, password, URLs, notes).
- 🧠 Autofill into matching sites (fill-on-click, per the rules above).
- 🎲 Password generator (length, character classes; sensible strong default).
- 💾 Save-on-login capture (offer to save a new credential when one is entered).

**Deferred (not v1):**
- TOTP / 2FA code storage & autofill
- Secure notes / other item types
- Import / export (e.g. CSV, other managers)
- Cloud sync / multi-device
- Mobile, Firefox, Safari

---

## 7. Tech Stack

| Piece | Tech | Why |
| --- | --- | --- |
| Core + Desktop GUI | **Tauri (Rust)** | First-class crypto crates (`argon2`, `chacha20poly1305`), tiny single binary, far lighter than Electron, native tray + autostart. |
| Browser extension | **TypeScript** | Browsers only run JS/WASM; TS can share types/UI with the Tauri webview. |
| Browsers | **Chromium (Brave + Vivaldi)** | Both Chromium → one extension covers both; native messaging supported in both. |

Likely Rust crates: `argon2`, `chacha20poly1305`, `zeroize`, `serde`,
`publicsuffix` (or an embedded PSL), `tauri`. Native messaging host can be a
small Rust binary shim.

---

## 8. Rough Roadmap

1. **Vault core** — file format, Argon2id + XChaCha20-Poly1305, lock/unlock,
   in-memory session, auto-lock. Unit-tested in isolation.
2. **Desktop GUI** — create/unlock vault, list/add/edit entries, generator.
3. **Native messaging bridge** — Core ↔ extension channel + host manifest.
4. **Extension** — form detection, eTLD+1 matching, fill-on-click, the hard
   safety rules.
5. **Save-on-login** — capture flow end to end.
6. **Tray + autostart** polish.

---

## 9. Open Questions / Risks

- **PSL freshness:** the Public Suffix List changes over time; decide between
  embedding a snapshot vs. periodic refresh.
- **Native messaging UX:** registering the host manifest per browser/OS is
  fiddly; the installer must handle Brave + Vivaldi paths.
- **Auto-lock vs. autofill convenience:** tune the default timeout so it isn't
  annoying but isn't "unlocked forever."
- **Form detection robustness:** login forms vary wildly; expect iteration.
