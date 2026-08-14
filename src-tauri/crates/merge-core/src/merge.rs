//! Three-way merge: combine LOCAL and REMOTE against a common BASE.
//!
//! Classic diff3 semantics, line-based:
//! 1. diff(base, local) and diff(base, remote) into two-way hunks.
//! 2. Split the base line range into segments bounded by every hunk start/end.
//! 3. For each segment, each side contributes either its covering hunk text or
//!    the base lines; equal contributions are clean, differing ones conflict.
//!    Empty segments (insertion points) only conflict when **both** sides
//!    insert different text at the same point.

use std::collections::BTreeSet;

use crate::diff::{diff_lines, split_lines, Edit};
use crate::{Decision, Hunk, LineRange, MergeDocument};

/// A two-way diff hunk: base lines `[start, end)` replaced by `text`.
#[derive(Debug, Clone)]
struct H2 {
    start: u32,
    end: u32,
    text: String,
}

pub fn three_way(
    local: &str,
    remote: &str,
    base: Option<&str>,
    file_label: impl Into<String>,
) -> MergeDocument {
    three_way_with(local, remote, base.unwrap_or(""), file_label)
}

/// Full three-way merge. With no base, `base` is empty so every line looks
/// changed on both sides — differences surface as conflicts instead of guesses.
pub fn three_way_with(
    local: &str,
    remote: &str,
    base: &str,
    file_label: impl Into<String>,
) -> MergeDocument {
    let base_lines = split_lines(base);

    let h1 = hunks(&diff_lines(base, local));
    let h2 = hunks(&diff_lines(base, remote));

    let n = base_lines.len() as u32;
    let mut boundaries = BTreeSet::new();
    boundaries.insert(0);
    boundaries.insert(n);
    for h in h1.iter().chain(h2.iter()) {
        boundaries.insert(h.start.min(n));
        boundaries.insert(h.end.min(n));
    }

    let mut hunks_out: Vec<Hunk> = Vec::new();
    let mut next_id: u64 = 0;

    let bounds: Vec<u32> = boundaries.into_iter().collect();
    // Insertion-style hunks (start == end) are emitted at their position;
    // region hunks (start < end) cover their base segment.
    let ins_l: Vec<&H2> = h1.iter().filter(|h| h.start == h.end).collect();
    let ins_r: Vec<&H2> = h2.iter().filter(|h| h.start == h.end).collect();

    for w in bounds.windows(2) {
        let s = w[0];
        let e = w[1];

        // Insertions anchored at s.
        emit_insertions(&mut hunks_out, &mut next_id, s, &ins_l, &ins_r);

        if s < e {
            let seg = s..e;
            let lh = covering(&h1, seg.clone());
            let rh = covering(&h2, seg.clone());

            match (lh, rh) {
                // Neither side touched this region -> base text.
                (None, None) => {
                    hunks_out.push(Hunk::Clean {
                        text: base_text(&base_lines, seg),
                    });
                }
                // Exactly one side changed it -> take that side, cleanly.
                (Some(h), None) => hunks_out.push(Hunk::Clean {
                    text: h.text.clone(),
                }),
                (None, Some(h)) => hunks_out.push(Hunk::Clean {
                    text: h.text.clone(),
                }),
                // Both sides changed it: identical -> clean, differing -> conflict.
                (Some(a), Some(b)) if a.text == b.text => hunks_out.push(Hunk::Clean {
                    text: a.text.clone(),
                }),
                (Some(a), Some(b)) => {
                    hunks_out.push(Hunk::Conflict {
                        id: next_id,
                        local: a.text.clone(),
                        remote: b.text.clone(),
                        base: Some(base_text(&base_lines, seg)),
                        decision: Decision::Unresolved,
                    });
                    next_id += 1;
                }
            }
        }
    }

    // Trailing insertion(s) at n (start == end == n).
    emit_insertions(&mut hunks_out, &mut next_id, n, &ins_l, &ins_r);

    MergeDocument {
        file_label: file_label.into(),
        hunks: hunks_out,
    }
}

/// Convert a raw edit script into two-way hunks over base coordinates.
/// Keeps advance both cursors; Del advances the base cursor; Ins appends.
fn hunks(edits: &[Edit]) -> Vec<H2> {
    let mut out: Vec<H2> = Vec::new();
    let mut base_cur: u32 = 0;
    let mut region: Option<u32> = None; // region start in base coords
    let mut text = String::new();

    for e in edits {
        match e {
            Edit::Keep { end, .. } => {
                // Push any open region, then advance. (Keeps never open regions.)
                if let Some(s) = region.take() {
                    out.push(H2 {
                        start: s,
                        end: base_cur,
                        text: std::mem::take(&mut text),
                    });
                }
                base_cur = *end;
            }
            Edit::Del { start, end } => {
                if region.is_none() {
                    region = Some(*start);
                }
                base_cur = *end;
            }
            Edit::Ins { text: t, .. } => {
                if region.is_none() {
                    region = Some(base_cur);
                }
                text.push_str(t);
            }
        }
    }
    if let Some(s) = region {
        out.push(H2 {
            start: s,
            end: base_cur,
            text,
        });
    }
    out
}

/// Emit insertions anchored at `pos` from both sides.
fn emit_insertions(out: &mut Vec<Hunk>, next_id: &mut u64, pos: u32, ins_l: &[&H2], ins_r: &[&H2]) {
    let l: Vec<&H2> = ins_l.iter().filter(|h| h.start == pos).copied().collect();
    let r: Vec<&H2> = ins_r.iter().filter(|h| h.start == pos).copied().collect();
    if l.is_empty() && r.is_empty() {
        return;
    }
    let lt: String = l.iter().map(|h| h.text.as_str()).collect();
    let rt: String = r.iter().map(|h| h.text.as_str()).collect();

    if l.is_empty() {
        out.push(Hunk::Clean { text: rt });
    } else if r.is_empty() || lt == rt {
        out.push(Hunk::Clean { text: lt });
    } else {
        out.push(Hunk::Conflict {
            id: *next_id,
            local: lt,
            remote: rt,
            base: None,
            decision: Decision::Unresolved,
        });
        *next_id += 1;
    }
}

/// The hunk covering base segment `seg`, if any. Hunks are sorted and never
/// overlap, so at most one matches.
fn covering(h: &[H2], seg: std::ops::Range<u32>) -> Option<&H2> {
    h.iter()
        .find(|x| x.start <= seg.start && x.end >= seg.end && seg.start < seg.end)
}

fn base_text(base: &[&str], seg: std::ops::Range<u32>) -> String {
    let mut out = String::new();
    for l in &base[seg.start as usize..seg.end as usize] {
        out.push_str(l);
    }
    out
}

impl MergeDocument {
    /// Return a copy with the given conflict's decision replaced. Ids for
    /// clean hunks don't exist; the id is scoped to this document.
    pub fn with_decision(&self, hunk_id: u64, decision: Decision) -> MergeDocument {
        let hunks = self
            .hunks
            .iter()
            .map(|h| match h {
                Hunk::Conflict {
                    id,
                    local,
                    remote,
                    base,
                    ..
                } if *id == hunk_id => Hunk::Conflict {
                    id: *id,
                    local: local.clone(),
                    remote: remote.clone(),
                    base: base.clone(),
                    decision: decision.clone(),
                },
                other => other.clone(),
            })
            .collect();
        MergeDocument {
            file_label: self.file_label.clone(),
            hunks,
        }
    }

    /// Line ranges (within the Result pane) of every conflict hunk, for nav.
    pub fn conflict_line_ranges(&self) -> Vec<(u64, LineRange)> {
        let mut out = Vec::new();
        let mut line = 0u32;
        for hunk in &self.hunks {
            let text = match hunk {
                Hunk::Clean { text } => text.as_str(),
                Hunk::Conflict { local, remote, .. } => {
                    if local.len() >= remote.len() {
                        local.as_str()
                    } else {
                        remote.as_str()
                    }
                }
            };
            let len = text.lines().count() as u32;
            if let Hunk::Conflict { id, .. } = hunk {
                out.push((
                    *id,
                    LineRange {
                        start: line,
                        end: line + len,
                    },
                ));
            }
            line += len;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disjoint_edits_merge_clean() {
        let doc = three_way_with("X\nB\n", "B\nY\n", "B\n", "t.rs");
        assert_eq!(doc.unresolved_count(), 0, "no conflict expected");
        assert_eq!(doc.apply(), "X\nB\nY\n");
    }

    #[test]
    fn one_side_edit_is_clean() {
        let doc = three_way_with("X\n", "B\n", "B\n", "t.rs");
        assert_eq!(doc.unresolved_count(), 0);
        assert_eq!(doc.apply(), "X\n");
    }

    #[test]
    fn both_sides_same_edit_is_clean() {
        let doc = three_way_with("X\n", "X\n", "B\n", "t.rs");
        assert_eq!(doc.unresolved_count(), 0);
        assert_eq!(doc.apply(), "X\n");
    }

    #[test]
    fn conflicting_edit_marks_conflict() {
        let doc = three_way_with("X\n", "Y\n", "B\n", "t.rs");
        assert_eq!(doc.unresolved_count(), 1);
        assert!(doc.apply().contains("<<<<<<<"));
        assert!(doc.apply().contains("X\n") && doc.apply().contains("Y\n"));
    }

    #[test]
    fn resolve_take_local() {
        let doc = three_way_with("X\n", "Y\n", "B\n", "t.rs");
        let Hunk::Conflict { id, .. } = doc.hunks[0] else {
            panic!("expected conflict");
        };
        let merged = doc.with_decision(id, Decision::TakeLocal);
        assert_eq!(merged.unresolved_count(), 0);
        assert_eq!(merged.apply(), "X\n");
    }

    #[test]
    fn merge_replaces_with_clean_append() {
        // local replaced B with X; remote kept B and appended Y.
        // Changes touch different lines -> clean merge.
        let doc = three_way_with("X\n", "B\nY\n", "B\n", "t.rs");
        assert_eq!(doc.unresolved_count(), 0);
        assert_eq!(doc.apply(), "X\nY\n");
    }

    #[test]
    fn conflicting_insertions_at_same_point() {
        let doc = three_way_with("a\nX\nc\n", "a\nY\nc\n", "a\nc\n", "t.rs");
        assert_eq!(doc.unresolved_count(), 1);
        assert!(doc.apply().contains("X\n") && doc.apply().contains("Y\n"));
        assert!(doc.apply().starts_with("a\n<<<<<<<"));
    }

    #[test]
    fn clean_insertions_differing_points() {
        let doc = three_way_with("X\na\n", "a\nY\n", "a\n", "t.rs");
        assert_eq!(doc.unresolved_count(), 0);
        assert_eq!(doc.apply(), "X\na\nY\n");
    }

    #[test]
    fn no_base_all_diff_is_conflict() {
        let doc = three_way_with("X\n", "B\n", "", "t.rs");
        assert_eq!(doc.unresolved_count(), 1);
    }

    #[test]
    fn identical_no_change_no_base_is_clean() {
        let doc = three_way_with("B\n", "B\n", "", "t.rs");
        assert_eq!(doc.unresolved_count(), 0);
        assert_eq!(doc.apply(), "B\n");
    }
}
