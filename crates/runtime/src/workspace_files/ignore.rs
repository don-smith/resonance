//! Replicated, anchored workspace ignore patterns.

use std::collections::BTreeMap;

use unicode_normalization::UnicodeNormalization;

const CONFLICT_MARKER: &str = ".resonance-conflict-";

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WorkspaceIgnoreSet {
    rules: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceIgnoreRule {
    pub operation_id: String,
    pub pattern: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IgnorePatternError {
    Empty,
    Absolute,
    EmptySegment,
    DotSegment,
    NullByte,
    NonNfc,
    Separator,
    InvalidDoubleStar,
    UnsupportedSyntax,
    PermanentRule,
}

impl std::fmt::Display for IgnorePatternError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => formatter.write_str("ignore pattern is empty"),
            Self::Absolute => formatter.write_str("ignore pattern must be relative to the root"),
            Self::EmptySegment => formatter.write_str("ignore pattern contains an empty segment"),
            Self::DotSegment => formatter.write_str("ignore pattern contains a dot segment"),
            Self::NullByte => formatter.write_str("ignore pattern contains a null byte"),
            Self::NonNfc => formatter.write_str("ignore pattern must use NFC-normalized UTF-8"),
            Self::Separator => formatter.write_str("ignore pattern contains a backslash"),
            Self::InvalidDoubleStar => {
                formatter.write_str("** must occupy a complete ignore-pattern segment")
            }
            Self::UnsupportedSyntax => {
                formatter.write_str("ignore pattern uses unsupported bracket or brace syntax")
            }
            Self::PermanentRule => formatter
                .write_str(".git and generated conflict names are already permanently excluded"),
        }
    }
}

impl std::error::Error for IgnorePatternError {}

impl WorkspaceIgnoreSet {
    pub(crate) fn from_rules(rules: BTreeMap<String, String>) -> Self {
        Self { rules }
    }

    #[must_use]
    pub fn rules(&self) -> Vec<WorkspaceIgnoreRule> {
        self.rules
            .iter()
            .map(|(operation_id, pattern)| WorkspaceIgnoreRule {
                operation_id: operation_id.clone(),
                pattern: pattern.clone(),
            })
            .collect()
    }

    #[must_use]
    pub fn matches_configured(&self, relative_path: &str) -> bool {
        self.rules
            .values()
            .any(|pattern| pattern_matches(pattern, relative_path))
    }

    #[must_use]
    pub fn is_ignored_input(&self, relative_path: &str) -> bool {
        is_git_path(relative_path)
            || is_generated_conflict_path(relative_path)
            || self.matches_configured(relative_path)
    }
}

pub fn validate_ignore_pattern(pattern: &str) -> Result<(), IgnorePatternError> {
    if pattern.is_empty() {
        return Err(IgnorePatternError::Empty);
    }
    if pattern.starts_with('/') || pattern.starts_with('\\') {
        return Err(IgnorePatternError::Absolute);
    }
    if pattern.contains('\0') {
        return Err(IgnorePatternError::NullByte);
    }
    if pattern.contains('\\') {
        return Err(IgnorePatternError::Separator);
    }
    if pattern.contains(['[', ']', '{', '}']) {
        return Err(IgnorePatternError::UnsupportedSyntax);
    }
    if !pattern.chars().nfc().eq(pattern.chars()) {
        return Err(IgnorePatternError::NonNfc);
    }
    let segments = pattern.split('/').collect::<Vec<_>>();
    for segment in &segments {
        if segment.is_empty() {
            return Err(IgnorePatternError::EmptySegment);
        }
        if matches!(*segment, "." | "..") {
            return Err(IgnorePatternError::DotSegment);
        }
        if segment.contains("**") && *segment != "**" {
            return Err(IgnorePatternError::InvalidDoubleStar);
        }
    }
    if pattern_matches(pattern, ".git")
        || pattern_matches(pattern, "nested/.git/config")
        || pattern_matches(pattern, "file.resonance-conflict-example.md")
    {
        return Err(IgnorePatternError::PermanentRule);
    }
    Ok(())
}

#[must_use]
pub fn is_git_path(relative_path: &str) -> bool {
    relative_path
        .split('/')
        .any(|segment| segment.eq_ignore_ascii_case(".git"))
}

#[must_use]
pub fn is_generated_conflict_path(relative_path: &str) -> bool {
    relative_path
        .split('/')
        .any(|segment| segment.to_ascii_lowercase().contains(CONFLICT_MARKER))
}

#[must_use]
pub fn ignore_pattern_matches(pattern: &str, relative_path: &str) -> bool {
    pattern_matches(pattern, relative_path)
}

fn pattern_matches(pattern: &str, relative_path: &str) -> bool {
    let pattern = pattern.split('/').collect::<Vec<_>>();
    let path = relative_path.split('/').collect::<Vec<_>>();
    matches_segments(&pattern, &path)
}

fn matches_segments(pattern: &[&str], path: &[&str]) -> bool {
    let Some((segment, remaining_pattern)) = pattern.split_first() else {
        return path.is_empty();
    };
    if *segment == "**" {
        return matches_segments(remaining_pattern, path)
            || path
                .split_first()
                .is_some_and(|(_, remaining_path)| matches_segments(pattern, remaining_path));
    }
    path.split_first()
        .is_some_and(|(path_segment, remaining_path)| {
            matches_segment(segment, path_segment)
                && matches_segments(remaining_pattern, remaining_path)
        })
}

fn matches_segment(pattern: &str, value: &str) -> bool {
    let pattern = pattern.chars().collect::<Vec<_>>();
    let value = value.chars().collect::<Vec<_>>();
    let mut matches = vec![vec![false; value.len() + 1]; pattern.len() + 1];
    matches[0][0] = true;
    for pattern_index in 0..pattern.len() {
        for value_index in 0..=value.len() {
            if !matches[pattern_index][value_index] {
                continue;
            }
            match pattern[pattern_index] {
                '*' => {
                    matches[pattern_index + 1][value_index] = true;
                    if value_index < value.len() {
                        matches[pattern_index][value_index + 1] = true;
                    }
                }
                '?' if value_index < value.len() => {
                    matches[pattern_index + 1][value_index + 1] = true;
                }
                literal if value.get(value_index) == Some(&literal) => {
                    matches[pattern_index + 1][value_index + 1] = true;
                }
                _ => {}
            }
        }
    }
    matches[pattern.len()][value.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_anchored_segment_globs() {
        assert!(pattern_matches("scratch/**", "scratch"));
        assert!(pattern_matches("scratch/**", "scratch/a/cache.bin"));
        assert!(pattern_matches("plans/*.tmp", "plans/draft.tmp"));
        assert!(!pattern_matches("plans/*.tmp", "archive/draft.tmp"));
        assert!(!pattern_matches("plans/*.tmp", "plans/nested/draft.tmp"));
        assert!(pattern_matches("**/*.tmp", "plans/nested/draft.tmp"));
    }

    #[test]
    fn rejects_ambiguous_and_permanent_patterns() {
        for pattern in ["", "/tmp", "a//b", "a/**x", "a/[x]", ".git", "**/.git/**"] {
            assert!(
                validate_ignore_pattern(pattern).is_err(),
                "accepted {pattern:?}"
            );
        }
        assert!(validate_ignore_pattern("scratch/**").is_ok());
        assert!(validate_ignore_pattern("plans/*.tmp").is_ok());
    }
}
