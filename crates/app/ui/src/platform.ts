// Thin platform helpers for clipboard + opening URLs. Kept dependency-light:
// Tauri's clipboard/opener live in separate plugins, so we use the webview's
// own browser APIs (which Tauri exposes) and degrade gracefully.

/** How long a copied secret lingers before we auto-clear the clipboard. */
const CLIPBOARD_CLEAR_MS = 20_000;

let clipboardClearTimer: number | null = null;

function cancelClipboardClear(): void {
  if (clipboardClearTimer !== null) {
    window.clearTimeout(clipboardClearTimer);
    clipboardClearTimer = null;
  }
}

/** Low-level clipboard write with a legacy fallback. Returns true on success. */
async function writeClipboard(text: string): Promise<boolean> {
  try {
    if (navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(text);
      return true;
    }
  } catch {
    // fall through to legacy path
  }

  try {
    const ta = document.createElement("textarea");
    ta.value = text;
    ta.style.position = "fixed";
    ta.style.opacity = "0";
    document.body.append(ta);
    ta.select();
    const ok = document.execCommand("copy");
    ta.remove();
    return ok;
  } catch {
    return false;
  }
}

/**
 * Copy non-secret text (a username or URL) to the clipboard. Cancels any
 * pending secret auto-clear, since the user has deliberately put something new
 * on the clipboard.
 */
export async function copyToClipboard(text: string): Promise<boolean> {
  cancelClipboardClear();
  return writeClipboard(text);
}

/**
 * Copy a secret (a password) and schedule the clipboard to be cleared a short
 * while later so it doesn't linger. Copying anything else, or locking the
 * vault, cancels the pending clear.
 */
export async function copySecret(text: string): Promise<boolean> {
  cancelClipboardClear();
  const ok = await writeClipboard(text);
  if (ok) {
    clipboardClearTimer = window.setTimeout(() => {
      clipboardClearTimer = null;
      void writeClipboard("");
    }, CLIPBOARD_CLEAR_MS);
  }
  return ok;
}

/**
 * Clear the clipboard now if a copied secret is still pending auto-clear (e.g.
 * on lock). Does nothing if the last copy was a non-secret, so it won't clobber
 * a username the user deliberately copied.
 */
export async function clearClipboard(): Promise<void> {
  const hadPendingSecret = clipboardClearTimer !== null;
  cancelClipboardClear();
  if (hadPendingSecret) {
    await writeClipboard("");
  }
}

/** Open an external URL. Normalizes bare hosts to https://. */
export function openUrl(url: string): void {
  const normalized = /^[a-z][a-z0-9+.-]*:\/\//i.test(url)
    ? url
    : `https://${url}`;
  window.open(normalized, "_blank", "noopener,noreferrer");
}
