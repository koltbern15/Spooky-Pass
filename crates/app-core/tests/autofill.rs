//! Integration tests for registrable-domain autofill matching.
//!
//! These exercise [`AppState::find_matches`], [`AppState::get_credential`], and
//! [`AppState::autofill_status`] against a real vault file in a
//! `tempfile::TempDir`, driving time with a [`FakeClock`] so the suite never
//! sleeps and never touches the real per-OS data directory.
//!
//! The phishing non-match cases are the security-critical ones: a wrong match
//! means a password typed into the wrong origin, so they are tested explicitly.

use std::time::Duration;

use app_core::error::AppError;
use app_core::state::AppState;
use app_core::testing::FakeClock;
use app_core::{EntryInput, MatchCandidate};

use assert_matches::assert_matches;
use tempfile::TempDir;

const PW: &str = "correct horse battery staple";
const TIMEOUT: Duration = Duration::from_secs(15 * 60);

/// A fresh `AppState` over a tempdir vault path, plus the clock and the owning
/// `TempDir` (returned so it outlives the test).
struct Harness {
    state: AppState,
    clock: FakeClock,
    _dir: TempDir,
}

fn harness() -> Harness {
    harness_with_timeout(TIMEOUT)
}

fn harness_with_timeout(timeout: Duration) -> Harness {
    let dir = TempDir::new().expect("create tempdir");
    let vault_path = dir.path().join("vault.spk");
    let clock = FakeClock::new();
    let state = AppState::new(vault_path, timeout, Box::new(clock.clone()));
    Harness {
        state,
        clock,
        _dir: dir,
    }
}

/// Create + unlock the vault, then add one entry with the given title/urls and
/// return its id.
fn add(state: &AppState, title: &str, urls: &[&str]) -> String {
    state
        .add_entry(EntryInput {
            title: title.into(),
            username: format!("{title}-user"),
            password: format!("{title}-pass"),
            urls: urls.iter().map(|u| (*u).to_string()).collect(),
            notes: String::new(),
        })
        .expect("add entry")
        .id
}

/// Titles of the candidates `find_matches` returns for `page_url`, sorted for a
/// stable assertion.
fn match_titles(state: &AppState, page_url: &str) -> Vec<String> {
    let mut titles: Vec<String> = state
        .find_matches(page_url)
        .expect("find_matches never errors")
        .into_iter()
        .map(|c| c.title)
        .collect();
    titles.sort();
    titles
}

// ---- Phishing non-matches (the careful part) ---------------------------------

#[test]
fn phishing_lookalikes_never_match() {
    let h = harness();
    h.state.create_vault(PW).unwrap();
    add(&h.state, "google", &["https://accounts.google.com/login"]);

    // Every one of these is a classic phishing trick that naive substring
    // matching would wrongly fill. The registrable domain of each is NOT
    // google.com, so none may match.
    for evil in [
        "https://google.com.evil.ru/login", // suffix smuggling
        "https://evil.com/google.com",      // path lookalike
        "https://google.com@evil.ru/login", // userinfo trick (host = evil.ru)
        "https://googlecom.evil.ru/login",  // label fusion
    ] {
        assert!(
            match_titles(&h.state, evil).is_empty(),
            "phishing url must not match: {evil}"
        );
    }
}

// ---- Subdomain matches -------------------------------------------------------

#[test]
fn subdomains_match_a_bare_registrable_domain() {
    let h = harness();
    h.state.create_vault(PW).unwrap();
    add(&h.state, "google", &["https://google.com"]);

    for page in [
        "https://accounts.google.com/signin",
        "https://mail.google.com/mail/u/0",
        "https://www.google.com/",
        "https://google.com/",
    ] {
        assert_eq!(
            match_titles(&h.state, page),
            vec!["google".to_string()],
            "subdomain should match bare domain: {page}"
        );
    }
}

#[test]
fn bare_page_matches_a_subdomain_entry() {
    // Symmetry: an entry saved on a subdomain fills the bare domain too, since
    // both reduce to the same registrable domain.
    let h = harness();
    h.state.create_vault(PW).unwrap();
    add(&h.state, "google", &["https://accounts.google.com"]);

    assert_eq!(
        match_titles(&h.state, "https://google.com/"),
        vec!["google".to_string()]
    );
    assert_eq!(
        match_titles(&h.state, "https://mail.google.com/"),
        vec!["google".to_string()]
    );
}

// ---- Multi-URL entries -------------------------------------------------------

#[test]
fn entry_with_multiple_urls_matches_any_of_them() {
    let h = harness();
    h.state.create_vault(PW).unwrap();
    add(
        &h.state,
        "multi",
        &["https://example.com", "https://github.com"],
    );

    assert_eq!(
        match_titles(&h.state, "https://www.example.com/"),
        vec!["multi".to_string()]
    );
    assert_eq!(
        match_titles(&h.state, "https://gist.github.com/"),
        vec!["multi".to_string()]
    );
    // A domain in neither URL does not match.
    assert!(match_titles(&h.state, "https://gitlab.com/").is_empty());
}

#[test]
fn multiple_entries_on_the_same_domain_all_match() {
    let h = harness();
    h.state.create_vault(PW).unwrap();
    add(&h.state, "work", &["https://mail.google.com"]);
    add(&h.state, "personal", &["https://accounts.google.com"]);
    add(&h.state, "other", &["https://example.com"]);

    assert_eq!(
        match_titles(&h.state, "https://google.com/"),
        vec!["personal".to_string(), "work".to_string()]
    );
}

// ---- PSL multi-level suffixes ------------------------------------------------

#[test]
fn multi_level_psl_suffix_matches_within_but_not_across() {
    let h = harness();
    h.state.create_vault(PW).unwrap();
    add(&h.state, "ukco", &["https://example.co.uk"]);

    // www.example.co.uk reduces to example.co.uk -> matches.
    assert_eq!(
        match_titles(&h.state, "https://www.example.co.uk/"),
        vec!["ukco".to_string()]
    );
    // foo.co.uk and bar.co.uk are *different* registrable domains under the
    // co.uk multi-level suffix; the saved example.co.uk must not match either.
    assert!(match_titles(&h.state, "https://foo.co.uk/").is_empty());

    // And an entry on foo.co.uk must not match bar.co.uk.
    add(&h.state, "foo", &["https://foo.co.uk"]);
    assert!(match_titles(&h.state, "https://bar.co.uk/").is_empty());
    assert_eq!(
        match_titles(&h.state, "https://sub.foo.co.uk/"),
        vec!["foo".to_string()]
    );
}

// ---- Fail-closed hosts -------------------------------------------------------

#[test]
fn ip_localhost_and_bare_suffix_hosts_fail_closed() {
    let h = harness();
    h.state.create_vault(PW).unwrap();
    // Entries whose own URLs are fail-closed hosts: they can never match,
    // because the page domain (also fail-closed) is None.
    add(&h.state, "ip", &["https://127.0.0.1:8443"]);
    add(&h.state, "local", &["http://localhost:3000"]);
    add(&h.state, "bare", &["http://intranet"]);

    for page in [
        "https://127.0.0.1:8443/login",   // IPv4 literal
        "http://[::1]/login",             // IPv6 literal
        "http://localhost:3000/",         // localhost
        "http://intranet/",               // bare hostname (no public suffix)
        "https://example.invalidtldxyz/", // unknown TLD
    ] {
        assert!(
            match_titles(&h.state, page).is_empty(),
            "fail-closed host must match nothing: {page}"
        );
    }
}

#[test]
fn unparseable_url_yields_no_matches() {
    let h = harness();
    h.state.create_vault(PW).unwrap();
    add(&h.state, "google", &["https://google.com"]);

    for bad in ["not a url", "", "://missing-scheme", "javascript:alert(1)"] {
        assert!(
            match_titles(&h.state, bad).is_empty(),
            "unparseable url must yield no matches: {bad:?}"
        );
    }
}

// ---- Case / trailing dot / scheme insensitivity ------------------------------

#[test]
fn matching_is_case_and_trailing_dot_insensitive() {
    let h = harness();
    h.state.create_vault(PW).unwrap();
    add(&h.state, "google", &["https://Google.COM"]);

    for page in [
        "https://MAIL.GOOGLE.COM/",      // uppercase
        "https://google.com./",          // FQDN trailing dot
        "https://Accounts.Google.Com./", // mixed + trailing dot
    ] {
        assert_eq!(
            match_titles(&h.state, page),
            vec!["google".to_string()],
            "case/dot insensitive: {page}"
        );
    }
}

#[test]
fn matching_is_scheme_agnostic() {
    let h = harness();
    h.state.create_vault(PW).unwrap();
    // Entry stored with http; page is https (and vice-versa) — both match.
    add(&h.state, "google", &["http://google.com"]);

    assert_eq!(
        match_titles(&h.state, "https://google.com/"),
        vec!["google".to_string()]
    );
    assert_eq!(
        match_titles(&h.state, "ftp://files.google.com/"),
        vec!["google".to_string()]
    );
}

// ---- Locked behavior ---------------------------------------------------------

#[test]
fn find_matches_while_locked_returns_empty_not_error() {
    let h = harness();
    h.state.create_vault(PW).unwrap();
    add(&h.state, "google", &["https://google.com"]);
    h.state.lock().unwrap();

    // Locked: fail closed to an empty result, never an error.
    assert!(h
        .state
        .find_matches("https://google.com/")
        .expect("locked find_matches is Ok([])")
        .is_empty());
}

#[test]
fn get_credential_while_locked_errors() {
    let h = harness();
    h.state.create_vault(PW).unwrap();
    let id = add(&h.state, "google", &["https://google.com"]);
    h.state.lock().unwrap();

    assert_matches!(h.state.get_credential(&id), Err(AppError::Locked));
}

#[test]
fn get_credential_unknown_id_errors() {
    let h = harness();
    h.state.create_vault(PW).unwrap();
    let err = h
        .state
        .get_credential("does-not-exist")
        .expect_err("unknown id");
    assert_matches!(err, AppError::EntryNotFound(id) if id == "does-not-exist");
}

#[test]
fn get_credential_returns_full_credential() {
    let h = harness();
    h.state.create_vault(PW).unwrap();
    let id = add(&h.state, "github", &["https://github.com"]);

    let cred = h.state.get_credential(&id).expect("credential");
    assert_eq!(cred.id, id);
    assert_eq!(cred.username, "github-user");
    assert_eq!(cred.password, "github-pass");
}

// ---- No password in matches (serde guarantee) --------------------------------

#[test]
fn match_candidates_carry_no_password() {
    let h = harness();
    h.state.create_vault(PW).unwrap();
    add(&h.state, "github", &["https://github.com", "https://gh.io"]);

    let matches = h.state.find_matches("https://github.com/").unwrap();
    assert_eq!(matches.len(), 1);
    let c: &MatchCandidate = &matches[0];
    assert_eq!(c.title, "github");
    assert_eq!(c.username, "github-user");
    // primary_url is the first URL.
    assert_eq!(c.primary_url.as_deref(), Some("https://github.com"));

    // Serialize and confirm the wire form has no password field at all (this is
    // a compile-time type property, asserted here on the actual JSON too).
    let json = serde_json::to_value(c).unwrap();
    assert!(json.get("password").is_none(), "no password on the wire");
    assert_eq!(json.get("username").unwrap(), "github-user");
}

#[test]
fn match_candidate_primary_url_is_none_when_entry_has_no_urls() {
    // An entry with no URLs can never match a real page (it has no domain), but
    // the projection itself must still produce `primary_url: None` rather than
    // panicking — verified indirectly: it simply never appears in results.
    let h = harness();
    h.state.create_vault(PW).unwrap();
    add(&h.state, "no-url", &[]);
    assert!(h
        .state
        .find_matches("https://google.com/")
        .unwrap()
        .is_empty());
}

// ---- Activity / auto-lock interaction ----------------------------------------

#[test]
fn find_matches_does_not_bump_activity_but_get_credential_does() {
    // Auto-lock at 60s. A find_matches probe at 59s must NOT reset the idle
    // timer (so the vault still locks at 60s), whereas a get_credential at 59s
    // is real use and DOES reset it (so the vault survives to 119s).
    let h = harness_with_timeout(Duration::from_secs(60));
    h.state.create_vault(PW).unwrap();
    let id = add(&h.state, "google", &["https://google.com"]);

    // --- find_matches does NOT defer auto-lock ---
    h.clock.advance(Duration::from_secs(59));
    let _ = h.state.find_matches("https://google.com/").unwrap();
    h.clock.advance(Duration::from_secs(1)); // now 60s of true idle
    assert!(
        h.state.tick_autolock(),
        "find_matches must not reset the idle timer"
    );

    // Re-unlock and verify get_credential DOES defer auto-lock.
    h.state.unlock(PW).unwrap();
    h.clock.advance(Duration::from_secs(59));
    let _ = h.state.get_credential(&id).expect("credential");
    // A full 59s more: still under the timeout *because* the credential fetch
    // reset last_activity.
    h.clock.advance(Duration::from_secs(59));
    assert!(
        !h.state.tick_autolock(),
        "get_credential should have reset the idle timer"
    );
    assert!(h.state.status().unlocked);
}

// ---- autofill_status ---------------------------------------------------------

#[test]
fn autofill_status_reports_unlocked_and_has_vault() {
    let h = harness();

    // No vault yet.
    let s = h.state.autofill_status();
    assert!(!s.unlocked);
    assert!(!s.has_vault);

    // After create: unlocked, vault present.
    h.state.create_vault(PW).unwrap();
    let s = h.state.autofill_status();
    assert!(s.unlocked);
    assert!(s.has_vault);

    // After lock: still has a vault file, but locked.
    h.state.lock().unwrap();
    let s = h.state.autofill_status();
    assert!(!s.unlocked);
    assert!(s.has_vault);
}

#[test]
fn autofill_status_does_not_bump_activity() {
    // A passive status probe must not defer auto-lock.
    let h = harness_with_timeout(Duration::from_secs(60));
    h.state.create_vault(PW).unwrap();

    h.clock.advance(Duration::from_secs(59));
    let _ = h.state.autofill_status();
    h.clock.advance(Duration::from_secs(1));
    assert!(
        h.state.tick_autolock(),
        "autofill_status must not reset the idle timer"
    );
}
