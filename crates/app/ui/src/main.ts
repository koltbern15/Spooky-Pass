// Entrypoint: bootstraps the app, picks the initial screen from the backend
// status, wires the store to a render loop, and listens for backend
// `vault-locked` events to route back to the unlock screen.

import { listen } from "@tauri-apps/api/event";

import { store } from "./store";
import type { AppState } from "./store";
import { createActions } from "./actions";
import type { Actions } from "./actions";

import { renderCreate } from "./views/create";
import { renderUnlock } from "./views/unlock";
import { renderList } from "./views/list";
import { renderDetail } from "./views/detail";
import { renderEdit } from "./views/edit";
import { renderGenerator } from "./views/generator";
import { renderSettings } from "./views/settings";

const root = document.getElementById("app");
if (!root) throw new Error("#app mount point not found");
const app = root;

// A toast-ish global error sink for navigational/non-form actions (e.g. Lock).
// Form screens bind their own createActions(showError); this default just logs.
function reportGlobalError(message: string): void {
  console.error("[spooky-pass]", message);
}

const actions: Actions = createActions(reportGlobalError);

/** Dispatch the active route to its view renderer. */
function render(state: AppState): void {
  switch (state.route) {
    case "create":
      renderCreate(app);
      break;
    case "unlock":
      renderUnlock(app);
      break;
    case "list":
      renderList(app, state, actions);
      break;
    case "detail":
      renderDetail(app, state, actions);
      break;
    case "edit":
      renderEdit(app, state, actions);
      break;
    case "generator":
      renderGenerator(app, state, actions);
      break;
    case "settings":
      renderSettings(app, state, actions);
      break;
  }
}

/** Choose the initial screen from the backend status. */
function routeForStatus(): "create" | "unlock" | "list" {
  const status = store.get().status;
  if (!status || !status.hasVault) return "create";
  if (!status.unlocked) return "unlock";
  return "list";
}

async function bootstrap(): Promise<void> {
  // Re-render on every state change.
  store.subscribe(render);

  // Listen for the backend auto-lock / lock event and route to unlock.
  try {
    await listen("vault-locked", () => {
      store.set({ route: "unlock", selectedId: null, entries: [] });
    });
  } catch (err) {
    reportGlobalError(`Failed to subscribe to vault-locked: ${String(err)}`);
  }

  // Pull initial status and route accordingly. `store.set` triggers render.
  const status = await actions.refreshStatus();
  const route = routeForStatus();
  store.set({ route });

  if (route === "list" && status?.unlocked) {
    await actions.loadEntries();
  }
}

void bootstrap();
