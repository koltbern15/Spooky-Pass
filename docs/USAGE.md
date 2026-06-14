# Spooky-Pass — Install & Usage Guide 🦇

A local-first password manager: an encrypted vault, a desktop app to manage it,
and a Chromium extension that autofills logins based on the page URL. Nothing
leaves your machine — no cloud, no account, no sync.

This guide covers building it from source, installing all three moving parts,
and day-to-day use.

---

## 1. How the pieces fit together

```
 Browser page
   │  content script  (detects login form, shows a 🦇 fill-on-click button)
   ▼
 Browser extension (MV3)
   │  native messaging
   ▼
 spooky-pass-host  (a tiny relay — holds no vault, no key)
   │  local socket (owner-only)
   ▼
 Spooky-Pass app  (the "Core": owns the encrypted vault, runs in the tray)
   ▲
   │  you unlock it once with your master password
 Desktop GUI  (create/unlock vault, add/edit/search entries, generate passwords)
```

Three things you install: **the desktop app**, **the native-messaging host**, and
**the browser extension**. The app must be running for autofill to work (it lives
in the tray).

---

## 2. Prerequisites

- **Rust** (stable, 1.74+) — <https://rustup.rs>
- **Node.js 22+** and npm — for building the frontend and the extension
- **A Chromium browser** — Brave or Vivaldi (both are supported by the pinned
  extension ID)
- **Tauri's system libraries** (desktop app only):
  - **Linux:** `libwebkit2gtk-4.1-dev libgtk-3-dev libsoup-3.0-dev libjavascriptcoregtk-4.1-dev librsvg2-dev` (Debian/Ubuntu names)
  - **macOS:** Xcode command-line tools
  - **Windows:** WebView2 runtime (preinstalled on Win 11) + MSVC build tools
- **Optional:** the Tauri CLI for one-command dev/bundle:
  `cargo install tauri-cli` (or `npm i -g @tauri-apps/cli`)

```sh
git clone https://github.com/koltbern15/Spooky-Pass.git
cd Spooky-Pass
```

---

## 3. Build

### 3a. The desktop app (the Core)

The frontend must be built first; Tauri embeds it.

```sh
# build the web UI
cd crates/app/ui
npm install
npm run build          # produces crates/app/ui/dist/
cd ../../..

# build the app
#   with the Tauri CLI (recommended — also produces installers):
cargo tauri build      # bundles land in target/release/bundle/
#   …or a plain dev build/run without the CLI (dist/ must already exist):
cargo run -p spooky-pass-app --release
```

The executable is named **`spooky-pass`** (`target/release/spooky-pass`).

### 3b. The native-messaging host

```sh
cargo build -p spooky-pass-native-host --release
# binary: target/release/spooky-pass-host
```

Note its absolute path — you'll need it in the next step:

```sh
echo "$(pwd)/target/release/spooky-pass-host"
```

### 3c. The browser extension

```sh
cd extension
npm install
npm run build          # produces extension/dist/
cd ..
```

`extension/dist/` is the loadable, unpacked extension.

---

## 4. Install

### 4a. Run the desktop app

Launch `spooky-pass` (the binary from 3a, or the installed bundle). It opens the
main window and adds a 🦇 **tray icon** (Open / Lock now / Quit). Closing the
window **hides it to the tray** — the Core keeps running so autofill stays
available. Quit fully from the tray menu.

### 4b. Install the native-messaging host

1. Open the template **`extension/host/com.spooky_pass.host.json`** and set
   `"path"` to the absolute path of `spooky-pass-host` from step 3b.
   (`allowed_origins` is already pinned to the extension's stable ID.)
2. Copy that file (named exactly `com.spooky_pass.host.json`) into your browser's
   `NativeMessagingHosts` directory:

   | Browser | Linux | macOS |
   | --- | --- | --- |
   | **Brave** | `~/.config/BraveSoftware/Brave-Browser/NativeMessagingHosts/` | `~/Library/Application Support/BraveSoftware/Brave-Browser/NativeMessagingHosts/` |
   | **Vivaldi** | `~/.config/vivaldi/NativeMessagingHosts/` | `~/Library/Application Support/Vivaldi/NativeMessagingHosts/` |

   On **Windows**, place the JSON anywhere and register its path under
   `HKCU\Software\<Browser>\NativeMessagingHosts\com.spooky_pass.host` (see
   `extension/README.md` for the exact keys).

### 4c. Load the extension

1. Go to `chrome://extensions` (or `brave://extensions` / `vivaldi://extensions`).
2. Enable **Developer mode**.
3. **Load unpacked** → select `extension/dist/`.
4. Confirm the extension ID is **`mdefkabhkkpajkmoakenbfkchkhaffnc`** (this is
   what the host manifest's `allowed_origins` expects). Restart the browser so it
   picks up the native-messaging host.

---

## 5. First run & everyday use

### Create your vault
On first launch the app shows a **Create vault** screen. Choose a strong **master
password** (and confirm it). This is the only password you must remember — it
derives the key that encrypts everything. **There is no recovery if you forget
it** (that's the point — it's never stored).

### Unlock
After that, launching the app (or returning after auto-lock) shows the **Unlock**
screen. Enter your master password. The vault stays unlocked for the session and
**auto-locks after 15 minutes idle** (or immediately via tray → *Lock now*).

### Manage entries (desktop app)
- **Add** a login: title, username, password, one or more URLs, notes.
- **Search** the list (filters by title / username / URL).
- **Open** an entry to reveal/copy the password, copy the username, or open a URL.
- **Edit / Delete** from the detail view.
- **Generate** strong passwords (length + character classes + exclude-ambiguous),
  standalone or inline while adding/editing.

Every change is saved immediately to the encrypted vault file.

### Autofill in the browser
1. Make sure the **app is running** (tray) and the vault is **unlocked**.
2. Visit a site you have a saved login for. Focus the username/password field — a
   small **🦇 button** appears if Spooky-Pass has a match for that site.
3. **Click it** to fill. Spooky-Pass **never auto-fills on load, never
   auto-submits**, and **never fills inside cross-origin iframes**.
4. On `http://` pages it shows an insecure-page warning. If the vault is locked it
   offers **"Unlock Spooky-Pass"** instead (unlock in the app, then retry).

Matching is by **registrable domain (eTLD+1)** — a login saved on
`accounts.google.com` also fills `mail.google.com`, but a lookalike like
`google.com.evil.ru` will **not** match.

---

## 6. Where your data lives

| What | Linux | macOS |
| --- | --- | --- |
| **Vault** (`vault.spk`) | `~/.local/share/SpookyPass/` | `~/Library/Application Support/dev.SpookyPass.SpookyPass/` |
| **IPC socket** | `$XDG_RUNTIME_DIR/spooky-pass.sock` | app data dir (owner-only `0600`) |

The vault file is written **atomically and `0600`** (owner-only). It's safe to
back up the `vault.spk` file — it's useless without your master password. To move
to a new machine, copy `vault.spk` into the same data directory there.

---

## 7. Security model (what it does and doesn't protect)

- ✅ **Encrypted at rest:** Argon2id-derived key + XChaCha20-Poly1305 AEAD. A
  stolen/copied `vault.spk` is opaque without the master password.
- ✅ **No-wrong-site autofill:** strict eTLD+1 matching; fill only on your click.
- ✅ **Owner-only files & socket** (`0600`); the host holds no key; a password
  crosses to the browser only when you click *fill*.
- ⚠️ **Out of scope (by design):** malware already running as *you* on your
  machine, and hardening individual secret strings in process memory. Spooky-Pass
  is convenience-first for personal use, not a defense against a compromised OS.

---

## 8. Troubleshooting

- **No 🦇 button appears.** Check, in order: the app is running (tray icon) and
  unlocked; you actually have a saved entry whose URL matches this site's
  registrable domain; the extension is loaded with ID
  `mdefkabhkkpajkmoakenbfkchkhaffnc`; the host manifest is installed with the
  correct absolute `path` and you restarted the browser after installing it.
- **"Core not running" in the button.** Start the desktop app (it must be running
  for the browser to reach the vault).
- **"Unlock Spooky-Pass" button.** The vault auto-locked — unlock it in the app.
- **Autofill stopped after moving the host binary.** The path is baked into the
  host manifest; update `"path"` and restart the browser.

---

## 9. Uninstall

1. Remove the extension from `chrome://extensions`.
2. Delete `com.spooky_pass.host.json` from the browser's `NativeMessagingHosts`
   directory (and the Windows registry key, if applicable).
3. Quit the app from the tray and remove the binary / bundle.
4. To erase your data, delete the `vault.spk` file from the data directory above.

---

*Spooky-Pass is early software (v0.1). The browser autofill round-trip is best
verified manually on your machine; the cryptographic core, the desktop logic, and
the URL-matching are covered by an automated test suite.*
