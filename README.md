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

**Phase 1 — `vault-core` (done):** the cryptographic core now exists at
[`crates/vault-core`](./crates/vault-core). It implements the encrypted vault
file format, Argon2id key derivation, XChaCha20-Poly1305 AEAD, the
locked/unlocked typestate with entry CRUD, atomic save, and a CSPRNG-backed
password generator — all unit- and integration-tested. The desktop app,
native-messaging host, and browser extension are reserved for later phases.
