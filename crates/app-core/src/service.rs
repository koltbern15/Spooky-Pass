//! The application service layer: the operations the Tauri commands wrap.
//!
//! Every method takes `&self` and grabs the session `Mutex` exactly once for the
//! duration of the call (the lock is *never* held across an await — these methods
//! are synchronous). Reads and mutations both refresh the activity timestamp so
//! interacting with the app defers idle auto-lock; mutations additionally persist
//! the vault to disk immediately via [`UnlockedVault::save_to_path`] (an atomic
//! temp-file + rename), so a crash can never corrupt the vault or lose an
//! already-persisted change. If a save itself fails, the error is returned to the
//! caller; the in-memory vault keeps the change while disk does not, and the next
//! successful save reconciles the two.
//!
//! Errors are mapped to [`AppError`] for the UI. In particular `unlock` collapses
//! *every* failure — missing file aside — to [`AppError::WrongPassword`], so a
//! wrong password and a tampered vault stay indistinguishable.

use std::sync::MutexGuard;

use vault_core::{EntryPatch, KdfParams, LockedVault, NewEntry, PasswordOptions, UnlockedVault};

use crate::dto::{
    EntryInput, EntryPatchInput, EntrySummary, EntryView, GeneratorOptionsDto, StatusDto,
};
use crate::error::{AppError, Result};
use crate::state::{AppState, Session, SessionState};

impl AppState {
    /// Create a brand-new vault from `master_password` and persist it, leaving
    /// the session **unlocked**.
    ///
    /// Refuses with [`AppError::VaultAlreadyExists`] if a file is already present
    /// at the vault path — we never overwrite an existing vault on create. Uses
    /// [`KdfParams::default`].
    pub fn create_vault(&self, master_password: &str) -> Result<StatusDto> {
        let mut session = self.lock_session();

        if session.vault_path.exists() {
            return Err(AppError::VaultAlreadyExists);
        }

        let vault = LockedVault::create(master_password, KdfParams::default())?;
        vault.save_to_path(&session.vault_path)?;

        let now = session.clock.now();
        session.state = SessionState::Unlocked {
            vault,
            last_activity: now,
        };

        Ok(Self::status_of(&session))
    }

    /// Unlock the on-disk vault with `master_password`.
    ///
    /// Returns [`AppError::NoVault`] if no vault file exists. Any other failure —
    /// a read error, a malformed file, or a failed AEAD-open — collapses to
    /// [`AppError::WrongPassword`] so we never reveal whether the password was
    /// wrong or the vault was tampered with.
    pub fn unlock(&self, master_password: &str) -> Result<StatusDto> {
        let mut session = self.lock_session();

        if !session.vault_path.exists() {
            return Err(AppError::NoVault);
        }

        // Map *every* open/unlock failure to WrongPassword: a corrupt or tampered
        // vault must be indistinguishable from a bad password.
        let locked = LockedVault::open_from_path(&session.vault_path)
            .map_err(|_| AppError::WrongPassword)?;
        let vault =
            UnlockedVault::unlock(&locked, master_password).map_err(|_| AppError::WrongPassword)?;

        let now = session.clock.now();
        session.state = SessionState::Unlocked {
            vault,
            last_activity: now,
        };

        Ok(Self::status_of(&session))
    }

    /// Lock the session, zeroizing the in-memory key. Idempotent: locking an
    /// already-locked session simply reports the locked status.
    pub fn lock(&self) -> Result<StatusDto> {
        let mut session = self.lock_session();
        session.do_lock();
        Ok(Self::status_of(&session))
    }

    /// The current high-level status. Infallible — usable on the create/unlock
    /// screens before any vault is open.
    pub fn status(&self) -> StatusDto {
        let session = self.lock_session();
        Self::status_of(&session)
    }

    /// List every entry as a password-free [`EntrySummary`].
    ///
    /// Bumps activity. Returns [`AppError::Locked`] if the vault is locked.
    pub fn list_entries(&self) -> Result<Vec<EntrySummary>> {
        let mut session = self.lock_session();
        let vault = Self::unlocked_mut(&mut session)?;
        let summaries = vault.list_entries().iter().map(entry_summary).collect();
        Self::bump_activity(&mut session);
        Ok(summaries)
    }

    /// Fetch a single entry (including its password) as an [`EntryView`].
    ///
    /// Bumps activity. Returns [`AppError::Locked`] if locked, or
    /// [`AppError::EntryNotFound`] if no entry has that id.
    pub fn get_entry(&self, id: &str) -> Result<EntryView> {
        let mut session = self.lock_session();
        let vault = Self::unlocked_mut(&mut session)?;
        let view = vault
            .get_entry(id)
            .map(entry_view)
            .ok_or_else(|| AppError::EntryNotFound(id.to_string()))?;
        Self::bump_activity(&mut session);
        Ok(view)
    }

    /// Add a new entry, stamping `created == updated` with the clock's wall time,
    /// persist the vault, and return the stored entry as an [`EntryView`].
    pub fn add_entry(&self, input: EntryInput) -> Result<EntryView> {
        let mut session = self.lock_session();
        let now = session.clock.now_rfc3339();

        let view = {
            let vault = Self::unlocked_mut(&mut session)?;
            let entry = vault.add_entry(NewEntry {
                title: input.title,
                username: input.username,
                password: input.password,
                urls: input.urls,
                notes: input.notes,
                now,
            })?;
            entry_view(entry)
        };

        Self::persist(&mut session)?;
        Self::bump_activity(&mut session);
        Ok(view)
    }

    /// Apply a partial update to the entry with `id`, refreshing only its
    /// `updated` timestamp, persist the vault, and return the updated
    /// [`EntryView`].
    pub fn update_entry(&self, id: &str, patch: EntryPatchInput) -> Result<EntryView> {
        let mut session = self.lock_session();
        let now = session.clock.now_rfc3339();

        let view = {
            let vault = Self::unlocked_mut(&mut session)?;
            let entry = vault.update_entry(
                id,
                EntryPatch {
                    title: patch.title,
                    username: patch.username,
                    password: patch.password,
                    urls: patch.urls,
                    notes: patch.notes,
                    now,
                },
            )?;
            entry_view(entry)
        };

        Self::persist(&mut session)?;
        Self::bump_activity(&mut session);
        Ok(view)
    }

    /// Delete the entry with `id` and persist the vault.
    ///
    /// Returns [`AppError::Locked`] if locked, or [`AppError::EntryNotFound`] if
    /// no entry has that id.
    pub fn delete_entry(&self, id: &str) -> Result<()> {
        let mut session = self.lock_session();
        {
            let vault = Self::unlocked_mut(&mut session)?;
            vault.delete_entry(id)?;
        }
        Self::persist(&mut session)?;
        Self::bump_activity(&mut session);
        Ok(())
    }

    /// Generate a password from the supplied options.
    ///
    /// Does not require an unlocked vault (the generator screen is reachable
    /// before unlock), but does bump activity when a vault is open. Maps an empty
    /// charset / zero length to [`AppError::InvalidPasswordOptions`].
    pub fn generate_password(&self, opts: GeneratorOptionsDto) -> Result<String> {
        let mut session = self.lock_session();
        let options: PasswordOptions = opts.into();
        let password = vault_core::generate_password(&options)?;
        Self::bump_activity(&mut session);
        Ok(password)
    }

    /// Persist the currently-unlocked vault to disk. Returns
    /// [`AppError::Locked`] if no vault is open.
    pub fn save(&self) -> Result<()> {
        let mut session = self.lock_session();
        Self::persist(&mut session)
    }

    // ---- internal helpers ----------------------------------------------------

    /// Acquire the session mutex, recovering from a poisoned lock rather than
    /// propagating the panic (we never leave the session in a broken invariant
    /// inside the guard).
    ///
    /// `pub(crate)` so the auto-lock module shares this single, poison-recovering
    /// acquisition: if any thread ever poisoned the mutex, an `.expect()` there
    /// would kill the auto-lock driver and leave the vault unlocked forever.
    pub(crate) fn lock_session(&self) -> MutexGuard<'_, Session> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Build a [`StatusDto`] from a held session guard.
    fn status_of(session: &Session) -> StatusDto {
        StatusDto {
            has_vault: session.vault_path.exists(),
            unlocked: matches!(session.state, SessionState::Unlocked { .. }),
            idle_timeout_secs: session.idle_timeout.as_secs(),
        }
    }

    /// Borrow the unlocked vault mutably, or return [`AppError::Locked`].
    fn unlocked_mut(session: &mut Session) -> Result<&mut UnlockedVault> {
        match &mut session.state {
            SessionState::Unlocked { vault, .. } => Ok(vault),
            SessionState::Locked => Err(AppError::Locked),
        }
    }

    /// Refresh `last_activity` to "now" (no-op if locked). Called inline while the
    /// session lock is already held, so it does not re-enter the mutex.
    fn bump_activity(session: &mut Session) {
        let now = session.clock.now();
        if let SessionState::Unlocked { last_activity, .. } = &mut session.state {
            *last_activity = now;
        }
    }

    /// Write the unlocked vault to its path, mapping failures to [`AppError`].
    /// Returns [`AppError::Locked`] if no vault is open.
    fn persist(session: &mut Session) -> Result<()> {
        let path = session.vault_path.clone();
        let vault = Self::unlocked_mut(session)?;
        vault.save_to_path(&path)?;
        Ok(())
    }
}

/// Project a stored [`vault_core::Entry`] into a password-free [`EntrySummary`].
fn entry_summary(entry: &vault_core::Entry) -> EntrySummary {
    EntrySummary {
        id: entry.id.clone(),
        title: entry.title.clone(),
        username: entry.username.clone(),
        primary_url: entry.urls.first().cloned(),
        updated: entry.updated.clone(),
    }
}

/// Project a stored [`vault_core::Entry`] into a full [`EntryView`].
fn entry_view(entry: &vault_core::Entry) -> EntryView {
    EntryView {
        id: entry.id.clone(),
        title: entry.title.clone(),
        username: entry.username.clone(),
        password: entry.password.clone(),
        urls: entry.urls.clone(),
        notes: entry.notes.clone(),
        created: entry.created.clone(),
        updated: entry.updated.clone(),
    }
}
