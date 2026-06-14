# Spooky-Pass 🦇

A scary-good, **local-first password manager** with browser autofill.

Spooky-Pass keeps your logins encrypted on your own machine and fills them into
websites based on the page's URL — no cloud account, no server, no sync. Unlock
once with a master password, stay unlocked for the session, and let the 🦇 do
the typing.

## What it is

- **Local-first & private** — your vault never leaves your computer.
- **Encrypted at rest** — Argon2id-derived key + XChaCha20-Poly1305 AEAD.
- **Browser autofill** — a Chromium extension (Brave + Vivaldi) fills logins
  based on the registrable domain, on click, never auto-submitting.
- **Desktop app** — view, search, edit, and generate passwords from a real GUI.
- **Save-on-login** — offers to capture new credentials as you create them.

## Architecture at a glance

A small **Core** owns the encrypted vault and runs in the tray; a **desktop
GUI** manages it, and a **browser extension** asks the Core for credentials to
autofill. One vault, one source of truth.

```
 Browser extension ──native messaging──▶ Spooky-Pass Core ◀──in-process── Desktop GUI
       (autofill)                        (encrypted vault)                 (manage)
```

## Stack

- **Core + Desktop app:** Tauri (Rust)
- **Browser extension:** TypeScript (Chromium — Brave & Vivaldi)

## Status

🚧 Early development. See **[DESIGN.md](./DESIGN.md)** for the full
architecture, threat model, security design, and roadmap.

- **Phase 1 — `vault-core` ✅ done:** the cryptographic core
  ([`crates/vault-core`](./crates/vault-core)) — encrypted vault file format,
  Argon2id key derivation, XChaCha20-Poly1305 AEAD, the locked/unlocked
  typestate with entry CRUD, atomic `0600` save, and a CSPRNG password
  generator. Fully unit- and integration-tested.
- **Phase 2 — desktop app ✅ done:** a Tauri Core + GUI built on a Tauri-free,
  fully-tested [`crates/app-core`](./crates/app-core) (session model,
  injectable-clock idle auto-lock, vault-file management, service layer) plus
  the [`crates/app`](./crates/app) Tauri shell (commands, tray, autostart) and
  a plain-TypeScript UI (create / unlock / search / edit / generate).
- **Phase 3 — native-messaging host 🚧 in progress:** the bridge that lets the
  browser reach the running Core.
- **Phase 4 — Chromium extension 🚧 in progress:** login-form detection and
  eTLD+1 / fill-on-click autofill for Brave & Vivaldi.
