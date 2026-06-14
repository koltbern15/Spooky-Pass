//! Integration tests for `spooky-pass-app-core`.
//!
//! These exercise the full session/service/auto-lock surface against a real
//! vault file in a `tempfile::TempDir`, driving time with a [`FakeClock`] so the
//! suite never sleeps and never touches the real per-OS data directory.
//!
//! `create_vault` / `unlock` perform genuine Argon2id derivations with the
//! crate's default (64 MiB) parameters, so a handful of unlock cycles is the
//! dominant cost here; everything else is in-memory and instant.

use std::time::Duration;

use app_core::dto::GeneratorOptionsDto;
use app_core::error::AppError;
use app_core::state::AppState;
use app_core::testing::{FakeClock, DEFAULT_WALL};
use app_core::{EntryInput, EntryPatchInput};

use assert_matches::assert_matches;
use tempfile::TempDir;
use vault_core::{LockedVault, PasswordOptions, UnlockedVault};

const PW: &str = "correct horse battery staple";
const TIMEOUT: Duration = Duration::from_secs(15 * 60);

/// A fresh, locked `AppState` over a tempdir vault path, plus the clock and the
/// owning `TempDir` (returned so it outlives the test).
struct Harness {
    state: AppState,
    clock: FakeClock,
    _dir: TempDir,
    vault_path: std::path::PathBuf,
}

fn harness() -> Harness {
    harness_with_timeout(TIMEOUT)
}

fn harness_with_timeout(timeout: Duration) -> Harness {
    let dir = TempDir::new().expect("create tempdir");
    let vault_path = dir.path().join("vault.spk");
    let clock = FakeClock::new();
    let state = AppState::new(vault_path.clone(), timeout, Box::new(clock.clone()));
    Harness {
        state,
        clock,
        _dir: dir,
        vault_path,
    }
}

fn sample_input(title: &str) -> EntryInput {
    EntryInput {
        title: title.into(),
        username: format!("{title}-user"),
        password: format!("{title}-pass"),
        urls: vec![format!("https://{title}.example")],
        notes: format!("notes for {title}"),
    }
}

/// Open + unlock the vault file directly (bypassing the service) to assert what
/// was persisted to disk.
fn reopen(path: &std::path::Path) -> UnlockedVault {
    let locked = LockedVault::open_from_path(path).expect("reopen vault file");
    UnlockedVault::unlock(&locked, PW).expect("unlock reopened vault")
}

// ---- State transitions -------------------------------------------------------

#[test]
fn create_leaves_session_unlocked() {
    let h = harness();
    let status = h.state.create_vault(PW).expect("create vault");
    assert!(status.has_vault);
    assert!(status.unlocked);
    assert_eq!(status.idle_timeout_secs, TIMEOUT.as_secs());
    assert!(h.vault_path.exists(), "vault file written on create");
}

#[test]
fn create_when_file_exists_is_rejected() {
    let h = harness();
    h.state.create_vault(PW).expect("first create");
    let err = h
        .state
        .create_vault(PW)
        .expect_err("second create must fail");
    assert_matches!(err, AppError::VaultAlreadyExists);
}

#[test]
fn lock_then_status_reports_locked() {
    let h = harness();
    h.state.create_vault(PW).unwrap();
    let status = h.state.lock().expect("lock");
    assert!(status.has_vault);
    assert!(!status.unlocked);
}

#[test]
fn lock_is_idempotent() {
    let h = harness();
    h.state.create_vault(PW).unwrap();
    h.state.lock().unwrap();
    // Locking again must not error and must stay locked.
    let status = h.state.lock().expect("second lock");
    assert!(!status.unlocked);
}

#[test]
fn unlock_with_correct_password_succeeds() {
    let h = harness();
    h.state.create_vault(PW).unwrap();
    h.state.lock().unwrap();
    let status = h.state.unlock(PW).expect("unlock");
    assert!(status.unlocked);
}

#[test]
fn unlock_with_wrong_password_maps_to_wrong_password() {
    let h = harness();
    h.state.create_vault(PW).unwrap();
    h.state.lock().unwrap();
    let err = h.state.unlock("not the password").expect_err("must fail");
    assert_matches!(err, AppError::WrongPassword);
}

#[test]
fn unlock_with_no_file_maps_to_no_vault() {
    let h = harness();
    let err = h.state.unlock(PW).expect_err("no vault to unlock");
    assert_matches!(err, AppError::NoVault);
}

#[test]
fn status_before_any_vault_reports_no_vault_locked() {
    let h = harness();
    let status = h.state.status();
    assert!(!status.has_vault);
    assert!(!status.unlocked);
    assert_eq!(status.idle_timeout_secs, TIMEOUT.as_secs());
}

#[test]
fn operations_while_locked_return_locked() {
    let h = harness();
    h.state.create_vault(PW).unwrap();
    h.state.lock().unwrap();

    assert_matches!(h.state.list_entries(), Err(AppError::Locked));
    assert_matches!(h.state.get_entry("whatever"), Err(AppError::Locked));
    assert_matches!(h.state.add_entry(sample_input("x")), Err(AppError::Locked));
    assert_matches!(
        h.state.update_entry("id", EntryPatchInput::default()),
        Err(AppError::Locked)
    );
    assert_matches!(h.state.delete_entry("id"), Err(AppError::Locked));
    assert_matches!(h.state.save(), Err(AppError::Locked));
}

#[test]
fn lock_then_unlock_preserves_entries() {
    let h = harness();
    h.state.create_vault(PW).unwrap();
    let id = h.state.add_entry(sample_input("github")).unwrap().id;

    h.state.lock().unwrap();
    h.state.unlock(PW).unwrap();

    let view = h.state.get_entry(&id).expect("entry survives lock cycle");
    assert_eq!(view.title, "github");
    assert_eq!(view.password, "github-pass");
}

// ---- Service CRUD ------------------------------------------------------------

#[test]
fn add_entry_returns_view_with_matching_timestamps_and_persists() {
    let h = harness();
    h.state.create_vault(PW).unwrap();

    let view = h.state.add_entry(sample_input("github")).expect("add");
    assert_eq!(view.title, "github");
    assert_eq!(view.username, "github-user");
    assert_eq!(view.password, "github-pass");
    assert_eq!(view.urls, vec!["https://github.example".to_string()]);
    assert_eq!(view.notes, "notes for github");
    // created == updated == the fake clock's wall time.
    assert_eq!(view.created, DEFAULT_WALL);
    assert_eq!(view.updated, DEFAULT_WALL);
    assert!(!view.id.is_empty());

    // Reopen the file from disk: the add was persisted immediately.
    let reopened = reopen(&h.vault_path);
    let stored = reopened.get_entry(&view.id).expect("persisted on add");
    assert_eq!(stored.title, "github");
    assert_eq!(stored.password, "github-pass");
}

#[test]
fn list_entries_summary_omits_password_and_uses_first_url() {
    let h = harness();
    h.state.create_vault(PW).unwrap();
    h.state
        .add_entry(EntryInput {
            title: "multi".into(),
            username: "u".into(),
            password: "secret".into(),
            urls: vec!["https://a.example".into(), "https://b.example".into()],
            notes: String::new(),
        })
        .unwrap();

    let list = h.state.list_entries().expect("list");
    assert_eq!(list.len(), 1);
    let s = &list[0];
    assert_eq!(s.title, "multi");
    assert_eq!(s.primary_url.as_deref(), Some("https://a.example"));
    // EntrySummary structurally has no password field; nothing to leak.
}

#[test]
fn summary_primary_url_is_none_when_no_urls() {
    let h = harness();
    h.state.create_vault(PW).unwrap();
    h.state
        .add_entry(EntryInput {
            title: "no-url".into(),
            username: "u".into(),
            password: "p".into(),
            urls: vec![],
            notes: String::new(),
        })
        .unwrap();

    let list = h.state.list_entries().unwrap();
    assert_eq!(list[0].primary_url, None);
}

#[test]
fn get_unknown_entry_errors() {
    let h = harness();
    h.state.create_vault(PW).unwrap();
    let err = h.state.get_entry("does-not-exist").expect_err("unknown id");
    assert_matches!(err, AppError::EntryNotFound(id) if id == "does-not-exist");
}

#[test]
fn update_bumps_only_updated_and_persists() {
    let h = harness();
    h.state.create_vault(PW).unwrap();
    let added = h.state.add_entry(sample_input("github")).unwrap();
    let id = added.id.clone();
    assert_eq!(added.created, DEFAULT_WALL);

    // Move wall time forward, then patch only the password.
    let later = "2026-06-14T12:00:00Z";
    h.clock.set_wall(later);
    let patch = EntryPatchInput {
        password: Some("new-pass".into()),
        ..Default::default()
    };
    let updated = h.state.update_entry(&id, patch).expect("update");

    assert_eq!(updated.password, "new-pass");
    assert_eq!(updated.title, "github", "untouched field preserved");
    assert_eq!(updated.created, DEFAULT_WALL, "created not bumped");
    assert_eq!(updated.updated, later, "updated bumped to new now");

    // Persisted to disk.
    let reopened = reopen(&h.vault_path);
    let stored = reopened.get_entry(&id).unwrap();
    assert_eq!(stored.password, "new-pass");
    assert_eq!(stored.updated, later);
    assert_eq!(stored.created, DEFAULT_WALL);
}

#[test]
fn update_unknown_entry_errors() {
    let h = harness();
    h.state.create_vault(PW).unwrap();
    let err = h
        .state
        .update_entry("nope", EntryPatchInput::default())
        .expect_err("unknown id");
    assert_matches!(err, AppError::EntryNotFound(id) if id == "nope");
}

#[test]
fn delete_removes_and_persists() {
    let h = harness();
    h.state.create_vault(PW).unwrap();
    let id = h.state.add_entry(sample_input("github")).unwrap().id;
    h.state.add_entry(sample_input("gitlab")).unwrap();

    h.state.delete_entry(&id).expect("delete");
    assert_eq!(h.state.list_entries().unwrap().len(), 1);
    assert_matches!(h.state.get_entry(&id), Err(AppError::EntryNotFound(_)));

    // Persisted: the reopened file no longer has the deleted entry.
    let reopened = reopen(&h.vault_path);
    assert!(reopened.get_entry(&id).is_none());
    assert_eq!(reopened.list_entries().len(), 1);
}

#[test]
fn delete_unknown_entry_errors() {
    let h = harness();
    h.state.create_vault(PW).unwrap();
    let err = h.state.delete_entry("ghost").expect_err("unknown id");
    assert_matches!(err, AppError::EntryNotFound(id) if id == "ghost");
}

// ---- Generator ---------------------------------------------------------------

#[test]
fn generate_honors_length_and_classes() {
    let h = harness();
    let opts = GeneratorOptionsDto {
        length: 40,
        lowercase: true,
        uppercase: false,
        digits: false,
        symbols: false,
        exclude_ambiguous: true,
    };
    let pw = h.state.generate_password(opts).expect("generate");
    assert_eq!(pw.chars().count(), 40);
    assert!(pw.chars().all(|c| c.is_ascii_lowercase()), "got {pw}");
}

#[test]
fn generate_with_empty_charset_errors() {
    let h = harness();
    let opts = GeneratorOptionsDto {
        length: 16,
        lowercase: false,
        uppercase: false,
        digits: false,
        symbols: false,
        exclude_ambiguous: false,
    };
    let err = h.state.generate_password(opts).expect_err("empty charset");
    assert_matches!(err, AppError::InvalidPasswordOptions);
}

#[test]
fn generate_with_zero_length_errors() {
    let h = harness();
    let opts = GeneratorOptionsDto {
        length: 0,
        ..GeneratorOptionsDto::default()
    };
    let err = h.state.generate_password(opts).expect_err("zero length");
    assert_matches!(err, AppError::InvalidPasswordOptions);
}

#[test]
fn generator_dto_default_matches_vault_core_default() {
    let dto = GeneratorOptionsDto::default();
    let core = PasswordOptions::default();
    assert_eq!(dto.length, core.length);
    assert_eq!(dto.lowercase, core.lowercase);
    assert_eq!(dto.uppercase, core.uppercase);
    assert_eq!(dto.digits, core.digits);
    assert_eq!(dto.symbols, core.symbols);
    assert_eq!(dto.exclude_ambiguous, core.exclude_ambiguous);
}

// ---- Auto-lock ---------------------------------------------------------------

#[test]
fn idle_lock_fires_after_timeout() {
    let h = harness_with_timeout(Duration::from_secs(60));
    h.state.create_vault(PW).unwrap();

    // Exactly at the timeout boundary: `>=` means it locks.
    h.clock.advance(Duration::from_secs(60));
    assert!(h.state.tick_autolock(), "should lock at the boundary");
    assert!(!h.state.status().unlocked);
}

#[test]
fn idle_lock_does_not_fire_before_timeout() {
    let h = harness_with_timeout(Duration::from_secs(60));
    h.state.create_vault(PW).unwrap();

    h.clock.advance(Duration::from_secs(59));
    assert!(!h.state.tick_autolock(), "must not lock before timeout");
    assert!(h.state.status().unlocked);
}

#[test]
fn activity_resets_the_idle_countdown() {
    let h = harness_with_timeout(Duration::from_secs(60));
    h.state.create_vault(PW).unwrap();

    // Almost there...
    h.clock.advance(Duration::from_secs(59));
    assert!(!h.state.tick_autolock());
    // ...activity (a read) resets last_activity to "now".
    h.state.list_entries().unwrap();
    // Another 59s: still under the timeout because the clock was reset.
    h.clock.advance(Duration::from_secs(59));
    assert!(!h.state.tick_autolock(), "activity should have reset timer");
    assert!(h.state.status().unlocked);

    // Now let the full window elapse without activity.
    h.clock.advance(Duration::from_secs(1));
    assert!(
        h.state.tick_autolock(),
        "locks once idle window truly elapses"
    );
}

#[test]
fn repeated_activity_never_locks() {
    let h = harness_with_timeout(Duration::from_secs(60));
    h.state.create_vault(PW).unwrap();

    for _ in 0..10 {
        h.clock.advance(Duration::from_secs(59));
        // Each operation bumps activity, so the idle window never elapses.
        h.state.list_entries().unwrap();
        assert!(!h.state.tick_autolock(), "should never lock under activity");
    }
    assert!(h.state.status().unlocked);
}

#[test]
fn tick_on_locked_session_is_a_noop() {
    let h = harness_with_timeout(Duration::from_secs(60));
    // Never unlocked: ticking is a harmless no-op returning false.
    assert!(!h.state.tick_autolock());

    h.state.create_vault(PW).unwrap();
    h.state.lock().unwrap();
    h.clock.advance(Duration::from_secs(600));
    assert!(!h.state.tick_autolock(), "no-op once already locked");
}

#[test]
fn operations_after_autolock_return_locked() {
    let h = harness_with_timeout(Duration::from_secs(60));
    h.state.create_vault(PW).unwrap();
    h.state.add_entry(sample_input("github")).unwrap();

    h.clock.advance(Duration::from_secs(60));
    assert!(h.state.tick_autolock());

    // Every operation now reports the vault as locked.
    assert_matches!(h.state.list_entries(), Err(AppError::Locked));
    assert_matches!(h.state.add_entry(sample_input("x")), Err(AppError::Locked));

    // ...and re-unlocking restores access to the persisted entries.
    h.state.unlock(PW).unwrap();
    assert_eq!(h.state.list_entries().unwrap().len(), 1);
}

// ---- Error mapping -----------------------------------------------------------

#[test]
fn entry_not_found_serializes_to_kind_and_message() {
    let value = serde_json::to_value(AppError::EntryNotFound("x".into())).unwrap();
    assert_eq!(
        value,
        serde_json::json!({ "kind": "EntryNotFound", "message": "x" })
    );
}

#[test]
fn locked_serializes_without_message_payload() {
    let value = serde_json::to_value(AppError::Locked).unwrap();
    // Variants without data serialize as just the `kind` tag (no `message`).
    assert_eq!(value, serde_json::json!({ "kind": "Locked" }));
}

#[test]
fn wrong_password_serializes_to_its_kind() {
    let value = serde_json::to_value(AppError::WrongPassword).unwrap();
    assert_eq!(value, serde_json::json!({ "kind": "WrongPassword" }));
}

#[test]
fn malformed_vault_variants_collapse_to_invalid_vault_file() {
    use vault_core::VaultError;
    for e in [
        VaultError::BadMagic,
        VaultError::UnsupportedVersion(99),
        VaultError::UnsupportedKdf(7),
        VaultError::UnsupportedCipher(7),
        VaultError::MalformedFile,
        VaultError::InvalidKdfParams,
    ] {
        let mapped: AppError = e.into();
        assert_matches!(mapped, AppError::InvalidVaultFile);
    }
}

#[test]
fn unlocking_a_truncated_file_is_indistinguishable_from_wrong_password() {
    // A structurally-broken vault file must surface as WrongPassword through the
    // service (not InvalidVaultFile), preserving the "can't tell tamper from bad
    // password" property at the unlock boundary.
    let h = harness();
    h.state.create_vault(PW).unwrap();
    h.state.lock().unwrap();

    // Corrupt the file on disk.
    let mut bytes = std::fs::read(&h.vault_path).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 0xff;
    std::fs::write(&h.vault_path, &bytes).unwrap();

    let err = h.state.unlock(PW).expect_err("tampered vault");
    assert_matches!(err, AppError::WrongPassword);
}

// ---- Paths -------------------------------------------------------------------

#[test]
fn vault_path_ends_with_vault_spk() {
    // We cannot assert the absolute prefix (it varies per OS/user), but the file
    // name component is fixed by the contract.
    use std::path::Path;
    assert_eq!(app_core::paths::VAULT_FILE_NAME, "vault.spk");
    // Construct what resolve_vault_path joins and confirm the leaf.
    let joined = Path::new("/data/dir").join(app_core::paths::VAULT_FILE_NAME);
    assert!(joined.ends_with("vault.spk"));
}

#[test]
fn has_vault_detection_tracks_the_file() {
    let h = harness();
    assert!(!h.state.status().has_vault, "no file yet");
    h.state.create_vault(PW).unwrap();
    assert!(h.state.status().has_vault, "file present after create");
}
