//! Git integration for MergeDrag: scan unmerged (conflicted) files in a
//! repository and register the app as a Git mergetool.

use std::path::{Path, PathBuf};
use std::process::Command;

/// A conflicted file in the working tree.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictFile {
    pub path: PathBuf,
    pub is_binary: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum GitBridgeError {
    #[error("Git 仓库不可用: {0}")]
    NoRepo(String),
    #[error("Git 命令执行失败: {0}")]
    Command(String),
    #[error("Git 未安装")]
    GitNotFound,
}

/// List files that are currently in a conflicted (unmerged) state in `repo`.
/// Uses the git index: conflicted entries appear with stage 1/2/3.
pub fn list_unmerged(repo: &Path) -> Result<Vec<ConflictFile>, GitBridgeError> {
    let repo = git2::Repository::open(repo).map_err(|e| GitBridgeError::NoRepo(e.to_string()))?;
    let workdir = repo.workdir().map(|p| p.to_path_buf());
    let index = repo
        .index()
        .map_err(|e| GitBridgeError::NoRepo(e.to_string()))?;

    let mut seen: Vec<(PathBuf, bool)> = Vec::new();
    for entry in index.iter() {
        let stage = ((entry.flags >> 12) & 0x3) as u32;
        if stage != 0 {
            let path = PathBuf::from(String::from_utf8_lossy(&entry.path).into_owned());
            if !seen.iter().any(|(p, _)| p == &path) {
                let binary = workdir
                    .as_ref()
                    .map(|wd| sniff_binary(&wd.join(&path)))
                    .unwrap_or(false);
                seen.push((path.clone(), binary));
            }
        }
    }
    Ok(seen
        .into_iter()
        .map(|(path, is_binary)| ConflictFile { path, is_binary })
        .collect())
}

/// Whether a file looks binary (contains NUL bytes in the first 8 KB).
pub fn sniff_binary(path: &Path) -> bool {
    let Ok(data) = std::fs::read(path) else {
        return true;
    };
    data.iter().take(8192).any(|&b| b == 0)
}

/// Stage one file (`git add`) so merges resolve out of the unmerged state.
/// `repo` is the repository working directory; `rel` is the repo-relative path.
pub fn stage_file(repo: &Path, rel: &Path) -> Result<(), GitBridgeError> {
    let repo = git2::Repository::open(repo).map_err(|e| GitBridgeError::NoRepo(e.to_string()))?;
    let mut index = repo
        .index()
        .map_err(|e| GitBridgeError::NoRepo(e.to_string()))?;
    index
        .add_path(rel)
        .map_err(|e| GitBridgeError::NoRepo(e.to_string()))?;
    index
        .write()
        .map_err(|e| GitBridgeError::NoRepo(e.to_string()))
}

/// 全局 `merge.tool` 是否已登记为 mergedrag（未设置 / git 不可用 → false）。
pub fn merge_tool_is_mergedrag() -> Result<bool, GitBridgeError> {
    let out = Command::new("git")
        .args(["config", "--global", "--get", "merge.tool"])
        .output()
        .map_err(|_| GitBridgeError::GitNotFound)?;
    if !out.status.success() {
        return Ok(false);
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim() == "mergedrag")
}

/// Register MergeDrag as the global Git mergetool (`.gitconfig`).
///
/// `exe` should be the absolute path to the MergeDrag executable.
pub fn register_mergetool(exe: &Path) -> Result<(), GitBridgeError> {
    if find_git().is_none() {
        return Err(GitBridgeError::GitNotFound);
    }
    let run = |args: &[&str]| -> Result<(), GitBridgeError> {
        let out = Command::new("git")
            .args(args)
            .output()
            .map_err(|_| GitBridgeError::GitNotFound)?;
        if !out.status.success() {
            let msg = String::from_utf8_lossy(&out.stderr).trim().to_string();
            return Err(GitBridgeError::Command(msg));
        }
        Ok(())
    };

    run(&["config", "--global", "merge.tool", "mergedrag"])?;
    let cmd = format!(
        "\"{}\" --local \"$LOCAL\" --remote \"$REMOTE\" \
         --base \"$BASE\" --merged \"$MERGED\"",
        exe.display()
    );
    run(&["config", "--global", "mergetool.mergedrag.cmd", &cmd])?;
    run(&[
        "config",
        "--global",
        "mergetool.mergedrag.trustExitCode",
        "true",
    ])
}

/// Locate the `git` executable on PATH.
pub fn find_git() -> Option<PathBuf> {
    let candidates = ["git.exe", "git"];
    candidates.iter().find_map(|c| {
        let status = Command::new(c).arg("--version").output().ok()?;
        status.status.success().then(|| PathBuf::from(c))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sniff_binary_detects_nul() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("a.bin");
        std::fs::write(&f, b"hello\x00world").unwrap();
        assert!(sniff_binary(&f));
    }

    #[test]
    fn find_git_on_path() {
        // On any machine with git installed this passes; without git it is an
        // expected skip. We only assert it doesn't crash.
        let _ = find_git();
    }
}
