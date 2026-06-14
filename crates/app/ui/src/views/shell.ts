// Shared shell: the top bar shown on all authenticated screens (Lock, + Add,
// Generate, Settings) plus a content host the active view fills in.

import { el } from "../dom";
import type { Actions } from "../actions";
import type { AppState, Route } from "../store";

/**
 * Render the top bar + a `.shell-body` content container into `root`, and
 * return the body element for the active view to populate.
 */
export function renderShell(
  root: HTMLElement,
  state: AppState,
  actions: Actions,
): HTMLElement {
  const body = el("main", { class: "shell-body" });

  const navBtn = (label: string, route: Route, title?: string) =>
    el("button", {
      class: state.route === route ? "ghost active" : "ghost",
      textContent: label,
      ...(title ? { title } : {}),
      onClick: () => actions.go(route),
    });

  const topbar = el(
    "header",
    { class: "topbar" },
    el(
      "div",
      {
        class: "topbar-brand",
        title: "Back to vault",
        onClick: () => actions.go("list"),
      },
      "🦇",
      el("span", { textContent: "Spooky-Pass" }),
    ),
    navBtn("Vault", "list"),
    el("button", {
      class: "ghost",
      textContent: "+ Add",
      title: "Add a new entry",
      onClick: () => actions.newEntry(),
    }),
    navBtn("Generate", "generator", "Password generator"),
    navBtn("Settings", "settings"),
    el("button", {
      class: "danger",
      textContent: "Lock",
      title: "Lock the vault now",
      onClick: () => {
        void actions.lock();
      },
    }),
  );

  root.append(topbar, body);
  return body;
}
