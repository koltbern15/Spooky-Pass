// Entry detail: loads the full entry (api.getEntry), reveals/copies the
// password, copies the username, opens URLs, and offers Edit / Delete.

import { el, clear, append } from "../dom";
import { createActions } from "../actions";
import type { Actions } from "../actions";
import type { AppState } from "../store";
import type { EntryView } from "../types";
import { copyToClipboard, openUrl } from "../platform";
import { renderShell } from "./shell";

function flashCopy(btn: HTMLButtonElement, label: string): void {
  const prev = btn.textContent;
  btn.textContent = "Copied!";
  btn.disabled = true;
  window.setTimeout(() => {
    btn.textContent = prev ?? label;
    btn.disabled = false;
  }, 1200);
}

function copyButton(label: string, getText: () => string): HTMLButtonElement {
  const btn = el("button", {
    class: "ghost",
    textContent: label,
    onClick: () => {
      void (async () => {
        if (await copyToClipboard(getText())) flashCopy(btn, label);
      })();
    },
  });
  return btn;
}

export function renderDetail(
  root: HTMLElement,
  state: AppState,
  actions: Actions,
): void {
  clear(root);
  const body = renderShell(root, state, actions);

  const panel = el("div", { class: "panel" });
  body.append(panel);

  const id = state.selectedId;
  if (!id) {
    actions.go("list");
    return;
  }

  panel.append(el("p", { class: "faint", textContent: "Loading…" }));

  const localActions = createActions();

  void (async () => {
    const entry = await localActions.getEntry(id);
    clear(panel);
    if (!entry) {
      // getEntry already reported; if it was EntryNotFound, go back to list.
      panel.append(
        el("p", { class: "faint", textContent: "Could not load this entry." }),
        el("button", {
          class: "ghost",
          textContent: "← Back to vault",
          onClick: () => actions.go("list"),
        }),
      );
      return;
    }
    renderEntry(panel, entry, actions);
  })();
}

function renderEntry(
  panel: HTMLElement,
  entry: EntryView,
  actions: Actions,
): void {
  let revealed = false;

  const pwText = el("span", { class: "val mono", textContent: "••••••••••" });
  const revealBtn = el("button", {
    class: "ghost",
    textContent: "Reveal",
    title: "Show / hide password",
    onClick: () => {
      revealed = !revealed;
      pwText.textContent = revealed ? entry.password : "••••••••••";
      revealBtn.textContent = revealed ? "Hide" : "Reveal";
    },
  });

  const head = el(
    "div",
    { class: "panel-head" },
    el("h2", { textContent: entry.title || "(untitled)" }),
    el(
      "div",
      { class: "panel-actions" },
      el("button", {
        class: "ghost",
        textContent: "← Back",
        onClick: () => actions.go("list"),
      }),
      el("button", {
        class: "primary",
        textContent: "Edit",
        onClick: () => actions.editSelected(),
      }),
    ),
  );

  // Username row.
  const usernameRow = el(
    "div",
    { class: "detail-row" },
    el("div", { class: "detail-label", textContent: "Username" }),
    el(
      "div",
      { class: "detail-value" },
      el("span", {
        class: "val",
        textContent: entry.username || "—",
      }),
      entry.username ? copyButton("Copy", () => entry.username) : null,
    ),
  );

  // Password row.
  const passwordRow = el(
    "div",
    { class: "detail-row" },
    el("div", { class: "detail-label", textContent: "Password" }),
    el(
      "div",
      { class: "detail-value" },
      pwText,
      revealBtn,
      copyButton("Copy", () => entry.password),
    ),
  );

  // URLs.
  const urlRows =
    entry.urls.length > 0
      ? el(
          "div",
          { class: "detail-row" },
          el("div", { class: "detail-label", textContent: "URLs" }),
          ...entry.urls.map((url) =>
            el(
              "div",
              { class: "detail-value" },
              el("a", {
                class: "val url-link",
                href: "#",
                textContent: url,
                onClick: (e) => {
                  e.preventDefault();
                  openUrl(url);
                },
              }),
              copyButton("Copy", () => url),
            ),
          ),
        )
      : null;

  // Notes.
  const notesRow = entry.notes.trim()
    ? el(
        "div",
        { class: "detail-row" },
        el("div", { class: "detail-label", textContent: "Notes" }),
        el("div", { class: "notes-box", textContent: entry.notes }),
      )
    : null;

  const meta = el(
    "div",
    { class: "detail-row" },
    el("div", {
      class: "faint",
      textContent: `Updated ${formatDate(entry.updated)} · Created ${formatDate(entry.created)}`,
    }),
  );

  const deleteBtn = el("button", {
    class: "danger",
    textContent: "Delete entry",
    onClick: () => {
      void (async () => {
        if (!window.confirm(`Delete "${entry.title || "this entry"}"? This cannot be undone.`)) {
          return;
        }
        const ok = await actions.deleteEntry(entry.id);
        if (ok) actions.go("list");
      })();
    },
  });

  append(
    panel,
    head,
    usernameRow,
    passwordRow,
    urlRows,
    notesRow,
    meta,
    el("div", { class: "detail-row" }, deleteBtn),
  );
}

function formatDate(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toLocaleString();
}
