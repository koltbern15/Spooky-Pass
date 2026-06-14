# Spooky-Pass browser extension (MV3)

Fill-on-click autofill for Chromium-based browsers (Brave + Vivaldi). This is the
thin autofill client described in [`DESIGN.md`](../DESIGN.md) §3: it detects login
forms, asks the Spooky-Pass **Core** (via a native-messaging host) what matches the
page, and — only on a real user click — fills the username and password.

## Safety rules (enforced in `src/content.ts`)

- **Fill-on-click only.** Nothing is filled on page load. The user clicks a small
  🦇 affordance (a genuine user gesture) to fill.
- **Never auto-submit.** Values are set and `input`/`change` events dispatched so
  frameworks notice, but `form.submit()` is never called and no submit is synthesized.
- **Never fill cross-origin iframes.** Any non-top frame is treated as untrusted in
  v1 (`isTrustedTopFrame`): the content script self-disables — no detection, no
  affordance, no fill — even though it is injected with `all_frames: true`.
- **Warn on HTTP.** The affordance shows an insecure-page warning on `http:` pages.
- **Locked vault.** If the Core reports `unlocked: false`, the affordance offers
  "Unlock Spooky-Pass" (unlocking happens in the desktop app) instead of filling.
- **Core does the matching.** The extension never bundles the Public Suffix List; it
  sends the **top-level** page URL via `getMatches` and the Core computes the
  registrable domain (eTLD+1) and decides.
- A password appears in exactly one message — the `credential` response — requested
  only after a user click, written straight into the field, and never logged.

## Build

```sh
npm install        # generates package-lock.json (CI uses `npm ci`)
npm run typecheck  # tsc --noEmit
npm run lint       # eslint src
npm run build      # vite -> dist/
```

`dist/` contains `background.js`, `content.js`, `popup.js`, `popup.html`, and
`manifest.json`. Load it unpacked via `chrome://extensions` → "Load unpacked".

## Fixed extension key / ID strategy

`manifest.json` pins a fixed RSA public `key`, which gives the extension a **stable
ID across machines and unpacked reloads**:

```
mdefkabhkkpajkmoakenbfkchkhaffnc
```

This stability is what lets the native-messaging host manifest's `allowed_origins`
name a single, unchanging `chrome-extension://…` origin. Without a pinned key,
Chromium derives a per-installation ID and the host manifest would have to be patched
on every machine.

The ID is the SHA-256 of the DER-decoded public key, first 16 bytes, with each hex
nibble mapped `0–f → a–p`. To recompute it from the `key` field:

```sh
node -e 'const c=require("crypto");const k=process.argv[1];const d=Buffer.from(k,"base64");const h=c.createHash("sha256").update(d).digest();console.log([...h.subarray(0,16)].map(b=>b.toString(16).padStart(2,"0")).join("").split("").map(x=>String.fromCharCode(97+parseInt(x,16))).join(""))' "$(node -e 'console.log(require("./manifest.json").key)')"
```

If you ever rotate the key, regenerate the ID and update both `manifest.json`'s
`key` and every host manifest's `allowed_origins` (and `host/com.spooky_pass.host.json`
below).

## Native messaging host install

The extension talks to the Core through a native-messaging host named
`com.spooky_pass.host`. [`host/com.spooky_pass.host.json`](host/com.spooky_pass.host.json)
is a **template**: before installing, replace `path` with the absolute path to the
built host binary (the thin stdio shim that forwards to the running Core). The
`allowed_origins` entry already pins the stable extension origin above.

> Manual install only — wiring an installer for Brave + Vivaldi paths is out of
> scope for this phase (see `DESIGN.md` §9).

Copy the (path-patched) manifest to the per-browser, per-OS location below. The file
**must** be named `com.spooky_pass.host.json`.

### Brave

| OS      | Location |
| ------- | -------- |
| Linux   | `~/.config/BraveSoftware/Brave-Browser/NativeMessagingHosts/com.spooky_pass.host.json` |
| macOS   | `~/Library/Application Support/BraveSoftware/Brave-Browser/NativeMessagingHosts/com.spooky_pass.host.json` |
| Windows | `%LOCALAPPDATA%\BraveSoftware\Brave-Browser\User Data\NativeMessagingHosts\com.spooky_pass.host.json`, registered under `HKCU\Software\BraveSoftware\Brave-Browser\NativeMessagingHosts\com.spooky_pass.host` pointing at the JSON file. |

### Vivaldi

| OS      | Location |
| ------- | -------- |
| Linux   | `~/.config/vivaldi/NativeMessagingHosts/com.spooky_pass.host.json` |
| macOS   | `~/Library/Application Support/Vivaldi/NativeMessagingHosts/com.spooky_pass.host.json` |
| Windows | `%LOCALAPPDATA%\Vivaldi\User Data\NativeMessagingHosts\com.spooky_pass.host.json`, registered under `HKCU\Software\Vivaldi\NativeMessagingHosts\com.spooky_pass.host` pointing at the JSON file. |

On Windows, the JSON file may live anywhere; the `HKCU` registry value (default value
of the key, a string) holds its absolute path. On Linux/macOS, dropping the JSON into
the `NativeMessagingHosts` directory is sufficient.

After installing, restart the browser and confirm autofill works on a saved site
while the Core is running.
