//! Portable path policy: NFC relative segments, reject escaping / reserved / device names.

use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PortablePath {
    segments: Vec<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum PathError {
    Empty,
    Absolute,
    Escape,
    EmptySegment,
    DotSegment,
    NullByte,
    NonNfc,
    SeparatorInName,
    ReservedDeviceName,
    ReservedGitMetadata,
    ReservedConflictName,
    CaseFoldCollision,
}

impl std::fmt::Display for PathError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => formatter.write_str("path is empty"),
            Self::Absolute => formatter.write_str("path must be relative"),
            Self::Escape => formatter.write_str("path escapes the workspace root"),
            Self::EmptySegment => formatter.write_str("path contains an empty segment"),
            Self::DotSegment => formatter.write_str("path contains a dot segment"),
            Self::NullByte => formatter.write_str("path contains a null byte"),
            Self::NonNfc => formatter.write_str("path must use NFC-normalized UTF-8"),
            Self::SeparatorInName => formatter.write_str("segment name contains a path separator"),
            Self::ReservedDeviceName => {
                formatter.write_str("segment name is a reserved device name")
            }
            Self::ReservedGitMetadata => {
                formatter.write_str(".git is permanently outside workspace authority")
            }
            Self::ReservedConflictName => {
                formatter.write_str("segment name uses a reserved conflict prefix")
            }
            Self::CaseFoldCollision => {
                formatter.write_str("path collides with an existing name after case folding")
            }
        }
    }
}

impl std::error::Error for PathError {}

const RESERVED_CONFLICT_PREFIX: &str = ".resonance-conflict-";

const RESERVED_DEVICE_NAMES: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

impl PortablePath {
    pub fn parse(path: &str) -> Result<Self, PathError> {
        if path.is_empty() {
            return Err(PathError::Empty);
        }
        if path.starts_with('/') || path.starts_with('\\') {
            return Err(PathError::Absolute);
        }
        if path.contains('\0') {
            return Err(PathError::NullByte);
        }
        let segments: Vec<&str> = path.split('/').collect();
        let mut owned = Vec::with_capacity(segments.len());
        for segment in &segments {
            if segment.is_empty() {
                return Err(PathError::EmptySegment);
            }
            if *segment == "." || *segment == ".." {
                return Err(PathError::DotSegment);
            }
            if segment.contains('/') || segment.contains('\\') {
                return Err(PathError::SeparatorInName);
            }
            if !is_nfc(segment) {
                return Err(PathError::NonNfc);
            }
            let lower = segment.to_lowercase();
            if lower == ".git" {
                return Err(PathError::ReservedGitMetadata);
            }
            if lower.contains(RESERVED_CONFLICT_PREFIX) {
                return Err(PathError::ReservedConflictName);
            }
            let upper = segment.to_uppercase();
            if RESERVED_DEVICE_NAMES.contains(&upper.as_str()) {
                return Err(PathError::ReservedDeviceName);
            }
            owned.push(segment.to_string());
        }
        Ok(Self { segments: owned })
    }

    pub fn check_no_case_fold_collision(
        &self,
        existing: &BTreeSet<String>,
    ) -> Result<(), PathError> {
        let folded = self
            .segments
            .last()
            .map(|s| s.to_lowercase())
            .unwrap_or_default();
        if existing.iter().any(|name| name.to_lowercase() == folded) {
            return Err(PathError::CaseFoldCollision);
        }
        Ok(())
    }

    #[must_use]
    pub fn segments(&self) -> &[String] {
        &self.segments
    }

    #[must_use]
    pub fn name(&self) -> Option<&str> {
        self.segments.last().map(String::as_str)
    }

    #[must_use]
    pub fn parent_segments(&self) -> &[String] {
        let len = self.segments.len().saturating_sub(1);
        &self.segments[..len]
    }
}

impl std::fmt::Display for PortablePath {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (i, segment) in self.segments.iter().enumerate() {
            if i > 0 {
                formatter.write_str("/")?;
            }
            formatter.write_str(segment)?;
        }
        Ok(())
    }
}

fn is_nfc(value: &str) -> bool {
    use unicode_normalization::UnicodeNormalization;
    value.chars().nfc().eq(value.chars())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_valid_relative_portable_paths() {
        assert!(PortablePath::parse("plans").is_ok());
        assert!(PortablePath::parse("plans/quarterly.md").is_ok());
        assert!(PortablePath::parse("a/b/c/d.txt").is_ok());
    }

    #[test]
    fn rejects_empty_and_absolute_paths() {
        assert_eq!(PortablePath::parse(""), Err(PathError::Empty));
        assert_eq!(PortablePath::parse("/etc"), Err(PathError::Absolute));
    }

    #[test]
    fn rejects_dot_and_dotdot_segments() {
        assert_eq!(PortablePath::parse("."), Err(PathError::DotSegment));
        assert_eq!(PortablePath::parse(".."), Err(PathError::DotSegment));
        assert_eq!(PortablePath::parse("a/../b"), Err(PathError::DotSegment));
    }

    #[test]
    fn rejects_separators_in_names_and_null_bytes() {
        assert_eq!(
            PortablePath::parse("a/b"),
            Ok(PortablePath::parse("a/b").unwrap())
        );
        assert_eq!(PortablePath::parse("a/\0b"), Err(PathError::NullByte));
    }

    #[test]
    fn rejects_reserved_names() {
        assert_eq!(
            PortablePath::parse("CON"),
            Err(PathError::ReservedDeviceName)
        );
        assert_eq!(
            PortablePath::parse("a/file.resonance-conflict-abc.md"),
            Err(PathError::ReservedConflictName)
        );
        assert_eq!(
            PortablePath::parse("a/.GIT/config"),
            Err(PathError::ReservedGitMetadata)
        );
    }
}
