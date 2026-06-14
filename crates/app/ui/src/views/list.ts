// Vault list: shows all entries with a client-side search over title /
// username / primaryUrl. Selecting an entry routes to its detail view.

import { el, clear } from "../dom";
import type { Actions } from "../actions";
import type { AppState } from "../store";
import type { EntrySummary } from "../types";
import { renderShell } from "./shell";

// Search text persists across re-renders (e.g. when the entries list reloads).
let query = "";

function matches(entry: EntrySummary, q: string): boolean {
  if (!q) return true;
  const needle = q.toLowerCase();
  return (
    entry.title.toLowerCase().includes(needle) ||
    entry.username.toLowerCase().includes(needle) ||
    (entry.primaryUrl?.toLowerCase().includes(needle) ?? false)
  );
}

function avatarText(title: string): string {
  const t = title.trim();
  return t.length > 0 ? t[0] : "?";
}

export function renderList(
  root: HTMLElement,
  state: AppState,
  actions: Actions,
): void {
  clear(root);
  const body = renderShell(root, state, actions);

  const listHost = el("div", { class: "entries" });

  const renderEntries = () => {
    clear(listHost);
    const visible = state.entries.filter((e) => matches(e, query));

    if (state.entries.length === 0) {
      listHost.append(
        el(
          "div",
          { class: "empty" },
          el("span", { class: "empty-bat", textContent: "🦇" }),
          el("div", { textContent: "Your crypt is empty." }),
          el("p", {
            class: "faint",
            textContent: "Add your first credential to get started.",
          }),
          el("button", {
            class: "primary",
            textContent: "+ Add entry",
            onClick: () => actions.newEntry(),
          }),
        ),
      );
      return;
    }

    if (visible.length === 0) {
      listHost.append(
        el(
          "div",
          { class: "empty" },
          el("span", { class: "empty-bat", textContent: "🔍" }),
          el("div", { textContent: `No entries match "${query}".` }),
        ),
      );
      return;
    }

    for (const entry of visible) {
      listHost.append(
        el(
          "button",
          {
            class: "entry-card",
            onClick: () => actions.selectEntry(entry.id),
          },
          el("span", { class: "entry-avatar", textContent: avatarText(entry.title) }),
          el(
            "span",
            { class: "entry-main" },
            el("div", { class: "entry-title", textContent: entry.title || "(untitled)" }),
            el("div", {
              class: "entry-sub",
              textContent:
                entry.username || entry.primaryUrl || "no username",
            }),
          ),
          el("span", { class: "faint", textContent: "›" }),
        ),
      );
    }
  };

  const search = el("input", {
    type: "search",
    class: "list-search",
    placeholder: "Search title, username, or URL…",
    value: query,
    onInput: (e) => {
      query = (e.target as HTMLInputElement).value;
      renderEntries();
    },
  });

  body.append(
    el(
      "div",
      { class: "list-header" },
      el("h2", { textContent: "Vault" }),
      el("span", {
        class: "faint",
        textContent: `${state.entries.length} ${state.entries.length === 1 ? "entry" : "entries"}`,
      }),
      search,
    ),
    listHost,
  );

  renderEntries();
}
