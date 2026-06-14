// Unlock screen: enter the master password to unlock the vault. WrongPassword
// surfaces a generic inline error; the field autofocuses.

import { el, clear } from "../dom";
import { createActions } from "../actions";

export function renderUnlock(root: HTMLElement): void {
  clear(root);

  let password = "";
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

  const actions = createActions(showError);

  const pwInput = el("input", {
    type: "password",
    placeholder: "Master password",
    autofocus: true,
    onInput: (e) => {
      password = (e.target as HTMLInputElement).value;
      clearError();
    },
  });

  const submitBtn = el("button", {
    type: "submit",
    class: "primary block",
    textContent: "Unlock",
  });

  const submit = async () => {
    if (busy) return;
    clearError();
    if (password.length < 1) {
      showError("Enter your master password.");
      return;
    }
    busy = true;
    submitBtn.disabled = true;
    submitBtn.textContent = "Unlocking…";
    await actions.unlock(password);
    // Success re-renders into the vault; if we are still here, restore.
    busy = false;
    submitBtn.disabled = false;
    submitBtn.textContent = "Unlock";
    pwInput.focus();
    pwInput.select();
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
    el("h1", { class: "gate-title", textContent: "Spooky-Pass" }),
    el("p", {
      class: "gate-sub",
      textContent: "The crypt is locked. Enter your master password to open it.",
    }),
    errorBox,
    el(
      "div",
      { class: "field" },
      el("label", { textContent: "Master password" }),
      pwInput,
    ),
    submitBtn,
  );

  root.append(
    el("div", { class: "gate" }, el("div", { class: "gate-card" }, form)),
  );
}
