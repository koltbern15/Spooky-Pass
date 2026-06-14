// First-run screen: set the master password (with confirmation) and create the
// vault via api.createVault (through a locally-bound actions instance so errors
// land in this screen's inline error box).

import { el, clear } from "../dom";
import { createActions } from "../actions";

const MIN_MASTER_PASSWORD = 8;

interface Strength {
  label: string;
  color: string;
  pct: number;
}

/** A lightweight, dependency-free master-password strength hint. */
function passwordStrength(pw: string): Strength {
  if (pw.length === 0) return { label: "", color: "transparent", pct: 0 };
  if (pw.length < MIN_MASTER_PASSWORD) {
    return { label: "Too short", color: "#ff6b6b", pct: 15 };
  }
  let score = 0;
  if (pw.length >= 12) score++;
  if (pw.length >= 16) score++;
  if (/[a-z]/.test(pw) && /[A-Z]/.test(pw)) score++;
  if (/\d/.test(pw)) score++;
  if (/[^A-Za-z0-9]/.test(pw)) score++;
  if (score <= 1) return { label: "Weak", color: "#ff8a5b", pct: 35 };
  if (score <= 3) return { label: "Fair", color: "#ffd27f", pct: 68 };
  return { label: "Strong", color: "#7cfc9b", pct: 100 };
}

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

  // Live strength meter under the password field.
  const strengthFill = el("div", {});
  Object.assign(strengthFill.style, {
    height: "4px",
    width: "0%",
    borderRadius: "2px",
    background: "transparent",
    transition: "width 0.15s, background 0.15s",
  } satisfies Partial<CSSStyleDeclaration>);
  const strengthBar = el("div", {}, strengthFill);
  Object.assign(strengthBar.style, {
    background: "rgba(255,255,255,0.08)",
    borderRadius: "2px",
    marginTop: "6px",
  } satisfies Partial<CSSStyleDeclaration>);
  const strengthLabel = el("div", {});
  Object.assign(strengthLabel.style, {
    fontSize: "11px",
    marginTop: "3px",
    minHeight: "14px",
  } satisfies Partial<CSSStyleDeclaration>);

  const updateStrength = () => {
    const s = passwordStrength(password);
    strengthFill.style.width = `${s.pct}%`;
    strengthFill.style.background = s.color;
    strengthLabel.textContent = s.label;
    strengthLabel.style.color = s.color;
  };

  const pwInput = el("input", {
    type: "password",
    placeholder: `At least ${MIN_MASTER_PASSWORD} characters`,
    autofocus: true,
    onInput: (e) => {
      password = (e.target as HTMLInputElement).value;
      clearError();
      updateStrength();
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
    if (password.length < MIN_MASTER_PASSWORD) {
      showError(
        `Use at least ${MIN_MASTER_PASSWORD} characters for your master password.`,
      );
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
      strengthBar,
      strengthLabel,
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
