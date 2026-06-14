//! The `#[tauri::command]` surface.
//!
//! These are deliberately thin: each one borrows the managed [`AppState`] and
//! forwards to the corresponding app-core service method, returning
//! `Result<T, AppError>`. Tauri serializes a returned `AppError` as the JS
//! promise rejection value `{ kind, message? }` — the shape the frontend's
//! `types.ts` expects.
//!
//! ## Naming contract (FROZEN)
//!
//! The command names and parameter names here are mirrored by
//! `crates/app/ui/src/api.ts`. Tauri exposes each snake_case parameter to JS as
//! a camelCase key, so `master_password` ⇄ `masterPassword`, matching the
//! `invoke(...)` argument objects in `api.ts`. Do not rename without updating
//! the (frozen) TypeScript side.

use app_core::error::AppError;
use app_core::{
    AppState, EntryInput, EntryPatchInput, EntrySummary, EntryView, GeneratorOptionsDto, StatusDto,
};
use tauri::State;
use zeroize::Zeroizing;

/// Current high-level status (has-vault / unlocked / idle window). Infallible.
#[tauri::command]
pub fn status(state: State<'_, AppState>) -> StatusDto {
    state.status()
}

/// Create a brand-new vault and unlock the session.
#[tauri::command]
pub fn create_vault(
    state: State<'_, AppState>,
    master_password: String,
) -> Result<StatusDto, AppError> {
    // Hold the master password in a zeroizing buffer so it is wiped as soon as
    // app-core finishes deriving the key from it.
    let master_password = Zeroizing::new(master_password);
    state.create_vault(&master_password)
}

/// Unlock the on-disk vault.
#[tauri::command]
pub fn unlock(state: State<'_, AppState>, master_password: String) -> Result<StatusDto, AppError> {
    let master_password = Zeroizing::new(master_password);
    state.unlock(&master_password)
}

/// Lock the session, zeroizing the in-memory key.
#[tauri::command]
pub fn lock(state: State<'_, AppState>) -> Result<StatusDto, AppError> {
    state.lock()
}

/// List entries as password-free summaries.
#[tauri::command]
pub fn list_entries(state: State<'_, AppState>) -> Result<Vec<EntrySummary>, AppError> {
    state.list_entries()
}

/// Fetch a single entry (with its password) by id.
#[tauri::command]
pub fn get_entry(state: State<'_, AppState>, id: String) -> Result<EntryView, AppError> {
    state.get_entry(&id)
}

/// Add a new entry and return the stored view.
#[tauri::command]
pub fn add_entry(state: State<'_, AppState>, input: EntryInput) -> Result<EntryView, AppError> {
    state.add_entry(input)
}

/// Apply a partial update to an entry and return the updated view.
#[tauri::command]
pub fn update_entry(
    state: State<'_, AppState>,
    id: String,
    patch: EntryPatchInput,
) -> Result<EntryView, AppError> {
    state.update_entry(&id, patch)
}

/// Delete an entry by id.
#[tauri::command]
pub fn delete_entry(state: State<'_, AppState>, id: String) -> Result<(), AppError> {
    state.delete_entry(&id)
}

/// Generate a password from the supplied options.
#[tauri::command]
pub fn generate_password(
    state: State<'_, AppState>,
    options: GeneratorOptionsDto,
) -> Result<String, AppError> {
    state.generate_password(options)
}
