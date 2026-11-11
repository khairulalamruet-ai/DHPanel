//! Password strength policy.
//!
//! Rules (all violations are reported together so the UI can show them at once):
//! * length between [`PasswordPolicy::min_chars`] and [`PasswordPolicy::max_chars`]
//!   (Unicode scalar values, so Bangla passwords count correctly);
//! * must not contain the username or the email's local part;
//! * zxcvbn score at least [`PasswordPolicy::min_score`] (0–4). zxcvbn ships
//!   frequency lists of common passwords, names and words and also receives
//!   the account's own identifiers as user inputs.

use secrecy::{ExposeSecret, SecretString};

/// A single reason a password was rejected. Codes are stable for the UI/i18n.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Violation {
    /// Fewer characters than the minimum.
    TooShort {
        /// Required minimum.
        min: usize,
    },
    /// More characters than the maximum.
    TooLong {
        /// Allowed maximum.
        max: usize,
    },
    /// Contains the username or email local part.
    ContainsPersonalInfo,
    /// zxcvbn considers it guessable.
    TooWeak {
        /// zxcvbn score obtained (0–4).
        score: u8,
        /// Required minimum score.
        min: u8,
    },
}

impl Violation {
    /// Stable machine code (`password_too_short`, ...).
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::TooShort { .. } => "password_too_short",
            Self::TooLong { .. } => "password_too_long",
            Self::ContainsPersonalInfo => "password_contains_personal_info",
            Self::TooWeak { .. } => "password_too_weak",
        }
    }

    /// English message; the UI translates by [`Self::code`].
    #[must_use]
    pub fn message(&self) -> String {
        match self {
            Self::TooShort { min } => format!("Use at least {min} characters."),
            Self::TooLong { max } => format!("Use at most {max} characters."),
            Self::ContainsPersonalInfo => "Do not include your username or email.".to_owned(),
            Self::TooWeak { .. } => {
                "This password is too easy to guess. Use a longer passphrase of unrelated words."
                    .to_owned()
            }
        }
    }
}

/// Password rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PasswordPolicy {
    /// Minimum length in characters.
    pub min_chars: usize,
    /// Maximum length in characters (bounds hashing cost).
    pub max_chars: usize,
    /// Minimum zxcvbn score (0–4).
    pub min_score: u8,
}

impl Default for PasswordPolicy {
    fn default() -> Self {
        Self {
            min_chars: 12,
            max_chars: 128,
            min_score: 3,
        }
    }
}

impl PasswordPolicy {
    /// Checks a candidate password for an account.
    ///
    /// # Errors
    /// Every violated rule, in a stable order.
    pub fn check(
        &self,
        password: &SecretString,
        username: &str,
        email: &str,
    ) -> Result<(), Vec<Violation>> {
        let pw = password.expose_secret();
        let chars = pw.chars().count();
        let mut violations = Vec::new();

        if chars < self.min_chars {
            violations.push(Violation::TooShort {
                min: self.min_chars,
            });
        }
        if chars > self.max_chars {
            violations.push(Violation::TooLong {
                max: self.max_chars,
            });
            // Do not run zxcvbn on oversized input.
            return Err(violations);
        }

        let lower = pw.to_lowercase();
        let local_part = email.split('@').next().unwrap_or_default().to_lowercase();
        let personal = [username.to_lowercase(), local_part];
        if personal
            .iter()
            .any(|p| p.chars().count() >= 3 && lower.contains(p.as_str()))
        {
            violations.push(Violation::ContainsPersonalInfo);
        }

        if !pw.is_empty() {
            let inputs: Vec<&str> = personal.iter().map(String::as_str).collect();
            let score = u8::from(zxcvbn::zxcvbn(pw, &inputs).score());
            if score < self.min_score {
                violations.push(Violation::TooWeak {
                    score,
                    min: self.min_score,
                });
            }
        }

        if violations.is_empty() {
            Ok(())
        } else {
            Err(violations)
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    fn check(pw: &str) -> Result<(), Vec<Violation>> {
        PasswordPolicy::default().check(
            &SecretString::from(pw.to_owned()),
            "khairul",
            "khairul@example.com",
        )
    }

    fn codes(pw: &str) -> Vec<&'static str> {
        check(pw)
            .err()
            .unwrap_or_default()
            .iter()
            .map(Violation::code)
            .collect()
    }

    #[test]
    fn strong_passphrases_pass() {
        for pw in [
            "correct-horse-battery-staple",
            "Tiger!Lamp_Orbit7Quiet",
            "নদী-আকাশ-পাহাড়-সবুজ-2026",
        ] {
            assert!(check(pw).is_ok(), "{pw}: {:?}", check(pw));
        }
    }

    #[test]
    fn common_passwords_are_too_weak_even_when_long_enough() {
        for pw in [
            "password1234",
            "qwertyuiop12",
            "123456789012",
            "iloveyou2026",
        ] {
            assert!(
                codes(pw).contains(&"password_too_weak"),
                "{pw}: {:?}",
                codes(pw)
            );
        }
    }

    #[test]
    fn short_password_reports_length() {
        let c = codes("Ab1!xyz");
        assert!(c.contains(&"password_too_short"), "{c:?}");
    }

    #[test]
    fn oversized_password_is_rejected_without_scoring() {
        let long = "a".repeat(129);
        assert_eq!(codes(&long), vec!["password_too_long"]);
    }

    #[test]
    fn username_or_email_inside_password_is_rejected() {
        assert!(codes("Khairul-Strong-Pass-9").contains(&"password_contains_personal_info"));
    }

    #[test]
    fn length_is_counted_in_characters_not_bytes() {
        // 7 Unicode scalar values but 21 bytes in UTF-8: still too short.
        let pw = "আকাশনদী";
        assert!(pw.chars().count() < 12 && pw.len() > 12);
        assert!(codes(pw).contains(&"password_too_short"));
    }

    #[test]
    fn violations_have_stable_codes_and_messages() {
        let v = Violation::TooShort { min: 12 };
        assert_eq!(v.code(), "password_too_short");
        assert!(v.message().contains("12"));
    }
}
