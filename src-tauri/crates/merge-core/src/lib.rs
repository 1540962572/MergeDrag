//! merge-core — pure-Rust three-way merge engine for MergeDrag.
//!
//! Takes LOCAL, REMOTE and optional BASE text; produces a list of hunks
//! (clean regions already merged, conflict regions awaiting a decision).
//! No Tauri / Git / UI dependencies — unit-testable in isolation.

pub mod diff;
pub mod merge;

pub use diff::{diff_lines, Edit};
pub use merge::{three_way, three_way_with};

/// A line range, half-open `[start, end)`, 0-based line numbers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LineRange {
    pub start: u32,
    pub end: u32,
}

/// User decision for an unresolved conflict hunk.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", content = "payload", rename_all = "PascalCase")]
pub enum Decision {
    Unresolved,
    TakeLocal,
    TakeRemote,
    TakeBoth {
        #[serde(rename = "localFirst")]
        local_first: bool,
    },
    /// This block is intentionally dropped from the result.
    Ignore,
    /// User edited the block directly in the Result pane.
    Manual(String),
}

/// One merge region. Clean regions are already merged and carry no decision;
/// conflict regions carry both sides and a pending decision.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "PascalCase")]
pub enum Hunk {
    Clean {
        text: String,
    },
    Conflict {
        id: u64,
        local: String,
        remote: String,
        base: Option<String>,
        decision: Decision,
    },
}

/// The full parsed three-way result for one file.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeDocument {
    /// Display label, e.g. `src/main.rs`.
    pub file_label: String,
    pub hunks: Vec<Hunk>,
}

/// Errors surfaced to the UI layer.
#[derive(Debug, thiserror::Error)]
pub enum MergeError {
    #[error("文件不是有效 UTF-8 或读取失败")]
    Encoding,
    #[error("三方合并失败: {0}")]
    Merge(String),
    #[error("文件过大暂不支持")]
    TooLarge,
}

impl MergeDocument {
    /// Number of conflict hunks still awaiting a decision.
    pub fn unresolved_count(&self) -> usize {
        self.hunks
            .iter()
            .filter(|h| match h {
                Hunk::Conflict { decision, .. } => {
                    matches!(decision, Decision::Unresolved | Decision::Manual(_))
                }
                Hunk::Clean { .. } => false,
            })
            .count()
    }

    /// Render the current document back to plain text (the file to save).
    pub fn apply(&self) -> String {
        let mut out = String::new();
        for hunk in &self.hunks {
            match hunk {
                Hunk::Clean { text } => out.push_str(text),
                Hunk::Conflict {
                    local,
                    remote,
                    decision,
                    ..
                } => match decision {
                    Decision::Unresolved | Decision::Manual(_) => {
                        // Unresolved: emit conflict markers so we never lose data.
                        out.push_str("<<<<<<< MergeDrag:ours\n");
                        out.push_str(local);
                        out.push_str("=======\n");
                        out.push_str(remote);
                        out.push_str(">>>>>>> MergeDrag:theirs\n");
                    }
                    Decision::TakeLocal => out.push_str(local),
                    Decision::TakeRemote => out.push_str(remote),
                    Decision::TakeBoth { local_first } => {
                        if *local_first {
                            out.push_str(local);
                            out.push_str(remote);
                        } else {
                            out.push_str(remote);
                            out.push_str(local);
                        }
                    }
                    Decision::Ignore => {}
                },
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_lines_are_auto_merged() {
        let doc = three_way("X\nB\n", "B\nY\n", Some("B\n"), "t.rs");
        assert_eq!(doc.unresolved_count(), 0, "no conflict expected");
        assert_eq!(doc.apply(), "X\nB\nY\n");
    }

    #[test]
    fn same_line_conflicts() {
        let local = "X\n";
        let remote = "Y\n";
        let base = "B\n";
        let doc = three_way(local, remote, Some(base), "t.rs");
        assert_eq!(doc.unresolved_count(), 1);
        let applied = doc.apply();
        assert!(applied.contains("<<<<<<<"));
        assert!(applied.contains("X\n") && applied.contains("Y\n"));
    }

    #[test]
    fn take_local_resolves() {
        let doc = three_way("X\n", "Y\n", Some("B\n"), "t.rs");
        let Hunk::Conflict { id, .. } = &doc.hunks[0] else {
            panic!("expected conflict");
        };
        let resolved = doc.with_decision(*id, Decision::TakeLocal);
        assert_eq!(resolved.unresolved_count(), 0);
        assert_eq!(resolved.apply(), "X\n");
    }

    #[test]
    fn ignore_drops_block() {
        let doc = three_way("X\n", "Y\n", Some("B\n"), "t.rs");
        let Hunk::Conflict { id, .. } = &doc.hunks[0] else {
            panic!("expected conflict");
        };
        let resolved = doc.with_decision(*id, Decision::Ignore);
        assert_eq!(resolved.apply(), "");
    }

    #[test]
    fn missing_base_falls_back_to_two_way() {
        let doc = three_way("X\n", "B\n", None, "t.rs");
        assert_eq!(doc.unresolved_count(), 1);
    }

    #[test]
    fn diff_utils_exposed() {
        let edits = diff_lines("B\n", "X\n");
        assert!(!edits.is_empty());
    }

    /// IPC 契约：spare 输出的 JSON 必须与前端 TS 类型完全一致
    /// （tag+content 邻接标签、camelCase 字段）。
    #[test]
    fn decision_json_contract_matches_frontend() {
        let both = serde_json::to_value(Decision::TakeBoth { local_first: true }).unwrap();
        assert_eq!(
            both,
            serde_json::json!({ "kind": "TakeBoth", "payload": { "localFirst": true } })
        );

        let manual = serde_json::to_value(Decision::Manual("x\n".into())).unwrap();
        assert_eq!(
            manual,
            serde_json::json!({ "kind": "Manual", "payload": "x\n" })
        );

        let hunk = Hunk::Conflict {
            id: 7,
            local: "L\n".into(),
            remote: "R\n".into(),
            base: Some("B\n".into()),
            decision: Decision::Unresolved,
        };
        let hunk_json = serde_json::to_value(&hunk).unwrap();
        assert_eq!(
            hunk_json,
            serde_json::json!({
                "kind": "Conflict",
                "id": 7,
                "local": "L\n",
                "remote": "R\n",
                "base": "B\n",
                "decision": { "kind": "Unresolved" }
            })
        );
    }
}
