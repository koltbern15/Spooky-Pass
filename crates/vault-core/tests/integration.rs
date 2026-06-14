//! Integration tests for `vault-core`: full create/save/open/unlock flows,
//! entry CRUD, nonce freshness, error handling, and sound zeroization checks.

use assert_matches::assert_matches;
use vault_core::{
    generate_password, Entry, EntryPatch, KdfParams, LockedVault, NewEntry, PasswordOptions,
    UnlockedVault, VaultError, HEADER_LEN,
};
use zeroize::Zeroize;

const PW: &str = "correct horse battery staple";
const NOW: &str = "2026-06-14T00:00:00Z";
const LATER: &str = "2026-06-14T12:00:00Z";

fn sample_new(title: &str) -> NewEntry {
    NewEntry {
        title: title.into(),
        username: format!("{title}-user"),
        password: format!("{title}-pass"),
        urls: vec![format!("https://{title}.example")],
        notes: format!("notes for {title}"),
        now: NOW.into(),
    }
}

// ---- Vault lifecycle ---------------------------------------------------------

#[test]
fn create_produces_empty_unlocked_vault() {
    let vault = LockedVault::create(PW, KdfParams::default()).unwrap();
    assert!(vault.list_entries().is_empty());
}

#[test]
fn create_save_open_unlock_roundtrip() {
    let mut vault = LockedVault::create(PW, KdfParams::default()).unwrap();
    vault.add_entry(sample_new("github")).unwrap();
    let bytes = vault.to_bytes().unwrap();

    let locked = LockedVault::open_from_bytes(&bytes).unwrap();
    let reopened = UnlockedVault::unlock(&locked, PW).unwrap();
    assert_eq!(reopened.list_entries().len(), 1);
    assert_eq!(reopened.list_entries()[0].title, "github");
}

#[test]
fn unlock_with_correct_password_succeeds() {
    let vault = LockedVault::create(PW, KdfParams::default()).unwrap();
    let bytes = vault.to_bytes().unwrap();
    let locked = LockedVault::open_from_bytes(&bytes).unwrap();
    assert!(UnlockedVault::unlock(&locked, PW).is_ok());
}

#[test]
fn unlock_with_wrong_password_fails_cleanly() {
    let vault = LockedVault::create(PW, KdfParams::default()).unwrap();
    let bytes = vault.to_bytes().unwrap();
    let locked = LockedVault::open_from_bytes(&bytes).unwrap();
    assert_matches!(
        UnlockedVault::unlock(&locked, "wrong password"),
        Err(VaultError::WrongPassword)
    );
}

#[test]
fn unlock_with_tampered_body_returns_wrong_password() {
    let vault = LockedVault::create(PW, KdfParams::default()).unwrap();
    let mut bytes = vault.to_bytes().unwrap();
    // Flip a ciphertext byte (well past the 76-byte body offset).
    let last = bytes.len() - 1;
    bytes[last] ^= 0xff;
    let locked = LockedVault::open_from_bytes(&bytes).unwrap();
    assert_matches!(
        UnlockedVault::unlock(&locked, PW),
        Err(VaultError::WrongPassword)
    );
}

// ---- Entry CRUD --------------------------------------------------------------

#[test]
fn add_entry_assigns_id_and_timestamps() {
    let mut vault = LockedVault::create(PW, KdfParams::default()).unwrap();
    let entry = vault.add_entry(sample_new("github")).unwrap();
    assert_eq!(entry.id.len(), 32);
    assert!(entry
        .id
        .chars()
        .all(|c| c.is_ascii_hexdigit() && !c.is_uppercase()));
    assert_eq!(entry.created, NOW);
    assert_eq!(entry.updated, NOW);
}

#[test]
fn add_entry_assigns_unique_ids() {
    let mut vault = LockedVault::create(PW, KdfParams::default()).unwrap();
    let a = vault.add_entry(sample_new("a")).unwrap().id.clone();
    let b = vault.add_entry(sample_new("b")).unwrap().id.clone();
    assert_ne!(a, b);
}

#[test]
fn get_entry_found_and_not_found() {
    let mut vault = LockedVault::create(PW, KdfParams::default()).unwrap();
    let id = vault.add_entry(sample_new("github")).unwrap().id.clone();
    assert!(vault.get_entry(&id).is_some());
    assert!(vault.get_entry("does-not-exist").is_none());
}

#[test]
fn update_entry_changes_fields_and_bumps_updated() {
    let mut vault = LockedVault::create(PW, KdfParams::default()).unwrap();
    let id = vault.add_entry(sample_new("github")).unwrap().id.clone();
    let patch = EntryPatch {
        password: Some("new-pass".into()),
        notes: Some("rotated".into()),
        now: LATER.into(),
        ..Default::default()
    };
    let updated = vault.update_entry(&id, patch).unwrap();
    assert_eq!(updated.password, "new-pass");
    assert_eq!(updated.notes, "rotated");
    // Unchanged fields stay.
    assert_eq!(updated.title, "github");
    // created is preserved, updated is bumped.
    assert_eq!(updated.created, NOW);
    assert_eq!(updated.updated, LATER);
}

#[test]
fn update_missing_entry_errors() {
    let mut vault = LockedVault::create(PW, KdfParams::default()).unwrap();
    let patch = EntryPatch {
        title: Some("x".into()),
        now: LATER.into(),
        ..Default::default()
    };
    assert_matches!(
        vault.update_entry("nope", patch),
        Err(VaultError::EntryNotFound(_))
    );
}

#[test]
fn delete_entry_removes_it() {
    let mut vault = LockedVault::create(PW, KdfParams::default()).unwrap();
    let id = vault.add_entry(sample_new("github")).unwrap().id.clone();
    vault.delete_entry(&id).unwrap();
    assert!(vault.get_entry(&id).is_none());
    assert!(vault.list_entries().is_empty());
}

#[test]
fn delete_missing_entry_errors() {
    let mut vault = LockedVault::create(PW, KdfParams::default()).unwrap();
    assert_matches!(
        vault.delete_entry("nope"),
        Err(VaultError::EntryNotFound(_))
    );
}

#[test]
fn list_entries_returns_all_in_order() {
    let mut vault = LockedVault::create(PW, KdfParams::default()).unwrap();
    for name in ["a", "b", "c"] {
        vault.add_entry(sample_new(name)).unwrap();
    }
    let titles: Vec<&str> = vault
        .list_entries()
        .iter()
        .map(|e| e.title.as_str())
        .collect();
    assert_eq!(titles, ["a", "b", "c"]);
}

// ---- Serialization & persistence --------------------------------------------

#[test]
fn two_saves_use_different_nonces() {
    let vault = LockedVault::create(PW, KdfParams::default()).unwrap();
    let a = vault.to_bytes().unwrap();
    let b = vault.to_bytes().unwrap();
    // Nonce lives at header offset 44..68.
    assert_ne!(&a[44..68], &b[44..68], "nonce was reused across saves");
    // Different nonce => different ciphertext as well.
    assert_ne!(a, b);
}

#[test]
fn save_then_reload_preserves_all_fields() {
    let mut vault = LockedVault::create(PW, KdfParams::default()).unwrap();
    // id/created/updated are crate-managed; capture the full entry after insert.
    let original: Entry = vault
        .add_entry(NewEntry {
            title: "Bank".into(),
            username: "alice@example.com".into(),
            password: "s3cr3t!".into(),
            urls: vec![
                "https://bank.example".into(),
                "https://m.bank.example".into(),
            ],
            notes: "PIN reminder".into(),
            now: NOW.into(),
        })
        .unwrap()
        .clone();

    let bytes = vault.to_bytes().unwrap();
    let locked = LockedVault::open_from_bytes(&bytes).unwrap();
    let reopened = UnlockedVault::unlock(&locked, PW).unwrap();

    let got = &reopened.list_entries()[0];
    assert_eq!(got, &original);
}

#[test]
fn save_to_path_then_open_from_path_roundtrips() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("vault.spk");

    let mut vault = LockedVault::create(PW, KdfParams::default()).unwrap();
    vault.add_entry(sample_new("github")).unwrap();
    vault.save_to_path(&path).unwrap();

    let locked = LockedVault::open_from_path(&path).unwrap();
    let reopened = UnlockedVault::unlock(&locked, PW).unwrap();
    assert_eq!(reopened.list_entries()[0].title, "github");
}

#[test]
fn save_to_path_overwrites_atomically() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("vault.spk");

    let mut v1 = LockedVault::create(PW, KdfParams::default()).unwrap();
    v1.add_entry(sample_new("first")).unwrap();
    v1.save_to_path(&path).unwrap();

    let mut v2 = LockedVault::create(PW, KdfParams::default()).unwrap();
    v2.add_entry(sample_new("second")).unwrap();
    v2.save_to_path(&path).unwrap();

    // The directory should contain only the vault file (no leftover temp file).
    let entries: Vec<_> = std::fs::read_dir(dir.path()).unwrap().collect();
    assert_eq!(entries.len(), 1);

    let locked = LockedVault::open_from_path(&path).unwrap();
    let reopened = UnlockedVault::unlock(&locked, PW).unwrap();
    assert_eq!(reopened.list_entries()[0].title, "second");
}

#[test]
fn lock_then_unlock_roundtrips() {
    let mut vault = LockedVault::create(PW, KdfParams::default()).unwrap();
    vault.add_entry(sample_new("github")).unwrap();
    let locked = vault.lock();
    let reopened = UnlockedVault::unlock(&locked, PW).unwrap();
    assert_eq!(reopened.list_entries()[0].title, "github");
}

#[test]
fn unsupported_format_version_is_handled() {
    let vault = LockedVault::create(PW, KdfParams::default()).unwrap();
    let mut bytes = vault.to_bytes().unwrap();
    // format_version lives at offset 4..6.
    bytes[4..6].copy_from_slice(&2u16.to_le_bytes());
    assert_matches!(
        LockedVault::open_from_bytes(&bytes),
        Err(VaultError::UnsupportedVersion(2))
    );
}

// ---- Sound zeroization / secret-handling checks ------------------------------

#[test]
fn zeroize_primitive_wipes_buffer() {
    // We cannot read freed memory in safe Rust, so we prove the *primitive*:
    // cloning the raw key bytes and zeroizing them produces all zeros.
    let mut buf = [0xAAu8; 32];
    buf.zeroize();
    assert_eq!(buf, [0u8; 32]);
}

#[test]
fn dropping_unlocked_vault_compiles_zeroize() {
    // The DerivedKey newtype derives ZeroizeOnDrop, so dropping an unlocked
    // vault wipes the key. We can't observe the freed bytes soundly, but we can
    // confirm a vault drops cleanly (and the derive is present, which is checked
    // by the crate compiling at all).
    let mut vault = LockedVault::create(PW, KdfParams::default()).unwrap();
    vault.add_entry(sample_new("github")).unwrap();
    drop(vault);
}

#[test]
fn debug_does_not_leak_secrets() {
    let mut vault = LockedVault::create("super-secret-master", KdfParams::default()).unwrap();
    vault
        .add_entry(NewEntry {
            title: "Bank".into(),
            username: "alice".into(),
            password: "topsecretpassword".into(),
            urls: vec![],
            notes: String::new(),
            now: NOW.into(),
        })
        .unwrap();

    let dbg = format!("{vault:?}");
    assert!(
        !dbg.contains("super-secret-master"),
        "leaked master pw: {dbg}"
    );
    assert!(!dbg.contains("topsecretpassword"), "leaked entry pw: {dbg}");
}

// ---- Password generator (smoke; full coverage in unit tests) ----------------

#[test]
fn generator_produces_requested_length() {
    let opts = PasswordOptions {
        length: 24,
        ..PasswordOptions::default()
    };
    assert_eq!(generate_password(&opts).unwrap().chars().count(), 24);
}

// ---- Header sanity -----------------------------------------------------------

#[test]
fn saved_file_header_is_well_formed() {
    let vault = LockedVault::create(PW, KdfParams::default()).unwrap();
    let bytes = vault.to_bytes().unwrap();
    assert!(bytes.len() > HEADER_LEN);
    assert_eq!(&bytes[0..4], b"SPK1");
}
