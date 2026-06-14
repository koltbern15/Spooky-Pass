//! Registrable-domain (eTLD+1) autofill matching — the one security-critical
//! decision in the app, isolated here in Tauri-free Rust so it is unit-tested
//! exhaustively.
//!
//! ## How a page "matches" an entry
//!
//! A page URL matches a stored entry iff they share the same **registrable
//! domain** (eTLD+1), computed from the embedded Public Suffix List. So a login
//! saved on `accounts.google.com` fills on `mail.google.com` (both →
//! `google.com`), while `google.com.evil.ru` (→ `evil.ru`) does **not** match.
//! We never do naive substring matching — that is exactly the bug that hands
//! passwords to phishers.
//!
//! ## Fail closed
//!
//! [`registrable_domain`] returns `None` — and the page therefore matches
//! *nothing* — whenever the host is missing, an IP literal, `localhost`, or sits
//! under an unknown/unlisted suffix. In the autofill threat model an unmatched
//! page (the user just doesn't get a fill) is always safer than a wrong match
//! (a secret typed into the wrong origin), so every ambiguous case resolves to
//! "no match".
//!
//! ## Activity / auto-lock interaction
//!
//! [`AppState::find_matches`] is a *probe*: the extension's background worker may
//! call it for any page load, so it must **not** bump the session's activity
//! timestamp — otherwise merely browsing would defer idle auto-lock forever.
//! [`AppState::get_credential`], by contrast, is only ever issued from a real
//! user click to actually fill a password, so it *does* bump activity.

use vault_core::Entry;

use crate::autofill::contract::{AutofillStatusDto, CredentialDto, MatchCandidate};
use crate::error::{AppError, Result};
use crate::state::{AppState, SessionState};

/// Compute the registrable domain (eTLD+1) of a host string, or `None` if there
/// isn't an unambiguous, publicly-listed one.
///
/// Returns `None` (fail closed) for IP literals, `localhost`, bare hostnames,
/// and anything under an unknown suffix — in all those cases requiring
/// [`psl::Suffix::is_known`] keeps us from inventing a registrable domain the
/// PSL never sanctioned. The host is lowercased and any single trailing dot
/// (the FQDN root) is stripped before lookup so matching is case-insensitive and
/// dot-insensitive.
fn registrable_domain(host: &str) -> Option<String> {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    if host.is_empty() {
        return None;
    }
    let domain = psl::domain(host.as_bytes())?;
    // Only accept a registrable domain whose suffix is explicitly in the PSL.
    // An unknown suffix (IP literals, made-up TLDs, bare hostnames) yields a
    // `Domain` via the implicit `*` rule with `is_known() == false`; rejecting
    // those is what makes IP/localhost/unknown-TLD fail closed.
    if !domain.suffix().is_known() {
        return None;
    }
    std::str::from_utf8(domain.as_bytes())
        .ok()
        .map(str::to_owned)
}

/// Compute the registrable domain of a full URL string (scheme-agnostic).
///
/// Parses with [`url::Url`], extracts the host, and defers to
/// [`registrable_domain`]. Returns `None` on any parse failure or hostless URL
/// (so the caller fails closed).
fn url_registrable_domain(raw: &str) -> Option<String> {
    let url = url::Url::parse(raw).ok()?;
    let host = url.host_str()?;
    registrable_domain(host)
}

/// Decide whether a single stored entry matches `page_domain` (an already
/// computed, lowercased registrable domain).
///
/// An entry matches when **any** of its URLs has the same registrable domain as
/// the page. Routing the per-entry decision through this one function leaves a
/// clean seam for a future per-entry override (exact-host-only, or extra allowed
/// domains) without threading a new field through `vault-core`'s [`Entry`].
fn entry_matches(entry: &Entry, page_domain: &str) -> bool {
    entry
        .urls
        .iter()
        .filter_map(|u| url_registrable_domain(u))
        .any(|d| d == page_domain)
}

/// Project a matching entry into a password-free [`MatchCandidate`].
///
/// `primary_url` is the entry's first URL, mirroring the `EntrySummary`
/// projection. By construction the candidate carries no password — that is a
/// compile-time property of the [`MatchCandidate`] type, not a runtime filter.
fn match_candidate(entry: &Entry) -> MatchCandidate {
    MatchCandidate {
        id: entry.id.clone(),
        title: entry.title.clone(),
        username: entry.username.clone(),
        primary_url: entry.urls.first().cloned(),
    }
}

impl AppState {
    /// Find password-free credential candidates whose registrable domain matches
    /// `page_url`'s.
    ///
    /// Locked vault, an unparseable URL, or a fail-closed host (IP, `localhost`,
    /// unknown suffix) all yield `Ok(vec![])` — never an error, so a background
    /// probe of any page is harmless. **Does not bump activity**: this is a probe
    /// the extension may run on every navigation, and it must not defer idle
    /// auto-lock.
    pub fn find_matches(&self, page_url: &str) -> Result<Vec<MatchCandidate>> {
        let session = self.lock_session();

        let vault = match &session.state {
            SessionState::Unlocked { vault, .. } => vault,
            SessionState::Locked => return Ok(Vec::new()),
        };

        let Some(page_domain) = url_registrable_domain(page_url) else {
            return Ok(Vec::new());
        };

        let matches = vault
            .list_entries()
            .iter()
            .filter(|entry| entry_matches(entry, &page_domain))
            .map(match_candidate)
            .collect();

        Ok(matches)
    }

    /// Fetch the full credential (including password) for entry `id`.
    ///
    /// This is the only autofill path that returns a password, and it is only
    /// ever issued from a real user-gesture click, so it **does bump activity**
    /// (a real fill is real use). Returns [`AppError::Locked`] if the vault is
    /// locked, or [`AppError::EntryNotFound`] for an unknown id.
    pub fn get_credential(&self, id: &str) -> Result<CredentialDto> {
        let mut session = self.lock_session();

        let dto = {
            let vault = match &session.state {
                SessionState::Unlocked { vault, .. } => vault,
                SessionState::Locked => return Err(AppError::Locked),
            };
            let entry = vault
                .get_entry(id)
                .ok_or_else(|| AppError::EntryNotFound(id.to_string()))?;
            CredentialDto {
                id: entry.id.clone(),
                username: entry.username.clone(),
                password: entry.password.clone(),
            }
        };

        Self::bump_activity(&mut session);
        Ok(dto)
    }

    /// The autofill status the extension affordance branches on: whether the
    /// Core is unlocked and whether a vault file exists. Infallible; does not
    /// bump activity (a passive status probe is not user activity).
    pub fn autofill_status(&self) -> AutofillStatusDto {
        let session = self.lock_session();
        AutofillStatusDto {
            unlocked: matches!(session.state, SessionState::Unlocked { .. }),
            has_vault: session.vault_path.exists(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registrable_domain_strips_case_and_trailing_dot() {
        assert_eq!(
            registrable_domain("Mail.Google.COM."),
            Some("google.com".to_string())
        );
    }

    #[test]
    fn registrable_domain_fails_closed_on_ip_and_localhost() {
        assert_eq!(registrable_domain("127.0.0.1"), None);
        assert_eq!(registrable_domain("localhost"), None);
        assert_eq!(registrable_domain("example"), None);
        assert_eq!(registrable_domain("example.invalidtldxyz"), None);
    }

    #[test]
    fn phishing_hosts_do_not_collapse_to_the_target() {
        let target = registrable_domain("google.com").unwrap();
        for evil in [
            "google.com.evil.ru",
            "googlecom.evil.ru",
            "google.com@evil.ru",
        ] {
            // `@evil.ru` is userinfo, but at the host level it never appears;
            // the host of these is evil.ru / a bare label, never google.com.
            assert_ne!(registrable_domain(evil).as_deref(), Some(target.as_str()));
        }
    }
}
