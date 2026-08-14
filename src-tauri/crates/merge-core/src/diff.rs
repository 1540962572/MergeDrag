//! Line-based diff utilities used by the merge engine.
//!
//! Classic LCS dynamic programming + backtrack, producing an edit script as a
//! sequence of [`Edit`] ops. Ops are emitted per line then coalesced into
//! runs, which keeps the walker simple and obviously correct.

/// A single line-level edit operation. Indexes refer to positions in the
/// original text, zero-based. `Keep`/`Del` ranges index into `a`; `Ins`
/// carries text to insert before original index `pos`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Edit {
    Keep { start: u32, end: u32 },
    Del { start: u32, end: u32 },
    Ins { pos: u32, text: String },
}

/// LCS DP table between `a` and `b`; `dp[i][j]` is the LCS length of `a[i..]`
/// and `b[j..]`.
pub fn lcs(a: &[&str], b: &[&str]) -> Vec<Vec<u32>> {
    let n = a.len();
    let m = b.len();
    let mut dp = vec![vec![0u32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            dp[i][j] = if a[i] == b[j] {
                dp[i + 1][j + 1] + 1
            } else {
                dp[i + 1][j].max(dp[i][j + 1])
            };
        }
    }
    dp
}

/// Diff two texts line by line; returns a coalesced edit script.
pub fn diff_lines(a: &str, b: &str) -> Vec<Edit> {
    let a_lines = split_lines(a);
    let b_lines = split_lines(b);

    if a_lines.is_empty() && b_lines.is_empty() {
        return vec![];
    }
    if a_lines.is_empty() {
        let mut text = String::new();
        for l in &b_lines {
            text.push_str(l);
        }
        return vec![Edit::Ins { pos: 0, text }];
    }
    if b_lines.is_empty() {
        return vec![Edit::Del {
            start: 0,
            end: a_lines.len() as u32,
        }];
    }

    let dp = lcs(&a_lines, &b_lines);
    let mut ops: Vec<(u8, u32, u32, String)> = Vec::new(); // (kind, a_idx, b_idx, text)
    backtrack(&dp, &a_lines, &b_lines, 0, 0, &mut ops);
    coalesce(&a_lines, ops)
}

fn backtrack(
    dp: &[Vec<u32>],
    a: &[&str],
    b: &[&str],
    mut i: usize,
    mut j: usize,
    ops: &mut Vec<(u8, u32, u32, String)>,
) {
    let n = a.len();
    let m = b.len();
    loop {
        while i < n && j < m && a[i] == b[j] {
            ops.push((0, i as u32, j as u32, String::new())); // keep
            i += 1;
            j += 1;
        }
        if i == n && j == m {
            return;
        }
        if i < n && (j == m || dp[i + 1][j] >= dp[i][j + 1]) {
            ops.push((1, i as u32, j as u32, String::new())); // del
            i += 1;
        } else {
            ops.push((2, i as u32, j as u32, b[j].to_string())); // ins
            j += 1;
        }
    }
}

/// Merge consecutive identical ops into `Edit` runs.
fn coalesce(a: &[&str], ops: Vec<(u8, u32, u32, String)>) -> Vec<Edit> {
    let mut edits: Vec<Edit> = Vec::new();
    for (kind, ai, _bi, text) in ops {
        match kind {
            0 => {
                if let Some(Edit::Keep { end, .. }) = edits.last_mut() {
                    if *end == ai {
                        *end += 1;
                        continue;
                    }
                }
                edits.push(Edit::Keep {
                    start: ai,
                    end: ai + 1,
                });
            }
            1 => {
                if let Some(Edit::Del { end, .. }) = edits.last_mut() {
                    if *end == ai {
                        *end += 1;
                        continue;
                    }
                }
                edits.push(Edit::Del {
                    start: ai,
                    end: ai + 1,
                });
            }
            _ => {
                if let Some(Edit::Ins { pos, text: t }) = edits.last_mut() {
                    if *pos == ai {
                        t.push_str(&text);
                        continue;
                    }
                }
                edits.push(Edit::Ins { pos: ai, text });
            }
        }
    }
    let _ = a;
    edits
}

/// Split text into lines, each retaining its trailing `\n` except possibly
/// the last line when the text doesn't end with a newline. Interior empty
/// lines survive as `"\n"`. With this, concatenating the lines of a region
/// reproduces the original text exactly.
pub(crate) fn split_lines(text: &str) -> Vec<&str> {
    if text.is_empty() {
        return vec![];
    }
    text.split_inclusive('\n').collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_simple_replace() {
        let edits = diff_lines("B\n", "X\n");
        assert!(edits.iter().any(|e| matches!(e, Edit::Del { .. })));
        assert!(edits.iter().any(|e| matches!(e, Edit::Ins { .. })));
    }

    #[test]
    fn diff_keeps_common_lines() {
        let edits = diff_lines("a\nc\n", "a\nb\nc\n");
        let keeps = edits
            .iter()
            .filter(|e| matches!(e, Edit::Keep { .. }))
            .count();
        assert!(keeps >= 2, "both common lines kept");
    }

    #[test]
    fn empty_texts() {
        assert_eq!(diff_lines("", ""), vec![]);
        assert_eq!(
            diff_lines("", "a\n"),
            vec![Edit::Ins {
                pos: 0,
                text: "a\n".into()
            }]
        );
        assert_eq!(diff_lines("a\n", ""), vec![Edit::Del { start: 0, end: 1 }]);
    }

    #[test]
    fn change_middle_keeps_edges() {
        let edits = diff_lines("a\nb\nc\n", "a\nx\nc\n");
        assert!(edits.iter().any(|e| matches!(e, Edit::Del { .. })));
        assert!(edits.iter().any(|e| matches!(e, Edit::Ins { .. })));
        assert!(edits.iter().any(|e| matches!(e, Edit::Keep { .. })));
    }
}
