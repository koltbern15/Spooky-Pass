//! Stateless password generator.
//!
//! Generates a random password from the selected character classes using the
//! operating system's CSPRNG ([`OsRng`]). Holds no state between calls.

use rand_core::{OsRng, RngCore};

use crate::error::{Result, VaultError};

/// Lowercase letters, excluding the ambiguous `l`.
const LOWERCASE: &str = "abcdefghijkmnopqrstuvwxyz";
/// The single ambiguous lowercase letter, added back when ambiguity is allowed.
const LOWERCASE_AMBIGUOUS: &str = "l";

/// Uppercase letters, excluding the ambiguous `I` and `O`.
const UPPERCASE: &str = "ABCDEFGHJKLMNPQRSTUVWXYZ";
/// The ambiguous uppercase letters, added back when ambiguity is allowed.
const UPPERCASE_AMBIGUOUS: &str = "IO";

/// Digits, excluding the ambiguous `0` and `1`.
const DIGITS: &str = "23456789";
/// The ambiguous digits, added back when ambiguity is allowed.
const DIGITS_AMBIGUOUS: &str = "01";

/// Symbols. None of these are treated as ambiguous.
const SYMBOLS: &str = "!@#$%^&*()-_=+[]{};:,.<>?";

/// Options controlling password generation.
///
/// At least one character class must be enabled and `length` must be non-zero,
/// or [`crate::generate_password`] returns [`VaultError::InvalidPasswordOptions`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PasswordOptions {
    /// Number of characters to generate.
    pub length: usize,
    /// Include lowercase letters.
    pub lowercase: bool,
    /// Include uppercase letters.
    pub uppercase: bool,
    /// Include digits.
    pub digits: bool,
    /// Include symbols.
    pub symbols: bool,
    /// Exclude visually ambiguous characters (`l`, `I`, `O`, `0`, `1`).
    pub exclude_ambiguous: bool,
}

impl Default for PasswordOptions {
    fn default() -> Self {
        PasswordOptions {
            length: 20,
            lowercase: true,
            uppercase: true,
            digits: true,
            symbols: true,
            exclude_ambiguous: true,
        }
    }
}

/// Build the full character pool for the given options.
fn build_charset(opts: &PasswordOptions) -> Vec<char> {
    let mut charset: Vec<char> = Vec::new();
    if opts.lowercase {
        charset.extend(LOWERCASE.chars());
        if !opts.exclude_ambiguous {
            charset.extend(LOWERCASE_AMBIGUOUS.chars());
        }
    }
    if opts.uppercase {
        charset.extend(UPPERCASE.chars());
        if !opts.exclude_ambiguous {
            charset.extend(UPPERCASE_AMBIGUOUS.chars());
        }
    }
    if opts.digits {
        charset.extend(DIGITS.chars());
        if !opts.exclude_ambiguous {
            charset.extend(DIGITS_AMBIGUOUS.chars());
        }
    }
    if opts.symbols {
        charset.extend(SYMBOLS.chars());
    }
    charset
}

/// Draw a uniform `usize` in `0..bound` from `OsRng` using rejection sampling
/// to avoid modulo bias. `bound` must be non-zero.
fn uniform_index(bound: usize) -> usize {
    debug_assert!(bound > 0);
    // Largest multiple of `bound` that fits in u64; values at or above the
    // threshold are rejected to keep the distribution uniform.
    let bound = bound as u64;
    let zone = u64::MAX - (u64::MAX % bound);
    loop {
        let x = OsRng.next_u64();
        if x < zone {
            return (x % bound) as usize;
        }
    }
}

/// Generate a random password according to `opts`.
///
/// Uses [`OsRng`] (the OS CSPRNG) and unbiased rejection sampling over the
/// selected character pool.
///
/// # Errors
///
/// Returns [`VaultError::InvalidPasswordOptions`] if `length` is zero or no
/// character class is selected.
pub fn generate_password(opts: &PasswordOptions) -> Result<String> {
    if opts.length == 0 {
        return Err(VaultError::InvalidPasswordOptions);
    }

    let charset = build_charset(opts);
    if charset.is_empty() {
        return Err(VaultError::InvalidPasswordOptions);
    }

    let mut out = String::with_capacity(opts.length);
    for _ in 0..opts.length {
        let idx = uniform_index(charset.len());
        out.push(charset[idx]);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn only(class: fn(&mut PasswordOptions)) -> PasswordOptions {
        let mut opts = PasswordOptions {
            length: 64,
            lowercase: false,
            uppercase: false,
            digits: false,
            symbols: false,
            exclude_ambiguous: false,
        };
        class(&mut opts);
        opts
    }

    #[test]
    fn generated_password_honors_length() {
        let opts = PasswordOptions {
            length: 37,
            ..PasswordOptions::default()
        };
        let pw = generate_password(&opts).unwrap();
        assert_eq!(pw.chars().count(), 37);
    }

    #[test]
    fn generated_password_lowercase_only() {
        let opts = only(|o| o.lowercase = true);
        let pw = generate_password(&opts).unwrap();
        assert!(pw.chars().all(|c| c.is_ascii_lowercase()), "got: {pw}");
    }

    #[test]
    fn generated_password_uppercase_only() {
        let opts = only(|o| o.uppercase = true);
        let pw = generate_password(&opts).unwrap();
        assert!(pw.chars().all(|c| c.is_ascii_uppercase()), "got: {pw}");
    }

    #[test]
    fn generated_password_digits_only() {
        let opts = only(|o| o.digits = true);
        let pw = generate_password(&opts).unwrap();
        assert!(pw.chars().all(|c| c.is_ascii_digit()), "got: {pw}");
    }

    #[test]
    fn generated_password_symbols_only() {
        let opts = only(|o| o.symbols = true);
        let pw = generate_password(&opts).unwrap();
        assert!(pw.chars().all(|c| SYMBOLS.contains(c)), "got: {pw}");
    }

    #[test]
    fn generated_password_uses_all_selected_classes() {
        // With all classes on and a long length, every class should appear with
        // overwhelming probability. Use a generous length to keep flakiness
        // negligible.
        let opts = PasswordOptions {
            length: 400,
            lowercase: true,
            uppercase: true,
            digits: true,
            symbols: true,
            exclude_ambiguous: true,
        };
        let pw = generate_password(&opts).unwrap();
        assert!(pw.chars().any(|c| c.is_ascii_lowercase()), "no lowercase");
        assert!(pw.chars().any(|c| c.is_ascii_uppercase()), "no uppercase");
        assert!(pw.chars().any(|c| c.is_ascii_digit()), "no digit");
        assert!(pw.chars().any(|c| SYMBOLS.contains(c)), "no symbol");
    }

    #[test]
    fn generator_excludes_ambiguous_when_requested() {
        let opts = PasswordOptions {
            length: 500,
            lowercase: true,
            uppercase: true,
            digits: true,
            symbols: false,
            exclude_ambiguous: true,
        };
        let pw = generate_password(&opts).unwrap();
        for ambiguous in ['l', 'I', 'O', '0', '1'] {
            assert!(
                !pw.contains(ambiguous),
                "ambiguous char {ambiguous:?} present in {pw}"
            );
        }
    }

    #[test]
    fn generator_rejects_zero_length() {
        let opts = PasswordOptions {
            length: 0,
            ..PasswordOptions::default()
        };
        assert_matches::assert_matches!(
            generate_password(&opts),
            Err(VaultError::InvalidPasswordOptions)
        );
    }

    #[test]
    fn generator_rejects_no_charset() {
        let opts = PasswordOptions {
            length: 16,
            lowercase: false,
            uppercase: false,
            digits: false,
            symbols: false,
            exclude_ambiguous: false,
        };
        assert_matches::assert_matches!(
            generate_password(&opts),
            Err(VaultError::InvalidPasswordOptions)
        );
    }

    #[test]
    fn generated_passwords_are_not_constant() {
        let opts = PasswordOptions::default();
        let a = generate_password(&opts).unwrap();
        let b = generate_password(&opts).unwrap();
        assert_ne!(a, b);
    }
}
