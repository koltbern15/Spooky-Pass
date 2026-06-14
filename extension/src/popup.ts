// Toolbar popup — status surface plus a manual "fill on this page" action.
//
// The popup never touches the native port directly; it asks the background
// worker for status (via the typed internal protocol) and, on the user's
// click, tells the active tab's content script to perform a fill. As with the
// in-page affordance, filling only ever happens on a real user gesture and the
// popup never sees a password.

import { type ExtRequest, type ExtResponse } from "./messages";

const badge = document.getElementById("badge") as HTMLSpanElement;
const hint = document.getElementById("hint") as HTMLParagraphElement;
const fillButton = document.getElementById("fill") as HTMLButtonElement;

function send(req: ExtRequest): Promise<ExtResponse> {
  return new Promise<ExtResponse>((resolve) => {
    chrome.runtime.sendMessage(req, (resp: ExtResponse | undefined) => {
      const lastError = chrome.runtime.lastError;
      if (lastError || !resp) {
        resolve({
          ok: false,
          coreUnavailable: true,
          message: lastError?.message ?? "No response from background worker.",
        });
        return;
      }
      resolve(resp);
    });
  });
}

function setBadge(cls: "ok" | "warn" | "err", text: string): void {
  badge.className = `badge ${cls}`;
  badge.textContent = text;
}

async function refreshStatus(): Promise<void> {
  const resp = await send({ kind: "getStatus" });

  if (resp.coreUnavailable) {
    setBadge("err", "Core offline");
    hint.textContent =
      "Spooky-Pass Core is not running. Open the desktop app to enable autofill.";
    fillButton.disabled = true;
    return;
  }

  if (!resp.ok) {
    setBadge("err", "Error");
    hint.textContent = resp.message;
    fillButton.disabled = true;
    return;
  }

  if (resp.kind !== "status") return;

  const { unlocked, hasVault } = resp.status;
  if (!hasVault) {
    setBadge("warn", "No vault");
    hint.textContent = "Create a vault in the Spooky-Pass app to get started.";
    fillButton.disabled = true;
    return;
  }
  if (!unlocked) {
    setBadge("warn", "Locked");
    hint.textContent = "Unlock your vault in the Spooky-Pass app to autofill.";
    fillButton.disabled = true;
    return;
  }

  setBadge("ok", "Unlocked");
  hint.textContent = "Click below to fill the matching login on this page.";
  fillButton.disabled = false;
}

async function fillActiveTab(): Promise<void> {
  const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
  if (!tab?.id) {
    hint.textContent = "No active tab.";
    return;
  }

  fillButton.disabled = true;
  hint.textContent = "Filling…";

  try {
    const result = await chrome.tabs.sendMessage<
      { kind: "popupFill" },
      { filled: boolean; reason?: string }
    >(tab.id, { kind: "popupFill" });

    if (result?.filled) {
      hint.textContent = "Filled. Review and submit yourself.";
      // Close the popup shortly after a successful fill.
      setTimeout(() => window.close(), 600);
      return;
    }
    hint.textContent = reasonText(result?.reason);
  } catch {
    hint.textContent = "No login form detected on this page.";
  } finally {
    fillButton.disabled = false;
  }
}

function reasonText(reason: string | undefined): string {
  switch (reason) {
    case "no-form":
      return "No login form detected on this page.";
    case "no-match":
      return "No saved login matches this site.";
    case "core-unavailable":
      return "Core is not running.";
    case "untrusted-frame":
      return "Cannot fill inside an embedded frame.";
    default:
      return "Nothing to fill here.";
  }
}

fillButton.addEventListener("click", () => void fillActiveTab());
void refreshStatus();
