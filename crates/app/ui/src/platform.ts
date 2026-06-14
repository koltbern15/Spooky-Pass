// Thin platform helpers for clipboard + opening URLs. Kept dependency-light:
// Tauri's clipboard/opener live in separate plugins, so we use the webview's
// own browser APIs (which Tauri exposes) and degrade gracefully.

/** Copy text to the clipboard; returns true on success. */
export async function copyToClipboard(text: string): Promise<boolean> {
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

/** Open an external URL. Normalizes bare hosts to https://. */
export function openUrl(url: string): void {
  const normalized = /^[a-z][a-z0-9+.-]*:\/\//i.test(url)
    ? url
    : `https://${url}`;
  window.open(normalized, "_blank", "noopener,noreferrer");
}
