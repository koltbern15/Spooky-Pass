//! The frozen autofill message contract.
//!
//! These types are the **single source of truth** for the two wire hops:
//! extension ↔ native-host (Chrome native messaging) and native-host ↔ Core
//! (local-socket IPC). They are mirrored field-for-field, camelCase, by
//! `extension/src/protocol.ts` — keep the two in lockstep.
//!
//! ## Password-crossing invariant
//!
//! A password appears in **exactly one** message — [`IpcResponse::Credential`] —
//! which the Core produces **only** in response to an [`IpcRequest::GetCredential`]
//! that the content script sends from a real user-gesture click. Match results
//! ([`MatchCandidate`]) carry no password *by type*, so "no secret in matches"
//! is a compile-time guarantee, not a runtime filter.

use serde::{Deserialize, Serialize};

/// A credential candidate for a page — everything the affordance/list needs,
/// and **never** a password.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchCandidate {
    /// Stable entry id (used to later request the credential).
    pub id: String,
    /// Entry title.
    pub title: String,
    /// Login username.
    pub username: String,
    /// First associated URL, if any.
    pub primary_url: Option<String>,
}

/// A full credential — the **only** payload that carries a password, returned
/// only on an explicit, user-initiated [`IpcRequest::GetCredential`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialDto {
    /// Stable entry id.
    pub id: String,
    /// Login username.
    pub username: String,
    /// Login password.
    pub password: String,
}

/// Whether the Core is reachable/unlocked — drives the extension affordance
/// (offer to fill vs. offer to unlock).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutofillStatusDto {
    /// Whether a vault is currently unlocked in the Core.
    pub unlocked: bool,
    /// Whether a vault file exists on disk.
    pub has_vault: bool,
}

/// A request from the extension (relayed by the native-host to the Core).
///
/// Serializes internally-tagged as `{ "type": "...", ...fields }`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum IpcRequest {
    /// Ask whether the Core is unlocked (no vault data crosses).
    GetStatus,
    /// Ask for credential candidates matching `url` (the top-level page origin).
    /// Results never contain passwords.
    GetMatches {
        /// The top-level page URL (the content script guarantees this is never
        /// an iframe URL).
        url: String,
    },
    /// Ask for the full credential of entry `id` — issued only on a user click.
    GetCredential {
        /// The entry id from a prior [`MatchCandidate`].
        id: String,
    },
    /// Offer to save a newly-entered login (save-on-login; minimal in v1).
    SaveLogin {
        /// The page URL the login was entered on.
        url: String,
        /// The entered username.
        username: String,
        /// The entered password.
        password: String,
        /// An optional title; the Core may derive one from the URL if absent.
        title: Option<String>,
    },
}

/// A response from the Core (relayed back to the extension).
///
/// Serializes internally-tagged as `{ "type": "...", ...fields }`. The
/// newtype variants flatten their inner struct's fields alongside `type`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum IpcResponse {
    /// Reply to [`IpcRequest::GetStatus`].
    Status(AutofillStatusDto),
    /// Reply to [`IpcRequest::GetMatches`] — password-free candidates (possibly
    /// empty, including when the vault is locked).
    Matches {
        /// The matching candidates.
        matches: Vec<MatchCandidate>,
    },
    /// Reply to [`IpcRequest::GetCredential`] — the only password-bearing reply.
    Credential(CredentialDto),
    /// Reply to a successful [`IpcRequest::SaveLogin`].
    Saved {
        /// The id of the saved entry.
        id: String,
    },
    /// Any failure. `kind` mirrors `AppError`'s kinds, plus host-level kinds
    /// such as `coreUnavailable` and `protocol`.
    Error {
        /// A stable machine-readable error kind.
        kind: String,
        /// An optional human-readable message.
        #[serde(skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
}

/// Transport envelope on the native-messaging leg: the extension's background
/// worker tags each request with a `req_id` so it can correlate the asynchronous
/// response. The native-host echoes the same `req_id` back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestEnvelope {
    /// Client-generated correlation id.
    pub req_id: u64,
    /// The wrapped request.
    pub request: IpcRequest,
}

/// The response counterpart of [`RequestEnvelope`], carrying the echoed
/// `req_id`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResponseEnvelope {
    /// The correlation id echoed from the request.
    pub req_id: u64,
    /// The wrapped response.
    pub response: IpcResponse,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn match_candidate_has_no_password_field() {
        let json = serde_json::to_value(MatchCandidate {
            id: "1".into(),
            title: "GitHub".into(),
            username: "octocat".into(),
            primary_url: Some("https://github.com".into()),
        })
        .unwrap();
        assert!(json.get("password").is_none());
        assert_eq!(json.get("primaryUrl").unwrap(), "https://github.com");
    }

    #[test]
    fn request_is_internally_tagged_camel_case() {
        let json = serde_json::to_value(IpcRequest::GetMatches {
            url: "https://x.com".into(),
        })
        .unwrap();
        assert_eq!(json.get("type").unwrap(), "getMatches");
        assert_eq!(json.get("url").unwrap(), "https://x.com");
    }

    #[test]
    fn status_response_flattens_inner_struct() {
        let json = serde_json::to_value(IpcResponse::Status(AutofillStatusDto {
            unlocked: true,
            has_vault: true,
        }))
        .unwrap();
        assert_eq!(json.get("type").unwrap(), "status");
        assert_eq!(json.get("unlocked").unwrap(), true);
        assert_eq!(json.get("hasVault").unwrap(), true);
    }

    #[test]
    fn envelope_round_trips() {
        let env = RequestEnvelope {
            req_id: 42,
            request: IpcRequest::GetCredential { id: "abc".into() },
        };
        let bytes = serde_json::to_vec(&env).unwrap();
        let back: RequestEnvelope = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(env, back);
    }

    #[test]
    fn error_omits_absent_message() {
        let json = serde_json::to_value(IpcResponse::Error {
            kind: "locked".into(),
            message: None,
        })
        .unwrap();
        assert_eq!(json.get("type").unwrap(), "error");
        assert_eq!(json.get("kind").unwrap(), "locked");
        assert!(json.get("message").is_none());
    }
}
