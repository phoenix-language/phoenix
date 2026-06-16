//! Human-readable byte size parsing for VM limits and project config.

/// Error parsing a byte size string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ByteSizeError {
    /// Input was empty or whitespace only.
    Empty,
    /// Value must be greater than zero.
    Zero,
    /// Unrecognized suffix or trailing junk.
    InvalidSuffix {
        /// Token that could not be parsed.
        detail: String,
    },
    /// Numeric portion is not a valid unsigned integer.
    InvalidNumber {
        /// Token that could not be parsed.
        detail: String,
    },
    /// Multiplication overflowed `usize`.
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

/// Parses a byte count from a bare integer or suffix string (`64mb`, `1gb`, `512k`).
///
/// Suffixes are binary (IEC): `k`/`kb`/`kib` = 1024, `m`/`mb`/`mib` = 1024²,
/// `g`/`gb`/`gib` = 1024³. Matching is case-insensitive.
///
/// # Errors
///
/// Returns [`ByteSizeError`] when the input is empty, zero, malformed, or overflows.
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
