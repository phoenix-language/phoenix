//! Human-readable byte size parsing for VM heap limits and project config.
//!
//! [`parse_byte_size`] converts strings such as `"64mb"` or `"67108864"` into a byte count
//! (`usize`). It is re-exported at the [`crate`] root for CLI and embedder use.
//!
//! ## Consumers
//!
//! - [`ProjectConfig`] `[vm] heap_cap` field in `phoenix.toml`
//! - `phx run --heap-cap` — parsed in `phx-cli`
//!
//! See the VM linear memory design doc for precedence between flag, manifest, and built-in
//! defaults.
//!
//! ## Accepted formats
//!
//! Input is trimmed. The numeric portion must be a positive unsigned integer (no decimals).
//! Suffixes use **binary (IEC) multipliers**; matching is case-insensitive.
//!
//! | Suffix | Multiplier |
//! |--------|------------|
//! | *(none)* or `b` | 1 |
//! | `k`, `kb`, `kib` | 1024 |
//! | `m`, `mb`, `mib` | 1024² |
//! | `g`, `gb`, `gib` | 1024³ |
//!
//! Longer suffixes (`kib`, `mib`, `gib`, `kb`, `mb`, `gb`) are matched before single-letter
//! forms so `"64mib"` is unambiguous.

/// Error parsing a human-readable byte size string.
///
/// Returned by [`parse_byte_size`]. Implements [`Display`](std::fmt::Display) with
/// user-facing messages suitable for CLI and manifest diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ByteSizeError {
    /// Input was empty or whitespace only.
    Empty,
    /// Value must be greater than zero.
    Zero,
    /// Unrecognized suffix, trailing junk, or non-digit characters in the numeric portion.
    InvalidSuffix {
        /// Original input token that could not be parsed.
        detail: String,
    },
    /// Numeric portion is not a valid unsigned integer (e.g. contains `.`).
    InvalidNumber {
        /// Original input token that could not be parsed.
        detail: String,
    },
    /// The product of base × multiplier exceeded [`usize::MAX`].
    Overflow,
}

impl std::fmt::Display for ByteSizeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => write!(f, "byte size cannot be empty"),
            Self::Zero => write!(f, "byte size must be greater than zero"),
            Self::InvalidSuffix { detail } => write!(f, "invalid byte size `{detail}`"),
            Self::InvalidNumber { detail } => {
                write!(f, "invalid byte size number `{detail}`")
            }
            Self::Overflow => write!(f, "byte size overflow"),
        }
    }
}

impl std::error::Error for ByteSizeError {}

/// Parses a byte count from a bare integer or suffixed string.
///
/// Accepts forms such as `"67108864"`, `"64mb"`, `"512k"`, and `"1gib"`. See the
/// [module-level suffix table](self#accepted-formats) for the full set of multipliers.
///
/// # Errors
///
/// Returns [`ByteSizeError::Empty`] when `s` is empty or whitespace only.
///
/// Returns [`ByteSizeError::Zero`] when the numeric portion parses to zero.
///
/// Returns [`ByteSizeError::InvalidSuffix`] for unrecognized tokens, trailing junk
/// (e.g. `"64mbx"`), or non-digit characters outside a valid suffix.
///
/// Returns [`ByteSizeError::InvalidNumber`] when the numeric portion contains a decimal
/// point or otherwise fails unsigned integer parsing.
///
/// Returns [`ByteSizeError::Overflow`] when base × multiplier exceeds [`usize::MAX`].
///
/// # Panics
///
/// Never panics on any input.
///
/// # Examples
///
/// ```
/// use phx_compiler::parse_byte_size;
///
/// assert_eq!(parse_byte_size("67108864").unwrap(), 67_108_864);
/// assert_eq!(parse_byte_size("64mb").unwrap(), 64 * 1024 * 1024);
/// assert_eq!(parse_byte_size("512k").unwrap(), 512 * 1024);
/// assert!(parse_byte_size("").is_err());
/// assert!(parse_byte_size("0mb").is_err());
/// ```
pub fn parse_byte_size(s: &str) -> Result<usize, ByteSizeError> {
    let s = s.trim();
    if s.is_empty() {
        return Err(ByteSizeError::Empty);
    }

    let lower = s.to_ascii_lowercase();
    let (num_str, multiplier) = if let Some(rest) = lower.strip_suffix("gib") {
        (rest, 1024_u64 * 1024 * 1024)
    } else if let Some(rest) = lower.strip_suffix("mib") {
        (rest, 1024_u64 * 1024)
    } else if let Some(rest) = lower.strip_suffix("kib") {
        (rest, 1024_u64)
    } else if let Some(rest) = lower.strip_suffix("gb") {
        (rest, 1024_u64 * 1024 * 1024)
    } else if let Some(rest) = lower.strip_suffix("mb") {
        (rest, 1024_u64 * 1024)
    } else if let Some(rest) = lower.strip_suffix("kb") {
        (rest, 1024_u64)
    } else if let Some(rest) = lower.strip_suffix('g') {
        (rest, 1024_u64 * 1024 * 1024)
    } else if let Some(rest) = lower.strip_suffix('m') {
        (rest, 1024_u64 * 1024)
    } else if let Some(rest) = lower.strip_suffix('k') {
        (rest, 1024_u64)
    } else if let Some(rest) = lower.strip_suffix('b') {
        if rest.is_empty() {
            return Err(ByteSizeError::InvalidSuffix {
                detail: s.to_owned(),
            });
        }
        (rest, 1)
    } else {
        (lower.as_str(), 1)
    };

    let num_str = num_str.trim();
    if num_str.is_empty() {
        return Err(ByteSizeError::InvalidSuffix {
            detail: s.to_owned(),
        });
    }
    if num_str.contains('.') {
        return Err(ByteSizeError::InvalidNumber {
            detail: s.to_owned(),
        });
    }
    if !num_str.chars().all(|c| c.is_ascii_digit()) {
        return Err(ByteSizeError::InvalidSuffix {
            detail: s.to_owned(),
        });
    }

    let base: u64 = num_str.parse().map_err(|_| ByteSizeError::InvalidNumber {
        detail: s.to_owned(),
    })?;
    if base == 0 {
        return Err(ByteSizeError::Zero);
    }

    let bytes = base
        .checked_mul(multiplier)
        .ok_or(ByteSizeError::Overflow)?;
    usize::try_from(bytes).map_err(|_| ByteSizeError::Overflow)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn bare_bytes() {
        assert_eq!(parse_byte_size("32").unwrap(), 32);
        assert_eq!(parse_byte_size("67108864").unwrap(), 67_108_864);
    }

    #[test]
    fn suffix_megabytes() {
        let expected = 64 * 1024 * 1024;
        assert_eq!(parse_byte_size("64mb").unwrap(), expected);
        assert_eq!(parse_byte_size("64MB").unwrap(), expected);
        assert_eq!(parse_byte_size("64mib").unwrap(), expected);
        assert_eq!(parse_byte_size("64MiB").unwrap(), expected);
        assert_eq!(parse_byte_size("64m").unwrap(), expected);
        assert_eq!(parse_byte_size("64M").unwrap(), expected);
    }

    #[test]
    fn suffix_gigabytes() {
        let expected = 1024_usize * 1024 * 1024;
        assert_eq!(parse_byte_size("1gb").unwrap(), expected);
        assert_eq!(parse_byte_size("1GB").unwrap(), expected);
        assert_eq!(parse_byte_size("1gib").unwrap(), expected);
        assert_eq!(parse_byte_size("1g").unwrap(), expected);
    }

    #[test]
    fn suffix_kilobytes() {
        assert_eq!(parse_byte_size("512k").unwrap(), 512 * 1024);
        assert_eq!(parse_byte_size("512kb").unwrap(), 512 * 1024);
        assert_eq!(parse_byte_size("512kib").unwrap(), 512 * 1024);
    }

    #[test]
    fn rejects_invalid() {
        assert_eq!(parse_byte_size(""), Err(ByteSizeError::Empty));
        assert_eq!(parse_byte_size("0"), Err(ByteSizeError::Zero));
        assert_eq!(parse_byte_size("0mb"), Err(ByteSizeError::Zero));
        assert_eq!(
            parse_byte_size("foo"),
            Err(ByteSizeError::InvalidSuffix {
                detail: "foo".to_owned()
            })
        );
        assert_eq!(
            parse_byte_size("64mbx"),
            Err(ByteSizeError::InvalidSuffix {
                detail: "64mbx".to_owned()
            })
        );
        assert_eq!(
            parse_byte_size("1.5gb"),
            Err(ByteSizeError::InvalidNumber {
                detail: "1.5gb".to_owned()
            })
        );
    }
}
