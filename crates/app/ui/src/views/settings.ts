// Settings: shows the auto-lock idle timeout (read-only, from Status) and a
// "Lock now" button that locks the vault immediately (api.lock).

import { el, clear } from "../dom";
import type { Actions } from "../actions";
import type { AppState } from "../store";
import { renderShell } from "./shell";

function formatTimeout(secs: number): string {
  if (!Number.isFinite(secs) || secs <= 0) return "Never (locks only on quit)";
  if (secs % 3600 === 0) {
    const h = secs / 3600;
    return `${h} hour${h === 1 ? "" : "s"}`;
  }
  if (secs % 60 === 0) {
    const m = secs / 60;
    return `${m} minute${m === 1 ? "" : "s"}`;
  }
  return `${secs} seconds`;
}

export function renderSettings(
  root: HTMLElement,
  state: AppState,
  actions: Actions,
): void {
  clear(root);
  const body = renderShell(root, state, actions);

  const idleTimeout = state.status?.idleTimeoutSecs ?? 0;

  const presets: Array<{ label: string; secs: number }> = [
    { label: "1 minute", secs: 60 },
    { label: "5 minutes", secs: 300 },
    { label: "15 minutes", secs: 900 },
    { label: "30 minutes", secs: 1800 },
    { label: "1 hour", secs: 3600 },
    { label: "Never (lock on quit)", secs: 0 },
  ];
  // Keep the current value selectable even if it isn't one of the presets.
  if (!presets.some((p) => p.secs === idleTimeout)) {
    presets.push({ label: formatTimeout(idleTimeout), secs: idleTimeout });
  }

  const timeoutSelect = el(
    "select",
    {
      class: "select",
      onChange: (e) => {
        const secs = Number((e.target as HTMLSelectElement).value);
        void actions.setIdleTimeout(secs);
      },
    },
    ...presets.map((p) => {
      const opt = el("option", { textContent: p.label });
      opt.value = String(p.secs);
      return opt;
    }),
  );
  // Pre-select the current value (setting .value picks the matching <option>).
  timeoutSelect.value = String(idleTimeout);

  body.append(
    el(
      "div",
      { class: "panel" },
      el(
        "div",
        { class: "panel-head" },
        el("h2", { textContent: "Settings" }),
      ),
      el(
        "div",
        { class: "settings-row" },
        el(
          "div",
          {},
          el("div", { class: "label", textContent: "Auto-lock timeout" }),
          el("div", {
            class: "desc",
            textContent: "How long the vault stays unlocked while idle.",
          }),
        ),
        timeoutSelect,
      ),
      el(
        "div",
        { class: "settings-row" },
        el(
          "div",
          {},
          el("div", { class: "label", textContent: "Lock now" }),
          el("div", {
            class: "desc",
            textContent: "Zeroize the key and return to the unlock screen.",
          }),
        ),
        el("button", {
          class: "danger",
          textContent: "Lock vault",
          onClick: () => {
            void actions.lock();
          },
        }),
      ),
      el("p", {
        class: "hint",
        textContent: "🦇 Spooky-Pass keeps your secrets local and encrypted at rest.",
      }),
    ),
  );
}
