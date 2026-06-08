//! PHX0 file header (24 bytes, little-endian).

/// ASCII magic bytes for Phoenix bytecode files.
pub const MAGIC: [u8; 4] = *b"PHX0";

/// Header size in bytes.
pub const HEADER_SIZE: usize = 24;

/// Format major version for MVP.
pub const VERSION_MAJOR: u16 = 0;

/// Format minor version for MVP.
pub const VERSION_MINOR: u16 = 2;

/// Sentinel `entry_function_id` for library images with no `main`.
pub const ENTRY_NONE: u32 = u32::MAX;

/// Parsed PHX0 file header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileHeader {
    /// Major format version.
    pub version_major: u16,
    /// Minor format version.
    pub version_minor: u16,
    /// Reserved flags (MVP must be zero).
    pub flags: u32,
    /// Number of section table entries.
    pub section_count: u32,
    /// Function id for `main`.
    pub entry_function_id: u32,
}

impl FileHeader {
    /// Creates an MVP header with `section_count` sections and `entry_function_id`.
    #[must_use]
    pub const fn new(section_count: u32, entry_function_id: u32) -> Self {
        Self {
            version_major: VERSION_MAJOR,
            version_minor: VERSION_MINOR,
            flags: 0,
            section_count,
            entry_function_id,
        }
    }

    /// Encodes the header to 24 bytes (little-endian).
    #[must_use]
    pub fn encode(&self) -> [u8; HEADER_SIZE] {
        let mut out = [0u8; HEADER_SIZE];
        out[0..4].copy_from_slice(&MAGIC);
        out[4..6].copy_from_slice(&self.version_major.to_le_bytes());
        out[6..8].copy_from_slice(&self.version_minor.to_le_bytes());
        out[8..12].copy_from_slice(&self.flags.to_le_bytes());
        out[12..16].copy_from_slice(&self.section_count.to_le_bytes());
        out[16..20].copy_from_slice(&self.entry_function_id.to_le_bytes());
        out[20..24].copy_from_slice(&0u32.to_le_bytes());
        out
    }

    /// Decodes a header from exactly 24 bytes.
    ///
    /// # Errors
    ///
    /// Returns [`HeaderError`] when magic or version are invalid.
    pub fn decode(bytes: &[u8; HEADER_SIZE]) -> Result<Self, HeaderError> {
        if bytes[0..4] != MAGIC {
            return Err(HeaderError::BadMagic);
        }
        let version_major = u16::from_le_bytes([bytes[4], bytes[5]]);
        let version_minor = u16::from_le_bytes([bytes[6], bytes[7]]);
        if version_major != VERSION_MAJOR || version_minor > VERSION_MINOR {
            return Err(HeaderError::UnsupportedVersion {
                major: version_major,
                minor: version_minor,
            });
        }
        Ok(Self {
            version_major,
            version_minor,
            flags: u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]),
            section_count: u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]),
            entry_function_id: u32::from_le_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]),
        })
    }
}

/// Errors when decoding a file header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeaderError {
    /// Magic is not `PHX0`.
    BadMagic,
    /// Version is not supported by this loader.
    UnsupportedVersion {
        /// Major version read from file.
        major: u16,
        /// Minor version read from file.
        minor: u16,
    },
}
