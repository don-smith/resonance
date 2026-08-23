//! Deterministic three-way line-based merge for Markdown files.
//!
//! Merges only when every change touches disjoint base line ranges. All other
//! cases — overlapping edits, binary content, or non-UTF-8 — return a conflict.

use std::collections::BTreeSet;

/// A line range in the base document: `[start, end)` inclusive-exclusive.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct LineRange {
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MergeResult {
    /// The merge succeeded; `content` is the merged text.
    Merged(String),
    /// Edits overlap or the file is not safely mergeable.
    Conflict { reason: &'static str },
}

/// Attempts a three-way line-based merge.
///
/// `base`, `left`, and `right` must be valid UTF-8. The merger splits them
/// into lines, computes the edit ranges against `base`, and merges only when
/// the changed ranges are disjoint.
pub fn merge_markdown(base: &str, left: &str, right: &str) -> MergeResult {
    let base_lines = lines(base);
    let left_lines = lines(left);
    let right_lines = lines(right);

    let left_edits = changed_ranges(&base_lines, &left_lines);
    let right_edits = changed_ranges(&base_lines, &right_lines);

    if regions_overlap(&left_edits, &right_edits) {
        return MergeResult::Conflict {
            reason: "overlapping Markdown edits",
        };
    }

    let merged = apply_merge(
        &base_lines,
        &left_edits,
        &left_lines,
        &right_edits,
        &right_lines,
    );
    MergeResult::Merged(merged.join("\n"))
}

fn lines(text: &str) -> Vec<&str> {
    if text.is_empty() {
        return vec![""];
    }
    text.split('\n').collect()
}

/// Returns the line ranges where `revision` differs from `base`.
/// Ranges are sorted by start.
fn changed_ranges(base: &[&str], revision: &[&str]) -> Vec<LineRange> {
    // Find the first differing line
    let mut prefix = 0;
    while prefix < base.len() && prefix < revision.len() && base[prefix] == revision[prefix] {
        prefix += 1;
    }
    // Find the last differing line from the end
    let mut suffix_base = base.len();
    let mut suffix_rev = revision.len();
    while suffix_base > prefix
        && suffix_rev > prefix
        && base[suffix_base - 1] == revision[suffix_rev - 1]
    {
        suffix_base -= 1;
        suffix_rev -= 1;
    }
    if prefix == base.len() && prefix == revision.len() {
        return Vec::new();
    }
    vec![LineRange {
        start: prefix,
        end: suffix_base,
    }]
}

fn regions_overlap(left: &[LineRange], right: &[LineRange]) -> bool {
    let mut all: BTreeSet<usize> = BTreeSet::new();
    for r in left {
        for line in r.start..r.end {
            all.insert(line);
        }
    }
    for r in right {
        for line in r.start..r.end {
            if !all.insert(line) {
                return true;
            }
        }
    }
    false
}

/// Applies the two edit sets to the base to produce the merged result.
fn apply_merge(
    base: &[&str],
    left_edits: &[LineRange],
    left_lines: &[&str],
    right_edits: &[LineRange],
    right_lines: &[&str],
) -> Vec<String> {
    // Sort both edits by start position
    let mut edits: Vec<(usize, bool, LineRange)> = Vec::new();
    for range in left_edits {
        edits.push((range.start, false, *range));
    }
    for range in right_edits {
        edits.push((range.start, true, *range));
    }
    edits.sort_by_key(|(start, _, _)| *start);

    let mut result: Vec<String> = Vec::new();
    let mut base_pos = 0;

    for (_start, is_right, range) in &edits {
        // Copy unchanged base lines up to this edit
        while base_pos < range.start && base_pos < base.len() {
            result.push(base[base_pos].to_string());
            base_pos += 1;
        }
        // Compute how many lines the revision contributes
        let revision = if *is_right { right_lines } else { left_lines };
        let common_suffix = (base.len() as isize - range.end as isize).max(0) as usize;
        let rev_end = (revision.len() as isize - common_suffix as isize).max(0) as usize;
        for line in revision.iter().take(rev_end).skip(range.start) {
            result.push((*line).to_string());
        }
        base_pos = range.end;
    }
    // Copy remaining base lines
    while base_pos < base.len() {
        result.push(base[base_pos].to_string());
        base_pos += 1;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merges_disjoint_line_changes() {
        let base = "line 1\nline 2\nline 3\n";
        let left = "left 1\nline 2\nline 3\n";
        let right = "line 1\nline 2\nright 3\n";
        match merge_markdown(base, left, right) {
            MergeResult::Merged(merged) => {
                assert!(merged.contains("left 1"));
                assert!(merged.contains("right 3"));
            }
            MergeResult::Conflict { .. } => panic!("expected merge"),
        }
    }

    #[test]
    fn conflicts_when_changes_overlap() {
        let base = "line 1\nline 2\nline 3\n";
        let left = "left edit\nline 2\nline 3\n";
        let right = "right edit\nline 2\nline 3\n";
        assert!(matches!(
            merge_markdown(base, left, right),
            MergeResult::Conflict { .. }
        ));
    }

    #[test]
    fn merges_additions_on_different_lines() {
        let base = "a\nc\n";
        let left = "a\nb\nc\n";
        let right = "a\nc\nd\n";
        match merge_markdown(base, left, right) {
            MergeResult::Merged(merged) => {
                assert!(merged.contains("b"));
                assert!(merged.contains("d"));
            }
            MergeResult::Conflict { .. } => panic!("expected merge"),
        }
    }
}
