//! The pages visitors see when a site cannot answer (T06.13): a suspended
//! domain, a suspended account, an address nobody hosts, and a site whose web
//! server does not answer. The installation ships plain defaults; an
//! administrator may replace each.

/// Largest page, in bytes.
pub const MAX_PAGE_BYTES: usize = 16 * 1024;

/// What a page is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    /// The domain was suspended by an administrator.
    SuspendedDomain,
    /// The whole account was suspended.
    SuspendedAccount,
    /// No site is set up for the address.
    NotFound,
    /// The site's web server does not answer (502, 503 or 504).
    BadGateway,
}

/// Every kind, in the order they are shown.
pub const KINDS: [Kind; 4] = [
    Kind::SuspendedDomain,
    Kind::SuspendedAccount,
    Kind::NotFound,
    Kind::BadGateway,
];

impl Kind {
    /// The name used in the API and on the wire.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SuspendedDomain => "suspended_domain",
            Self::SuspendedAccount => "suspended_account",
            Self::NotFound => "not_found",
            Self::BadGateway => "bad_gateway",
        }
    }

    /// Reads a name.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        KINDS.into_iter().find(|k| k.as_str() == text)
    }

    /// The page the installation ships.
    #[must_use]
    pub fn default_page(self) -> String {
        let (title, heading, text) = match self {
            Self::SuspendedDomain => (
                "Site suspended",
                "This site is suspended",
                "The owner of this address has been asked to contact their hosting provider.",
            ),
            Self::SuspendedAccount => (
                "Account suspended",
                "This account is suspended",
                "The sites of this account are offline. The owner has been asked to contact their hosting provider.",
            ),
            Self::NotFound => (
                "Not found",
                "Nothing here",
                "No site is set up for this address.",
            ),
            Self::BadGateway => (
                "Temporarily unavailable",
                "This site is not answering",
                "Please try again in a moment.",
            ),
        };
        format!(
            "<!DOCTYPE html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><title>{title}</title></head><body style=\"font-family:system-ui,sans-serif;display:grid;place-items:center;min-height:100vh;margin:0;background:#f5f6f8;color:#1d2330\"><main style=\"text-align:center;padding:2rem\"><h1>{heading}</h1><p>{text}</p></main></body></html>"
        )
    }
}

/// Checks a page an administrator wrote.
///
/// # Errors
/// A code and a message.
pub fn check(body: &str) -> Result<(), (&'static str, &'static str)> {
    if body.trim().is_empty() {
        return Err(("page_empty", "The page is empty."));
    }
    if body.len() > MAX_PAGE_BYTES {
        return Err(("page_too_large", "A page is at most 16 KiB."));
    }
    if body
        .chars()
        .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
    {
        return Err(("page_characters", "The page holds control characters."));
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn kinds_have_names_and_default_pages_that_pass_the_checks() {
        for kind in KINDS {
            assert_eq!(Kind::parse(kind.as_str()), Some(kind));
            let page = kind.default_page();
            assert!(page.contains("<h1>"), "{page}");
            assert!(check(&page).is_ok());
        }
        assert_eq!(Kind::parse("other"), None);
        assert_ne!(
            Kind::SuspendedDomain.default_page(),
            Kind::SuspendedAccount.default_page()
        );
    }

    #[test]
    fn pages_must_be_text_of_a_sane_size() {
        assert_eq!(check("").unwrap_err().0, "page_empty");
        assert_eq!(check("  \n").unwrap_err().0, "page_empty");
        assert_eq!(
            check(&"x".repeat(MAX_PAGE_BYTES + 1)).unwrap_err().0,
            "page_too_large"
        );
        assert_eq!(check("a\u{0}b").unwrap_err().0, "page_characters");
        assert!(check("<p>hello\r\n\t</p>").is_ok());
    }
}
