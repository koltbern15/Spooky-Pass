// Password generator. Exposes:
//   - renderGenerator(root, state, actions): standalone full-page view.
//   - renderGeneratorWidget(opts): reusable controls block, embeddable in edit.
//
// Controls: length slider, character-class checkboxes, exclude-ambiguous toggle.
// Re-generates live via api.generatePassword (through actions.generatePassword).

import { el, clear } from "../dom";
import { createActions } from "../actions";
import type { Actions } from "../actions";
import type { AppState } from "../store";
import type { GeneratorOptions } from "../types";
import { copyToClipboard } from "../platform";
import { renderShell } from "./shell";

export const DEFAULT_OPTIONS: GeneratorOptions = {
  length: 20,
  lowercase: true,
  uppercase: true,
  digits: true,
  symbols: true,
  excludeAmbiguous: false,
};

interface WidgetOptions {
  /** Initial generator options (defaults applied if omitted). */
  initial?: GeneratorOptions;
  /** Called with each freshly generated password. */
  onGenerate?: (password: string) => void;
  /** Show a "Copy" button next to the output (true for standalone). */
  showCopy?: boolean;
}

/** Build the generator controls + live output. Returns the root element. */
export function renderGeneratorWidget(opts: WidgetOptions = {}): HTMLElement {
  const options: GeneratorOptions = { ...DEFAULT_OPTIONS, ...opts.initial };

  let current = "";

  const errorBox = el("div", { class: "error" });
  errorBox.style.display = "none";
  const showLocalError = (msg: string) => {
    errorBox.textContent = msg;
    errorBox.style.display = "";
  };
  const clearLocalError = () => {
    errorBox.style.display = "none";
  };

  // Wrap the passed actions so generator errors land in this widget.
  const actions = createActions(showLocalError);

  const pwEl = el("span", { class: "pw", textContent: "…" });
  const copiedFlash = el("span", { class: "copied-flash" });

  const regenerate = async () => {
    clearLocalError();
    const pw = await actions.generatePassword(options);
    if (pw === null) return;
    current = pw;
    pwEl.textContent = pw;
    opts.onGenerate?.(pw);
  };

  const lengthValue = el("span", { textContent: String(options.length) });
  const lengthSlider = el("input", {
    type: "range",
    min: "8",
    max: "64",
    step: "1",
    value: String(options.length),
    onInput: (e) => {
      options.length = Number((e.target as HTMLInputElement).value);
      lengthValue.textContent = String(options.length);
      void regenerate();
    },
  });

  const classToggle = (
    key: "lowercase" | "uppercase" | "digits" | "symbols" | "excludeAmbiguous",
    label: string,
  ) =>
    el(
      "label",
      { class: "checkbox" },
      el("input", {
        type: "checkbox",
        checked: options[key],
        onChange: (e) => {
          options[key] = (e.target as HTMLInputElement).checked;
          void regenerate();
        },
      }),
      label,
    );

  const output = el(
    "div",
    { class: "gen-output" },
    pwEl,
    el("button", {
      class: "ghost",
      textContent: "↻",
      title: "Regenerate",
      onClick: () => {
        void regenerate();
      },
    }),
    opts.showCopy
      ? el("button", {
          class: "success",
          textContent: "Copy",
          onClick: () => {
            void (async () => {
              if (current && (await copyToClipboard(current))) {
                copiedFlash.textContent = "Copied!";
                window.setTimeout(() => {
                  copiedFlash.textContent = "";
                }, 1500);
              }
            })();
          },
        })
      : null,
    opts.showCopy ? copiedFlash : null,
  );

  const controls = el(
    "div",
    { class: "gen-controls" },
    el(
      "div",
      {},
      el(
        "div",
        { class: "gen-length-head" },
        el("span", { textContent: "Length" }),
        lengthValue,
      ),
      lengthSlider,
    ),
    el(
      "div",
      { class: "gen-classes" },
      classToggle("lowercase", "Lowercase (a-z)"),
      classToggle("uppercase", "Uppercase (A-Z)"),
      classToggle("digits", "Digits (0-9)"),
      classToggle("symbols", "Symbols (!@#…)"),
    ),
    classToggle("excludeAmbiguous", "Exclude ambiguous (O0l1…)"),
  );

  const widget = el("div", {}, errorBox, output, controls);

  // Generate one immediately. void keeps the function synchronous.
  void regenerate();

  return widget;
}

/** Standalone full-page generator with the app shell + top bar. */
export function renderGenerator(
  root: HTMLElement,
  state: AppState,
  actions: Actions,
): void {
  clear(root);
  const body = renderShell(root, state, actions);

  const widget = renderGeneratorWidget({ showCopy: true });

  body.append(
    el(
      "div",
      { class: "panel" },
      el(
        "div",
        { class: "panel-head" },
        el("h2", { textContent: "Password generator" }),
      ),
      widget,
      el("p", {
        class: "hint",
        textContent:
          "Tip: use this from the Add/Edit screen to fill an entry's password directly.",
      }),
    ),
  );
}
