//! Deterministic line-based three-way merge for UTF-8 Markdown.

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MergeResult {
    Merged(String),
    Conflict { reason: &'static str },
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Edit {
    start: usize,
    end: usize,
    replacement: Vec<String>,
}

pub fn merge_markdown(base: &str, left: &str, right: &str) -> MergeResult {
    if left == right {
        return MergeResult::Merged(left.to_owned());
    }
    if left == base {
        return MergeResult::Merged(right.to_owned());
    }
    if right == base {
        return MergeResult::Merged(left.to_owned());
    }

    let base_lines = split_lines(base);
    let left_edits = diff(&base_lines, &split_lines(left));
    let right_edits = diff(&base_lines, &split_lines(right));
    if left_edits
        .iter()
        .any(|left| right_edits.iter().any(|right| overlaps(left, right)))
    {
        return MergeResult::Conflict {
            reason: "overlapping Markdown edits",
        };
    }

    let mut edits = left_edits;
    for edit in right_edits {
        if !edits.contains(&edit) {
            edits.push(edit);
        }
    }
    edits.sort_by(|left, right| {
        left.start
            .cmp(&right.start)
            .then(left.end.cmp(&right.end))
            .then(left.replacement.cmp(&right.replacement))
    });

    let mut merged = Vec::new();
    let mut base_index = 0;
    for edit in edits {
        merged.extend(base_lines[base_index..edit.start].iter().cloned());
        merged.extend(edit.replacement);
        base_index = edit.end;
    }
    merged.extend(base_lines[base_index..].iter().cloned());
    MergeResult::Merged(merged.concat())
}

fn split_lines(text: &str) -> Vec<String> {
    if text.is_empty() {
        return Vec::new();
    }
    let mut lines = text
        .split_inclusive('\n')
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    if !text.ends_with('\n') && lines.is_empty() {
        lines.push(text.to_owned());
    }
    lines
}

fn diff(base: &[String], revision: &[String]) -> Vec<Edit> {
    let mut lcs = vec![vec![0_usize; revision.len() + 1]; base.len() + 1];
    for base_index in (0..base.len()).rev() {
        for revision_index in (0..revision.len()).rev() {
            lcs[base_index][revision_index] = if base[base_index] == revision[revision_index] {
                lcs[base_index + 1][revision_index + 1] + 1
            } else {
                lcs[base_index + 1][revision_index].max(lcs[base_index][revision_index + 1])
            };
        }
    }

    let mut edits = Vec::new();
    let mut base_index = 0;
    let mut revision_index = 0;
    let mut current: Option<Edit> = None;
    while base_index < base.len() || revision_index < revision.len() {
        if base_index < base.len()
            && revision_index < revision.len()
            && base[base_index] == revision[revision_index]
        {
            if let Some(edit) = current.take() {
                edits.push(edit);
            }
            base_index += 1;
            revision_index += 1;
            continue;
        }

        let edit = current.get_or_insert_with(|| Edit {
            start: base_index,
            end: base_index,
            replacement: Vec::new(),
        });
        let insert = revision_index < revision.len()
            && (base_index == base.len()
                || lcs[base_index][revision_index + 1] > lcs[base_index + 1][revision_index]);
        if insert {
            edit.replacement.push(revision[revision_index].clone());
            revision_index += 1;
        } else if base_index < base.len() {
            base_index += 1;
            edit.end = base_index;
        }
    }
    if let Some(edit) = current {
        edits.push(edit);
    }
    edits
}

fn overlaps(left: &Edit, right: &Edit) -> bool {
    let left_insert = left.start == left.end;
    let right_insert = right.start == right.end;
    match (left_insert, right_insert) {
        (true, true) => left.start == right.start && left.replacement != right.replacement,
        (true, false) => left.start >= right.start && left.start <= right.end,
        (false, true) => right.start >= left.start && right.start <= left.end,
        (false, false) => left.start < right.end && right.start < left.end,
    }
}

#[cfg(test)]
mod tests {
    use super::{merge_markdown, MergeResult};

    #[test]
    fn merges_disjoint_line_changes() {
        assert_eq!(
            merge_markdown(
                "one\ntwo\nthree\nfour\nfive\n",
                "ONE\ntwo\nthree\nfour\nFIVE\n",
                "one\ntwo\nTHREE\nfour\nfive\n",
            ),
            MergeResult::Merged("ONE\ntwo\nTHREE\nfour\nFIVE\n".to_owned())
        );
    }

    #[test]
    fn merges_insertions_at_different_positions() {
        assert_eq!(
            merge_markdown("a\nc\n", "a\nb\nc\n", "a\nc\nd\n"),
            MergeResult::Merged("a\nb\nc\nd\n".to_owned())
        );
    }

    #[test]
    fn conflicts_on_overlap_and_competing_same_position_insertions() {
        for (base, left, right) in [
            ("one\ntwo\n", "left\ntwo\n", "right\ntwo\n"),
            ("one\n", "new-left\none\n", "new-right\none\n"),
        ] {
            assert!(matches!(
                merge_markdown(base, left, right),
                MergeResult::Conflict { .. }
            ));
        }
    }
}
