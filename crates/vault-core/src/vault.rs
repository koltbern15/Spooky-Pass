//! The vault typestate: [`LockedVault`] and [`UnlockedVault`].
//!
//! These are two *distinct* types rather than one type with an `Option<Key>`,
//! so the type system enforces that you can only read/modify entries while
//! unlocked, and that a locked vault never holds key material.
//!
//! * [`LockedVault`] — header + last-known ciphertext, no secrets. Produced by
//!   opening a file or by locking an [`UnlockedVault`].
//! * [`UnlockedVault`] — owns the session (and thus the derived key) plus the
//!   in-memory vault body. This *is* the unlocked session.

use std::fs;
use std::path::Path;

use serde::Serialize;
use zeroize::Zeroizing;

use crate::crypto::{self, KdfParams};
use crate::entry::{Entry, EntryPatch, NewEntry, VaultBody};
use crate::error::{Result, VaultError};
use crate::format::{self, HeaderFields, ParsedHeader, NONCE_LEN, SALT_LEN};
use crate::session::Session;
use rand_core::{OsRng, RngCore};

/// A vault at rest: a parsed header plus the sealed ciphertext, holding no key
/// material.
///
/// Obtain one by opening a file ([`LockedVault::open_from_bytes`] /
/// [`LockedVault::open_from_path`]) or by locking an [`UnlockedVault`]
/// ([`UnlockedVault::lock`]). Unlock it with
/// [`UnlockedVault::unlock`].
#[derive(Debug, Clone)]
pub struct LockedVault {
    header: ParsedHeader,
    ciphertext: Vec<u8>,
}

impl LockedVault {
    /// Create a brand-new, empty vault and return it **unlocked**.
    ///
    /// Generates a fresh random salt and nonce, derives the key from
    /// `master_password`, and yields an [`UnlockedVault`] with no entries. The
    /// vault is not written to disk until you call
    /// [`UnlockedVault::save_to_path`] (or serialize via
    /// [`UnlockedVault::to_bytes`]).
    ///
    /// The `master_password` is borrowed and never copied or stored; the caller
    /// owns and is responsible for zeroizing it.
    pub fn create(master_password: &str, params: KdfParams) -> Result<UnlockedVault> {
        let mut salt = [0u8; SALT_LEN];
        OsRng.fill_bytes(&mut salt);

        let key = crypto::derive_key(master_password, &salt, &params)?;
        let session = Session::new(key, salt, params);
        Ok(UnlockedVault {
            session,
            body: VaultBody::default(),
        })
    }

    /// Parse and validate a vault file's header and framing. Performs **no**
    /// decryption (no password is required).
    pub fn open_from_bytes(bytes: &[u8]) -> Result<LockedVault> {
        let (header, ciphertext) = format::decode(bytes)?;
        Ok(LockedVault {
            header,
            ciphertext: ciphertext.to_vec(),
        })
    }

    /// Read a vault file from `path` and parse it (see
    /// [`LockedVault::open_from_bytes`]).
    pub fn open_from_path(path: &Path) -> Result<LockedVault> {
        let bytes = fs::read(path)?;
        LockedVault::open_from_bytes(&bytes)
    }

    /// The parsed header (KDF params, salt, nonce) of this vault.
    pub fn header(&self) -> &ParsedHeader {
        &self.header
    }
}

/// An unlocked, in-memory vault — this *is* the unlocked session.
///
/// Owns the session (and thus the derived key) and the decrypted vault
/// body. Read and mutate entries here; call
/// [`UnlockedVault::save_to_path`] to persist and [`UnlockedVault::lock`] to
/// zeroize the key and get a relockable [`LockedVault`] back.
pub struct UnlockedVault {
    session: Session,
    body: VaultBody,
}

impl UnlockedVault {
    /// Unlock a [`LockedVault`] with `master_password`, returning a fresh
    /// [`UnlockedVault`] on success.
    ///
    /// Re-derives the key from the header's salt/params and AEAD-opens the body
    /// (binding the full header as associated data).
    ///
    /// # Errors
    ///
    /// Returns [`VaultError::WrongPassword`] for **every** AEAD-open failure —
    /// an incorrect password and a tampered body are cryptographically
    /// indistinguishable, and we deliberately do not leak which occurred.
    pub fn unlock(locked: &LockedVault, master_password: &str) -> Result<UnlockedVault> {
        let header = &locked.header;
        let params = KdfParams {
            m_cost_kib: header.m_cost_kib,
            t_cost: header.t_cost,
            p_cost: header.p_cost,
        };

        let key = crypto::derive_key(master_password, &header.salt, &params)?;

        // Open the body, binding the full header as AAD. Any failure here is
        // reported as WrongPassword.
        let plaintext: Zeroizing<Vec<u8>> = Zeroizing::new(crypto::open(
            &key,
            &header.nonce,
            &locked.ciphertext,
            &header.header_bytes,
        )?);

        let body: VaultBody =
            serde_json::from_slice(&plaintext).map_err(|_| VaultError::Serialization)?;

        let session = Session::new(key, header.salt, params);
        Ok(UnlockedVault { session, body })
    }

    /// Lock the vault: consume `self`, zeroizing the derived key, and return a
    /// [`LockedVault`] holding the freshly re-sealed ciphertext + header.
    ///
    /// The returned handle can be unlocked again with [`UnlockedVault::unlock`].
    /// If re-sealing fails (which should not happen for a valid in-memory body)
    /// the key is still dropped/zeroized.
    pub fn lock(self) -> LockedVault {
        // Re-seal so the returned LockedVault reflects the current body. If
        // serialization somehow fails, fall back to an empty-body seal so we
        // never panic; the key is zeroized either way when `self` drops.
        let (header, ciphertext) = self.seal_current().unwrap_or_else(|_| {
            self.seal_body(&VaultBody::default())
                .expect("empty body seals")
        });
        // `self` (and its Session/key) drops here, zeroizing the key.
        LockedVault { header, ciphertext }
    }

    /// Add a new entry, assigning a fresh id and setting `created == updated`
    /// from the caller-supplied timestamp. Returns a reference to the stored
    /// entry.
    pub fn add_entry(&mut self, new: NewEntry) -> Result<&Entry> {
        let id = new_id();
        let entry = Entry {
            id,
            title: new.title,
            username: new.username,
            password: new.password,
            urls: new.urls,
            notes: new.notes,
            created: new.now.clone(),
            updated: new.now,
        };
        self.body.entries.push(entry);
        // The just-pushed entry is the last one.
        Ok(self.body.entries.last().expect("entry was just pushed"))
    }

    /// Get an entry by id, or `None` if no such entry exists.
    pub fn get_entry(&self, id: &str) -> Option<&Entry> {
        self.body.entries.iter().find(|e| e.id == id)
    }

    /// Apply a partial update to the entry with `id`, bumping its `updated`
    /// timestamp to the patch's `now`. Returns a reference to the updated entry.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError::EntryNotFound`] if no entry has that id.
    pub fn update_entry(&mut self, id: &str, patch: EntryPatch) -> Result<&Entry> {
        let entry = self
            .body
            .entries
            .iter_mut()
            .find(|e| e.id == id)
            .ok_or_else(|| VaultError::EntryNotFound(id.to_string()))?;

        if let Some(title) = patch.title {
            entry.title = title;
        }
        if let Some(username) = patch.username {
            entry.username = username;
        }
        if let Some(password) = patch.password {
            entry.password = password;
        }
        if let Some(urls) = patch.urls {
            entry.urls = urls;
        }
        if let Some(notes) = patch.notes {
            entry.notes = notes;
        }
        entry.updated = patch.now;
        Ok(entry)
    }

    /// Delete the entry with `id`.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError::EntryNotFound`] if no entry has that id.
    pub fn delete_entry(&mut self, id: &str) -> Result<()> {
        let before = self.body.entries.len();
        self.body.entries.retain(|e| e.id != id);
        if self.body.entries.len() == before {
            return Err(VaultError::EntryNotFound(id.to_string()));
        }
        Ok(())
    }

    /// All entries, in insertion order.
    pub fn list_entries(&self) -> &[Entry] {
        &self.body.entries
    }

    /// Serialize and seal the current vault into the on-disk byte format,
    /// using a **fresh random nonce** every call.
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let (fields, ciphertext) = self.seal_body_with_fields(&self.body)?;
        Ok(format::encode(&fields, &ciphertext))
    }

    /// Serialize, seal, and atomically write the vault to `path`.
    ///
    /// Writes to a uniquely named temporary file in the same directory, flushes
    /// it, and renames it into place. A rename within a directory is atomic on
    /// POSIX, so a crash mid-write cannot truncate or corrupt an existing
    /// vault. Uses a fresh random nonce.
    pub fn save_to_path(&self, path: &Path) -> Result<()> {
        use std::io::Write;

        let bytes = self.to_bytes()?;

        let dir = path.parent().filter(|p| !p.as_os_str().is_empty());
        let dir = dir.unwrap_or_else(|| Path::new("."));

        // Unique temp file name in the destination directory so the rename is a
        // same-filesystem (atomic) move.
        let mut rnd = [0u8; 8];
        OsRng.fill_bytes(&mut rnd);
        let tmp_path = dir.join(format!(".spookypass-tmp-{}", hex_encode_lower(&rnd)));

        // Scope the file handle so it is closed before the rename.
        let write_result = (|| -> std::io::Result<()> {
            let mut f = fs::File::create(&tmp_path)?;
            f.write_all(&bytes)?;
            f.flush()?;
            f.sync_all()?;
            Ok(())
        })();

        if let Err(e) = write_result {
            // Best-effort cleanup; ignore errors removing the temp file.
            let _ = fs::remove_file(&tmp_path);
            return Err(VaultError::Io(e));
        }

        if let Err(e) = fs::rename(&tmp_path, path) {
            let _ = fs::remove_file(&tmp_path);
            return Err(VaultError::Io(e));
        }
        Ok(())
    }

    /// Re-seal the current body and return a [`ParsedHeader`] + ciphertext (for
    /// [`lock`](Self::lock)).
    fn seal_current(&self) -> Result<(ParsedHeader, Vec<u8>)> {
        self.seal_body(&self.body)
    }

    /// Seal an arbitrary body with a fresh nonce, returning a [`ParsedHeader`]
    /// + ciphertext.
    fn seal_body(&self, body: &VaultBody) -> Result<(ParsedHeader, Vec<u8>)> {
        let (fields, ciphertext) = self.seal_body_with_fields(body)?;
        let header = ParsedHeader {
            format_version: format::FORMAT_VERSION,
            kdf_id: format::KDF_ID_ARGON2ID,
            cipher_id: format::CIPHER_ID_XCHACHA20POLY1305,
            m_cost_kib: fields.m_cost_kib,
            t_cost: fields.t_cost,
            p_cost: fields.p_cost,
            argon2_version: format::ARGON2_VERSION,
            salt: fields.salt,
            nonce: fields.nonce,
            header_bytes: format::build_header_bytes(&fields),
        };
        Ok((header, ciphertext))
    }

    /// Serialize `body`, generate a fresh nonce, build the header, and seal —
    /// binding the full header as AAD. Returns the [`HeaderFields`] used and the
    /// ciphertext, which together fully determine the on-disk file.
    fn seal_body_with_fields(&self, body: &VaultBody) -> Result<(HeaderFields, Vec<u8>)> {
        // Serialize into a zeroizing buffer so the plaintext JSON is wiped.
        let mut json = Zeroizing::new(Vec::new());
        let mut ser = serde_json::Serializer::new(&mut *json);
        body.serialize(&mut ser)
            .map_err(|_| VaultError::Serialization)?;

        // Fresh nonce per save — never reuse.
        let mut nonce = [0u8; NONCE_LEN];
        OsRng.fill_bytes(&mut nonce);

        let fields = HeaderFields {
            m_cost_kib: self.session.params().m_cost_kib,
            t_cost: self.session.params().t_cost,
            p_cost: self.session.params().p_cost,
            salt: *self.session.salt(),
            nonce,
        };
        let header_bytes = format::build_header_bytes(&fields);

        let ciphertext = crypto::seal(self.session.key(), &nonce, &json, &header_bytes)?;
        Ok((fields, ciphertext))
    }
}

/// Generate a 32-char lowercase-hex id from 16 random `OsRng` bytes.
fn new_id() -> String {
    let mut raw = [0u8; 16];
    OsRng.fill_bytes(&mut raw);
    hex_encode_lower(&raw)
}

/// Lowercase-hex encode without pulling in an extra dependency.
fn hex_encode_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 0x0f) as usize] as char);
    }
    s
}

// Redacting Debug: never print key material, passwords, or entry secrets.
impl core::fmt::Debug for UnlockedVault {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("UnlockedVault")
            .field("session", &"<redacted>")
            .field("entry_count", &self.body.entries.len())
            .finish_non_exhaustive()
    }
}
