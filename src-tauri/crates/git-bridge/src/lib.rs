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

// ---- workspace: status / pull / push --------------------------------------

/// 一个仓库工作区在 UI 里需要展示的状态。
#[derive(Debug, Clone, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceStatus {
    /// git 工作树根目录（绝对路径）。
    pub root: PathBuf,
    /// 当前分支名（detached 时为 None）。
    pub branch: Option<String>,
    /// 上游跟踪信息（无上游时为 None）。
    pub upstream: Option<UpstreamInfo>,
    /// 是否正在合并/变基/cherry-pick（.git/MERGE_HEAD 等存在）。
    pub merging: bool,
    /// 未解决冲突文件数（index 中 stage != 0 的去重路径数）。
    pub unmerged_count: usize,
    /// 是否有未提交改动（不含未跟踪文件）。
    pub dirty: bool,
    /// 单行状态摘要，用于列表：如 `master ↑1 ↓2 · 2 个冲突`。
    pub summary: String,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpstreamInfo {
    /// 如 `origin/main`。
    pub name: String,
    /// 跟踪远端 URL（如 https://… 或 C:/…）。
    pub remote_url: Option<String>,
    /// 本地领先提交数。
    pub ahead: u32,
    /// 本地落后提交数。
    pub behind: u32,
}

/// 读取一个仓库（或其子目录）的工作区状态。不要求是无冲突的：合并中也能读。
pub fn repo_status(repo: &Path) -> Result<WorkspaceStatus, GitBridgeError> {
    // 先解析出真正的仓库根（工作树顶），子目录调用也能工作。
    let root = run_git(repo, &["rev-parse", "--show-toplevel"])?
        .map(|s| PathBuf::from(s.trim()))
        .ok_or_else(|| GitBridgeError::Command("不是 Git 仓库".into()))?;

    let merging = git_dir_entry_exists(&root, &["rev-parse", "--git-path", "MERGE_HEAD"])
        || git_dir_entry_exists(&root, &["rev-parse", "--git-path", "rebase-merge"])
        || git_dir_entry_exists(&root, &["rev-parse", "--git-path", "rebase-apply"])
        || git_dir_entry_exists(&root, &["rev-parse", "--git-path", "CHERRY_PICK_HEAD"]);

    let branch =
        run_git(&root, &["symbolic-ref", "--short", "-q", "HEAD"])?.map(|s| s.trim().to_string());

    let upstream = load_upstream(&root)?;

    let unmerged_count = index_unmerged_count(&root);
    let dirty = is_dirty(&root);

    let mut summary = String::new();
    if let Some(b) = &branch {
        summary.push_str(b);
    } else {
        summary.push_str("(detached)");
    }
    if let Some(u) = &upstream {
        if u.ahead > 0 || u.behind > 0 {
            summary.push(' ');
            if u.behind > 0 {
                summary.push_str(&format!("↓{}", u.behind));
            }
            if u.ahead > 0 {
                summary.push(' ');
                summary.push_str(&format!("↑{}", u.ahead));
            }
        }
    }
    if merging {
        summary.push(' ');
        summary.push_str("· 合并中");
    }
    if unmerged_count > 0 {
        summary.push(' ');
        summary.push_str(&format!("· {} 个冲突", unmerged_count));
    } else if dirty {
        summary.push(' ');
        summary.push_str("· 有改动");
    }

    Ok(WorkspaceStatus {
        root,
        branch,
        upstream,
        merging,
        unmerged_count,
        dirty,
        summary,
    })
}

fn load_upstream(root: &Path) -> Result<Option<UpstreamInfo>, GitBridgeError> {
    // 无上游分支（新建分支、detached HEAD、远端已删）是常态：git 对 @{upstream}
    // 解析失败会以非零退出并输出 fatal，这里把它当“无上游”，不能当致命错误传播，
    // 否则 repo_status 会对整张状态卡片失败。真正的基建错误（git 缺失）仍传播。
    let name = match run_git(
        root,
        &[
            "rev-parse",
            "--abbrev-ref",
            "--symbolic-full-name",
            "@{upstream}",
        ],
    ) {
        Ok(Some(s)) => s.trim().to_string(),
        Ok(None) | Err(GitBridgeError::Command(_)) => return Ok(None),
        Err(e) => return Err(e),
    };
    if name.is_empty() || name.starts_with('@') {
        return Ok(None);
    }

    // 上游远端 URL：upstream 形如 origin/main，取第一段为远端名。
    // config 查不到（非远端上游 / 未配置）同样吞掉，回退 origin。
    let remote_name = name.split('/').next().unwrap_or_default();
    let candidates = [
        format!("remote.{remote_name}.url"),
        "remote.origin.url".to_string(),
    ];
    let remote_url = candidates.iter().find_map(|key| {
        run_git(root, &["config", "--get", key])
            .ok()
            .flatten()
            .map(|s| s.trim().to_string())
    });

    // 领先/落后：`git rev-list --left-right --count HEAD...@{upstream}`
    // 输出 "ahead\tbehind"。@{upstream} 存在但远端跟踪引用缺失时命令失败 → 记 0。
    let (ahead, behind) = run_git(
        root,
        &["rev-list", "--left-right", "--count", "HEAD...@{upstream}"],
    )?
    .map(|s| {
        let parts: Vec<&str> = s.trim().split('\t').collect();
        let a = parts
            .first()
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or(0);
        let b = parts
            .get(1)
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or(0);
        (a, b)
    })
    .unwrap_or((0, 0));

    Ok(Some(UpstreamInfo {
        name,
        remote_url,
        ahead,
        behind,
    }))
}

/// `.git/MERGE_HEAD` 等「进行中的合并」标记是否存在。
fn git_dir_entry_exists(root: &Path, args: &[&str]) -> bool {
    let Ok(Some(path)) = run_git(root, args) else {
        return false;
    };
    let p = PathBuf::from(path.trim());
    let p = if p.is_absolute() { p } else { root.join(&p) };
    p.exists()
}

/// index 中未合并路径数（stage != 0，去重）。
fn index_unmerged_count(root: &Path) -> usize {
    let Ok(repo) = git2::Repository::open(root) else {
        return 0;
    };
    let Ok(index) = repo.index() else {
        return 0;
    };
    let mut seen: Vec<String> = Vec::new();
    for entry in index.iter() {
        let stage = ((entry.flags >> 12) & 0x3) as u32;
        if stage != 0 {
            let path = String::from_utf8_lossy(&entry.path).into_owned();
            if !seen.contains(&path) {
                seen.push(path);
            }
        }
    }
    seen.len()
}

/// 是否有未提交改动（忽略未跟踪 `??` 文件）。
fn is_dirty(root: &Path) -> bool {
    has_changes(root, false)
}

/// 是否有可 stash 的改动：`include_untracked` 为 true 时未跟踪文件也算。
fn has_changes(root: &Path, include_untracked: bool) -> bool {
    let Ok(Some(out)) = run_git(root, &["status", "--porcelain"]) else {
        return false;
    };
    out.lines()
        .any(|l| include_untracked || !l.starts_with("??"))
}

/// 拉取（`git pull --no-edit`）。有冲突时 git 会失败 → Err 带回冲突标记。
/// GIT_TERMINAL_PROMPT=0：绝不弹凭据提示，避免阻塞 UI。
pub fn pull(repo: &Path) -> Result<String, GitBridgeError> {
    run_git(repo, &["pull", "--no-edit"])?
        .ok_or_else(|| GitBridgeError::Command("pull 无输出".into()))
}

/// 推送（`git push`）。
pub fn push(repo: &Path) -> Result<String, GitBridgeError> {
    run_git(repo, &["push"])?.ok_or_else(|| GitBridgeError::Command("push 无输出".into()))
}

/// 更新远端跟踪引用（`git fetch --prune`），让 repo_status 的落后数反映最新远端。
pub fn fetch(repo: &Path) -> Result<String, GitBridgeError> {
    run_git(repo, &["fetch", "--prune"])?
        .ok_or_else(|| GitBridgeError::Command("fetch 无输出".into()))
}

// ---- workspace: branch / stash / commit -----------------------------------

/// 一个本地分支（UI 分支面板用）。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchInfo {
    /// refname:short，如 `master`。
    pub name: String,
    /// 当前检出的分支。
    pub current: bool,
}

/// 列出全部本地分支（名字排序，与 `git branch` 一致）。
pub fn list_branches(repo: &Path) -> Result<Vec<BranchInfo>, GitBridgeError> {
    let out = run_git(repo, &["branch", "--format=%(refname:short)"])?.unwrap_or_default();
    let current = run_git(repo, &["rev-parse", "--abbrev-ref", "HEAD"])?
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "HEAD".into());
    Ok(out
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let name = l.trim().to_string();
            BranchInfo {
                current: name == current,
                name,
            }
        })
        .collect())
}

/// 新建本地分支于当前 HEAD（不切换）。名字校验交给 git，错误原样回报。
pub fn create_branch(repo: &Path, name: &str) -> Result<(), GitBridgeError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(GitBridgeError::Command("分支名不能为空".into()));
    }
    run_git(repo, &["branch", name])?;
    Ok(())
}

/// 切换到分支 `name`。`force` 为 true 时带 `-f` 丢弃未提交改动。
/// 返回 checkout 输出（切换提示或错误信息）。
pub fn switch_branch(repo: &Path, name: &str, force: bool) -> Result<String, GitBridgeError> {
    let mut args: Vec<&str> = vec!["checkout"];
    if force {
        args.push("-f");
    }
    args.push(name);
    Ok(run_git(repo, &args)?.unwrap_or_default())
}

/// 一条 stash（UI 暂存面板用）。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StashEntry {
    /// 列表序号（0 = 最近一条），即 `stash@{n}` 的 n。
    pub index: usize,
    /// stash 描述，如 `WIP on master: 1234abc 提交信息`。
    pub message: String,
}

/// 列出 stash（`git stash list --format=%gs`），index 与 `stash@{n}` 对应。
pub fn list_stashes(repo: &Path) -> Result<Vec<StashEntry>, GitBridgeError> {
    let out = run_git(repo, &["stash", "list", "--format=%gs"])?.unwrap_or_default();
    Ok(out
        .lines()
        .filter(|l| !l.trim().is_empty())
        .enumerate()
        .map(|(i, l)| StashEntry {
            index: i,
            message: l.trim().to_string(),
        })
        .collect())
}

/// 保存当前改动到 stash。`message` 为空用 git 自动生成（WIP on …）。
/// `include_untracked` 为 true 时未跟踪文件也一并保存（`git stash push -u`）。
/// 无可存改动（干净工作树，或仅剩未跟踪文件而未开启 -u）时报可读错误。
pub fn stash_push(
    repo: &Path,
    message: &str,
    include_untracked: bool,
) -> Result<String, GitBridgeError> {
    if !has_changes(repo, include_untracked) {
        return Err(GitBridgeError::Command("没有可保存的改动".into()));
    }
    let mut args: Vec<&str> = vec!["stash", "push"];
    if include_untracked {
        args.push("-u");
    }
    let msg = message.trim();
    if !msg.is_empty() {
        args.push("-m");
        args.push(msg);
    }
    Ok(run_git(repo, &args)?.unwrap_or_default())
}

/// 恢复最近一条 stash 并删除它。
pub fn stash_pop(repo: &Path) -> Result<String, GitBridgeError> {
    Ok(run_git(repo, &["stash", "pop"])?.unwrap_or_default())
}

/// 恢复第 `index` 条 stash 并删除它。
pub fn stash_pop_index(repo: &Path, index: usize) -> Result<String, GitBridgeError> {
    let which = format!("stash@{{{index}}}");
    Ok(run_git(repo, &["stash", "pop", &which])?.unwrap_or_default())
}

/// 删除第 `index` 条 stash。
pub fn stash_drop(repo: &Path, index: usize) -> Result<(), GitBridgeError> {
    let which = format!("stash@{{{index}}}");
    run_git(repo, &["stash", "drop", &which])?;
    Ok(())
}

/// 暂存并提交全部改动（`git add -A` + `git commit -m`）。
/// 返回 `短hash 标题` 供 UI 展示；提交信息为空或无事可提交时报错。
pub fn commit_all(repo: &Path, message: &str) -> Result<String, GitBridgeError> {
    let msg = message.trim();
    if msg.is_empty() {
        return Err(GitBridgeError::Command("提交信息不能为空".into()));
    }
    run_git(repo, &["add", "-A"])?;
    run_git(repo, &["commit", "-m", msg])?;
    let summary = run_git(repo, &["log", "-1", "--format=%h %s"])?.unwrap_or_default();
    Ok(summary)
}

// ---- workspace: 收尾进行中的合并 / 逐文件提交 --------------------------------

/// 完成进行中的合并（`git merge --continue`，用 git 自动生成的合并信息）。
/// 要求所有冲突已解决（无未合并文件）；不在合并中时报可读错误。
pub fn merge_continue(repo: &Path) -> Result<String, GitBridgeError> {
    if !git_dir_entry_exists(repo, &["rev-parse", "--git-path", "MERGE_HEAD"]) {
        return Err(GitBridgeError::Command("当前没有进行中的合并".into()));
    }
    if index_unmerged_count(repo) > 0 {
        return Err(GitBridgeError::Command(
            "还有未解决的冲突文件，请先解决后再继续合并".into(),
        ));
    }
    Ok(run_git(repo, &["merge", "--continue"])?.unwrap_or_default())
}

/// 放弃进行中的合并（`git merge --abort`，回到合并前状态）。
pub fn merge_abort(repo: &Path) -> Result<String, GitBridgeError> {
    if !git_dir_entry_exists(repo, &["rev-parse", "--git-path", "MERGE_HEAD"]) {
        return Err(GitBridgeError::Command("当前没有进行中的合并".into()));
    }
    Ok(run_git(repo, &["merge", "--abort"])?.unwrap_or_default())
}

/// 一条工作树改动（提交面板勾选用）。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeEntry {
    /// 仓库相对路径。重命名显示为 `旧 -> 新`，提交时两边都会 add。
    pub path: String,
    /// 状态码字母：M 修改 / A 新增 / D 删除 / R 重命名 / ? 未跟踪
    pub status: String,
}

/// 列出所有未提交改动（`git status --porcelain`）。
pub fn list_changes(repo: &Path) -> Result<Vec<ChangeEntry>, GitBridgeError> {
    let out = run_git_aligned(repo, &["status", "--porcelain"])?.unwrap_or_default();
    Ok(out
        .lines()
        .filter(|l| l.len() >= 4)
        .map(|l| {
            // 形如 ` M path` / `?? path` / `R  old -> new`；第 4 个字符起是路径。
            let code = &l[0..2];
            let path = l[3..].trim();
            // 工作树变化优先于暂存变化：取代码中「非空格」的那个字母。
            let status = {
                let staged = code.chars().next().unwrap_or(' ');
                let work = code.chars().nth(1).unwrap_or(' ');
                if work != ' ' {
                    work
                } else {
                    staged
                }
            }
            .to_string();
            ChangeEntry {
                path: path.to_string(),
                status,
            }
        })
        .collect())
}

/// 只暂存并提交选中的文件（`git add <paths...>` + `git commit -m`）。
/// 重命名路径（`旧 -> 新`）会拆成两个路径一起 add。返回 `短hash 标题`。
pub fn commit_files(
    repo: &Path,
    paths: Vec<String>,
    message: &str,
) -> Result<String, GitBridgeError> {
    let msg = message.trim();
    if msg.is_empty() {
        return Err(GitBridgeError::Command("提交信息不能为空".into()));
    }
    // 展开重命名条目，并去重。
    let mut expanded: Vec<String> = Vec::new();
    for p in &paths {
        if p.contains(" -> ") {
            let parts: Vec<&str> = p.split(" -> ").collect();
            for part in &parts {
                let s = part.trim().to_string();
                if !expanded.contains(&s) {
                    expanded.push(s);
                }
            }
        } else {
            let s = p.trim().to_string();
            if !expanded.contains(&s) {
                expanded.push(s);
            }
        }
    }
    if expanded.is_empty() {
        return Err(GitBridgeError::Command("请选择要提交的文件".into()));
    }
    let mut args: Vec<&str> = vec!["add", "--"];
    args.extend(expanded.iter().map(|s| s.as_str()));
    run_git(repo, &args)?;
    run_git(repo, &["commit", "-m", msg])?;
    let summary = run_git(repo, &["log", "-1", "--format=%h %s"])?.unwrap_or_default();
    Ok(summary)
}

/// 在 `repo` 里跑一条无交互 git 命令；成功返回合并后的 stdout+stderr 文本
/// （stdout 整体 `trim`，供普通文本消息使用）。
fn run_git(repo: &Path, args: &[&str]) -> Result<Option<String>, GitBridgeError> {
    run_git_impl(repo, args, true)
}

/// 同上，但保留 stdout 原样（只去掉结尾换行）。`git status --porcelain` 这类
/// 布局敏感的文本必须用它：整体 trim 会把首行状态列 ` X` 的前导空格吃掉，
/// 导致路径切片从第 4 个字符起（丢掉路径首字符）。
fn run_git_aligned(repo: &Path, args: &[&str]) -> Result<Option<String>, GitBridgeError> {
    run_git_impl(repo, args, false)
}

fn run_git_impl(
    repo: &Path,
    args: &[&str],
    trim_stdout: bool,
) -> Result<Option<String>, GitBridgeError> {
    let out = Command::new("git")
        .args(["-C"])
        .arg(repo)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .map_err(|_| GitBridgeError::GitNotFound)?;
    let mut text = if trim_stdout {
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    } else {
        let mut s = String::from_utf8_lossy(&out.stdout).into_owned();
        while s.ends_with('\n') || s.ends_with('\r') {
            s.pop();
        }
        s
    };
    if !out.stderr.is_empty() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        if !text.is_empty() {
            text.push('\n');
        }
        text.push_str(&err);
    }
    if text.is_empty() {
        return Ok(None);
    }
    if !out.status.success() {
        return Err(GitBridgeError::Command(text));
    }
    Ok(Some(text))
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
