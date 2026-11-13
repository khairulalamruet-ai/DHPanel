//! The wire format: one JSON line each way.

use serde::{Deserialize, Serialize};

/// Largest request line accepted, in bytes.
pub const MAX_REQUEST_BYTES: usize = 64 * 1024;

/// Everything the helper can be asked to do. Adding a variant here is the only
/// way to add a privileged action, and each one is reviewed on its own.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Action {
    /// Does nothing; proves that the channel and the permission check work.
    Noop,
}

impl Action {
    /// The action's wire name (safe to log).
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            Self::Noop => "noop",
        }
    }
}

/// A request from the agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    /// Chosen by the caller, echoed in the answer and the log (1-64 characters
    /// of `A-Z a-z 0-9 . _ : -`).
    pub id: String,
    /// What to do.
    pub action: Action,
}

/// Why a request was refused or failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    /// Not JSON, not a request, too long, or a bad id.
    InvalidRequest,
    /// The action is not on the list.
    UnknownAction,
    /// The action is known but its arguments are not acceptable.
    InvalidArguments,
    /// The caller is not the agent.
    Forbidden,
    /// The action was attempted and failed.
    Failed,
}

/// The helper's answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Response {
    /// The request's id (empty when the request could not be read).
    pub id: String,
    /// Whether the action was carried out.
    pub ok: bool,
    /// Present when `ok` is false.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<ResponseError>,
}

/// Failure detail. Messages are fixed texts, never echoes of the input.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResponseError {
    /// Machine-readable reason.
    pub code: ErrorCode,
    /// Human-readable reason.
    pub message: String,
}

impl Response {
    /// Success.
    #[must_use]
    pub fn done(id: &str) -> Self {
        Self {
            id: id.to_owned(),
            ok: true,
            error: None,
        }
    }

    /// Refusal or failure.
    #[must_use]
    pub fn refused(id: &str, code: ErrorCode, message: &str) -> Self {
        Self {
            id: id.to_owned(),
            ok: false,
            error: Some(ResponseError {
                code,
                message: message.to_owned(),
            }),
        }
    }
}

fn valid_id(id: &str) -> bool {
    (1..=64).contains(&id.len())
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | ':' | '-'))
}

/// The keys an action object may have (including `type`), or `None` for an
/// action that is not on the list. Must be extended together with [`Action`].
fn allowed_fields(action: &str) -> Option<&'static [&'static str]> {
    match action {
        "noop" => Some(&["type"]),
        _ => None,
    }
}

/// Why a line is not a request, with what may be logged about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rejected {
    /// The answer to send.
    pub response: Response,
    /// The action name the caller asked for, when it could be read and is
    /// harmless to log (letters, digits, `_`; at most 64 characters).
    pub asked_for: Option<String>,
}

/// Reads one request line.
///
/// # Errors
/// [`Rejected`] with the answer to send back.
pub fn parse_request(line: &[u8]) -> Result<Request, Rejected> {
    let reject = |id: &str, code, message: &str, asked_for: Option<String>| Rejected {
        response: Response::refused(id, code, message),
        asked_for,
    };
    if line.len() > MAX_REQUEST_BYTES {
        return Err(reject(
            "",
            ErrorCode::InvalidRequest,
            "request too large",
            None,
        ));
    }
    // Look at the shape first, so that an unknown action can be told apart
    // from garbage and named in the log.
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(line) else {
        return Err(reject("", ErrorCode::InvalidRequest, "not JSON", None));
    };
    let id = value
        .get("id")
        .and_then(|v| v.as_str())
        .filter(|id| valid_id(id))
        .unwrap_or_default()
        .to_owned();
    if id.is_empty() {
        return Err(reject(
            "",
            ErrorCode::InvalidRequest,
            "missing or invalid id",
            None,
        ));
    }
    let asked_for = value
        .get("action")
        .and_then(|a| a.get("type"))
        .and_then(|t| t.as_str())
        .filter(|t| {
            (1..=64).contains(&t.len()) && t.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        })
        .map(str::to_owned);
    // serde's `deny_unknown_fields` does not cover every enum shape (a variant
    // without fields accepts extra keys), so the allowed keys are checked here.
    let extra_keys = match (&asked_for, value.get("action").and_then(|a| a.as_object())) {
        (Some(name), Some(fields)) => allowed_fields(name)
            .is_some_and(|allowed| fields.keys().any(|k| !allowed.contains(&k.as_str()))),
        _ => false,
    };
    if extra_keys {
        return Err(reject(
            &id,
            ErrorCode::InvalidArguments,
            "invalid arguments for this action",
            asked_for,
        ));
    }
    match serde_json::from_value::<Request>(value) {
        Ok(request) => Ok(request),
        Err(_) => match &asked_for {
            Some(name) if allowed_fields(name).is_some() => Err(reject(
                &id,
                ErrorCode::InvalidArguments,
                "invalid arguments for this action",
                asked_for,
            )),
            Some(_) => Err(reject(
                &id,
                ErrorCode::UnknownAction,
                "this action is not on the allow-list",
                asked_for,
            )),
            None => Err(reject(
                &id,
                ErrorCode::InvalidRequest,
                "not a request",
                None,
            )),
        },
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn a_noop_request_round_trips() {
        let line = br#"{"id":"req-1","action":{"type":"noop"}}"#;
        let request = parse_request(line).unwrap();
        assert_eq!(
            request,
            Request {
                id: "req-1".into(),
                action: Action::Noop
            }
        );
        assert_eq!(serde_json::to_string(&request).unwrap().as_bytes(), line);
        assert_eq!(
            serde_json::to_string(&Response::done("req-1")).unwrap(),
            r#"{"id":"req-1","ok":true}"#
        );
    }

    #[test]
    fn unknown_actions_are_refused_and_named() {
        for (line, name) in [
            (
                r#"{"id":"a","action":{"type":"run_command","cmd":"id"}}"#,
                "run_command",
            ),
            (r#"{"id":"a","action":{"type":"shell"}}"#, "shell"),
            (r#"{"id":"a","action":{"type":"Noop"}}"#, "Noop"),
        ] {
            let rejected = parse_request(line.as_bytes()).unwrap_err();
            assert_eq!(
                rejected.response.error.as_ref().unwrap().code,
                ErrorCode::UnknownAction,
                "{line}"
            );
            assert_eq!(rejected.response.id, "a");
            assert_eq!(rejected.asked_for.as_deref(), Some(name));
            assert!(!rejected.response.ok);
        }
    }

    #[test]
    fn extra_fields_and_bad_shapes_are_refused() {
        let code = |line: &str| {
            parse_request(line.as_bytes())
                .unwrap_err()
                .response
                .error
                .unwrap()
                .code
        };
        // A known action with something extra is not silently accepted.
        assert_eq!(
            code(r#"{"id":"a","action":{"type":"noop","cmd":"id"}}"#),
            ErrorCode::InvalidArguments
        );
        assert_eq!(
            code(r#"{"id":"a","action":{"type":"noop"},"sudo":true}"#),
            ErrorCode::InvalidArguments
        );
        assert_eq!(code(r#"{"id":"a"}"#), ErrorCode::InvalidRequest);
        assert_eq!(
            code(r#"{"id":"a","action":"noop"}"#),
            ErrorCode::InvalidRequest
        );
        assert_eq!(
            code(r#"{"action":{"type":"noop"}}"#),
            ErrorCode::InvalidRequest
        );
        assert_eq!(
            code(r#"{"id":"","action":{"type":"noop"}}"#),
            ErrorCode::InvalidRequest
        );
        assert_eq!(
            code(r#"{"id":"a b","action":{"type":"noop"}}"#),
            ErrorCode::InvalidRequest
        );
        assert_eq!(
            code(r#"{"id":7,"action":{"type":"noop"}}"#),
            ErrorCode::InvalidRequest
        );
        assert_eq!(code("not json"), ErrorCode::InvalidRequest);
        assert_eq!(code(""), ErrorCode::InvalidRequest);
        assert_eq!(code("[]"), ErrorCode::InvalidRequest);
        let huge = format!(
            r#"{{"id":"a","action":{{"type":"noop"}},"pad":"{}"}}"#,
            "x".repeat(MAX_REQUEST_BYTES)
        );
        assert_eq!(code(&huge), ErrorCode::InvalidRequest);
    }

    #[test]
    fn hostile_action_names_are_not_passed_to_the_log() {
        for line in [
            r#"{"id":"a","action":{"type":"x\nFAKE LOG LINE"}}"#.to_owned(),
            r#"{"id":"a","action":{"type":"a b"}}"#.to_owned(),
            format!(r#"{{"id":"a","action":{{"type":"{}"}}}}"#, "x".repeat(65)),
        ] {
            let rejected = parse_request(line.as_bytes()).unwrap_err();
            assert_eq!(rejected.asked_for, None, "{line}");
        }
    }
}
