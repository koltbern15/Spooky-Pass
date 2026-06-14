// First-run screen: set the master password (with confirmation) and create the
// vault via api.createVault (through a locally-bound actions instance so errors
// land in this screen's inline error box).

import { el, clear } from "../dom";
import { createActions } from "../actions";

export function renderCreate(root: HTMLElement): void {
  clear(root);

  let password = "";
  let confirm = "";
  let busy = false;

  const errorBox = el("div", { class: "error" });
  errorBox.style.display = "none";

  const showError = (msg: string) => {
    errorBox.textContent = msg;
    errorBox.style.display = "";
  };
  const clearError = () => {
    errorBox.textContent = "";
    errorBox.style.display = "none";
  };

  // Errors from createVault are surfaced inline here.
  const actions = createActions(showError);

  const pwInput = el("input", {
    type: "password",
    placeholder: "Choose a master password",
    autofocus: true,
    onInput: (e) => {
      password = (e.target as HTMLInputElement).value;
      clearError();
    },
  });

  const confirmInput = el("input", {
    type: "password",
    placeholder: "Confirm master password",
    onInput: (e) => {
      confirm = (e.target as HTMLInputElement).value;
      clearError();
    },
  });

  const submitBtn = el("button", {
    type: "submit",
    class: "primary block",
    textContent: "Create vault",
  });

  const submit = async () => {
    if (busy) return;
    clearError();
    if (password.length < 1) {
      showError("Please choose a master password.");
      return;
    }
    if (password !== confirm) {
      showError("Passwords do not match.");
      return;
    }
    busy = true;
    submitBtn.disabled = true;
    submitBtn.textContent = "Creating…";
    await actions.createVault(password);
    // On success the store route changes and the app re-renders, replacing this
    // DOM. If we are still here, an error was shown; restore the button.
    busy = false;
    submitBtn.disabled = false;
    submitBtn.textContent = "Create vault";
  };

  const form = el(
    "form",
    {
      onSubmit: (e) => {
        e.preventDefault();
        void submit();
      },
    },
    el("span", { class: "gate-bat", textContent: "🦇" }),
    el("h1", { class: "gate-title", textContent: "Welcome to Spooky-Pass" }),
    el("p", {
      class: "gate-sub",
      textContent:
        "Create a vault to guard your secrets. Your master password is never stored — there is no recovery if you forget it.",
    }),
    errorBox,
    el(
      "div",
      { class: "field" },
      el("label", { textContent: "Master password" }),
      pwInput,
    ),
    el(
      "div",
      { class: "field" },
      el("label", { textContent: "Confirm password" }),
      confirmInput,
    ),
    submitBtn,
  );

  root.append(
    el("div", { class: "gate" }, el("div", { class: "gate-card" }, form)),
  );
}
