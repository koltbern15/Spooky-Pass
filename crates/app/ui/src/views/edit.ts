// Add / edit entry form, bound to EntryInput (add) or EntryPatchInput (edit).
// Includes an inline "Generate" affordance and an embeddable generator widget
// to fill the password. Save -> addEntry / updateEntry. Delete (edit only) ->
// deleteEntry with a confirm.

import { el, clear } from "../dom";
import { createActions } from "../actions";
import type { Actions } from "../actions";
import type { AppState } from "../store";
import type { EntryInput, EntryView } from "../types";
import { renderGeneratorWidget } from "./generator";
import { renderShell } from "./shell";

interface FormModel {
  title: string;
  username: string;
  password: string;
  urls: string;
  notes: string;
}

function modelFromEntry(entry: EntryView | null): FormModel {
  return {
    title: entry?.title ?? "",
    username: entry?.username ?? "",
    password: entry?.password ?? "",
    urls: entry ? entry.urls.join("\n") : "",
    notes: entry?.notes ?? "",
  };
}

function parseUrls(raw: string): string[] {
  return raw
    .split(/[\n,]/)
    .map((s) => s.trim())
    .filter((s) => s.length > 0);
}

export function renderEdit(
  root: HTMLElement,
  state: AppState,
  actions: Actions,
): void {
  clear(root);
  const body = renderShell(root, state, actions);
  const panel = el("div", { class: "panel" });
  body.append(panel);

  const id = state.selectedId;
  const isEdit = id !== null;

  if (!isEdit) {
    renderForm(panel, null, actions);
    return;
  }

  panel.append(el("p", { class: "faint", textContent: "Loading…" }));
  const localActions = createActions();
  void (async () => {
    const entry = await localActions.getEntry(id);
    clear(panel);
    if (!entry) {
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
    renderForm(panel, entry, actions);
  })();
}

function renderForm(
  panel: HTMLElement,
  entry: EntryView | null,
  actions: Actions,
): void {
  const model = modelFromEntry(entry);
  const isEdit = entry !== null;
  let busy = false;

  const errorBox = el("div", { class: "error" });
  errorBox.style.display = "none";
  const showError = (msg: string) => {
    errorBox.textContent = msg;
    errorBox.style.display = "";
  };
  const clearError = () => {
    errorBox.style.display = "none";
  };

  // Locally-bound actions so save/delete errors surface in this form.
  const formActions = createActions(showError);

  const textField = (
    label: string,
    key: "title" | "username",
    placeholder: string,
    autofocus = false,
  ) =>
    el(
      "div",
      { class: "field" },
      el("label", { textContent: label }),
      el("input", {
        type: "text",
        value: model[key],
        placeholder,
        autofocus,
        onInput: (e) => {
          model[key] = (e.target as HTMLInputElement).value;
        },
      }),
    );

  // Password field with reveal + inline Generate.
  const pwInput = el("input", {
    type: "password",
    value: model.password,
    placeholder: "Password",
    onInput: (e) => {
      model.password = (e.target as HTMLInputElement).value;
    },
  });

  let pwRevealed = false;
  const revealBtn = el("button", {
    type: "button",
    class: "ghost",
    textContent: "Reveal",
    onClick: () => {
      pwRevealed = !pwRevealed;
      pwInput.type = pwRevealed ? "text" : "password";
      revealBtn.textContent = pwRevealed ? "Hide" : "Reveal";
    },
  });

  const generatorHost = el("div", {});
  let generatorOpen = false;
  const toggleGenBtn = el("button", {
    type: "button",
    class: "ghost",
    textContent: "Generate",
    title: "Open the password generator",
    onClick: () => {
      generatorOpen = !generatorOpen;
      clear(generatorHost);
      toggleGenBtn.textContent = generatorOpen ? "Hide generator" : "Generate";
      if (generatorOpen) {
        const widget = renderGeneratorWidget({
          initial: undefined,
          onGenerate: (pw) => {
            model.password = pw;
            pwInput.value = pw;
          },
        });
        generatorHost.append(
          el(
            "div",
            { class: "embedded" },
            el("div", { class: "embedded-head", textContent: "Generator" }),
            widget,
            el(
              "div",
              { class: "embedded-foot" },
              el("button", {
                type: "button",
                class: "primary",
                textContent: "Use this password",
                onClick: () => {
                  // The latest generated value is already mirrored into the
                  // input via onGenerate; just close the panel.
                  generatorOpen = false;
                  clear(generatorHost);
                  toggleGenBtn.textContent = "Generate";
                },
              }),
            ),
          ),
        );
      }
    },
  });

  const passwordField = el(
    "div",
    { class: "field" },
    el("label", { textContent: "Password" }),
    el("div", { class: "input-row" }, pwInput, revealBtn, toggleGenBtn),
    generatorHost,
  );

  const urlsField = el(
    "div",
    { class: "field" },
    el("label", { textContent: "URLs (one per line)" }),
    el("textarea", {
      value: model.urls,
      placeholder: "https://example.com",
      rows: 3,
      onInput: (e) => {
        model.urls = (e.target as HTMLTextAreaElement).value;
      },
    }),
    el("p", {
      class: "hint",
      textContent: "The first URL is used as the entry's primary URL.",
    }),
  );

  const notesField = el(
    "div",
    { class: "field" },
    el("label", { textContent: "Notes" }),
    el("textarea", {
      value: model.notes,
      placeholder: "Anything you want to remember…",
      rows: 3,
      onInput: (e) => {
        model.notes = (e.target as HTMLTextAreaElement).value;
      },
    }),
  );

  const saveBtn = el("button", {
    type: "submit",
    class: "success",
    textContent: isEdit ? "Save changes" : "Add entry",
  });

  const submit = async () => {
    if (busy) return;
    clearError();
    if (model.title.trim().length === 0) {
      showError("Please give this entry a title.");
      return;
    }
    busy = true;
    saveBtn.disabled = true;
    const prevLabel = saveBtn.textContent;
    saveBtn.textContent = "Saving…";

    const input: EntryInput = {
      title: model.title.trim(),
      username: model.username,
      password: model.password,
      urls: parseUrls(model.urls),
      notes: model.notes,
    };

    let result: EntryView | null;
    if (isEdit && entry) {
      result = await formActions.updateEntry(entry.id, input);
    } else {
      result = await formActions.addEntry(input);
    }

    if (result) {
      actions.selectEntry(result.id);
      return;
    }

    busy = false;
    saveBtn.disabled = false;
    saveBtn.textContent = prevLabel ?? (isEdit ? "Save changes" : "Add entry");
  };

  const cancelBtn = el("button", {
    type: "button",
    class: "ghost",
    textContent: "Cancel",
    onClick: () => {
      if (isEdit && entry) actions.selectEntry(entry.id);
      else actions.go("list");
    },
  });

  const headActions = el("div", { class: "panel-actions" }, cancelBtn);

  if (isEdit && entry) {
    headActions.append(
      el("button", {
        type: "button",
        class: "danger",
        textContent: "Delete",
        onClick: () => {
          void (async () => {
            if (
              !window.confirm(
                `Delete "${entry.title || "this entry"}"? This cannot be undone.`,
              )
            ) {
              return;
            }
            const ok = await formActions.deleteEntry(entry.id);
            if (ok) actions.go("list");
          })();
        },
      }),
    );
  }

  const form = el(
    "form",
    {
      onSubmit: (e) => {
        e.preventDefault();
        void submit();
      },
    },
    el(
      "div",
      { class: "panel-head" },
      el("h2", { textContent: isEdit ? "Edit entry" : "Add entry" }),
      headActions,
    ),
    errorBox,
    textField("Title", "title", "e.g. GitHub", true),
    textField("Username", "username", "you@example.com"),
    passwordField,
    urlsField,
    notesField,
    saveBtn,
  );

  panel.append(form);
}
