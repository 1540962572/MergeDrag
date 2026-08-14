//! Tauri commands exposed to the frontend.

use std::path::{Path, PathBuf};

use git_bridge::sniff_binary;
use merge_core::{three_way, Decision, MergeDocument};
use serde::Serialize;
use tauri::{Manager, State};

use crate::{canonical_abs, AppState};

/// A conflicted file shown in the left list.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictFileDto {
    pub path: String,
    pub is_binary: bool,
    pub unresolved_count: u32,
}

/// The snapshot handed to the UI after opening a session or editing a hunk.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSnapshot {
    pub repo_root: Option<String>,
    pub files: Vec<ConflictFileDto>,
    pub current: Option<MergeDocument>,
    pub message: Option<String>,
}

#[tauri::command]
pub fn open_session(state: State<'_, AppState>) -> Result<SessionSnapshot, String> {
    let launch = &state.launch;
    if !launch.is_mergetool_launch() {
        return Ok(SessionSnapshot {
            repo_root: None,
            files: vec![],
            current: None,
            message: Some("请通过 git mergetool 启动，或加载示例冲突。".into()),
        });
    }

    // git mergetool 传的是相对路径（相对本进程 CWD），先规范化成绝对路径再读，
    // 并让 save_target 存绝对形式，repo_rel_of 才能反推相对路径并 git add。
    let local = read_text(&canonical_abs(launch.local.as_ref().unwrap())?)?;
    let remote = read_text(&canonical_abs(launch.remote.as_ref().unwrap())?)?;
    let base = match &launch.base {
        Some(p) => canonical_abs(p).ok().and_then(|p| read_text(&p).ok()),
        _ => None,
    };

    let merged_path = canonical_abs(launch.merged.as_ref().unwrap())?;
    *state.save_target.lock().map_err(|e| e.to_string())? = Some(merged_path.clone());
    let file_label = merged_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("merged")
        .to_string();

    let doc = three_way(&local, &remote, base.as_deref(), file_label.clone());
    let unresolved = doc.unresolved_count() as u32;
    set_current(&state, doc.clone());

    Ok(SessionSnapshot {
        repo_root: repo_root_of(&merged_path),
        files: vec![ConflictFileDto {
            path: file_label,
            is_binary: sniff_binary(&merged_path),
            unresolved_count: unresolved,
        }],
        current: Some(doc),
        message: None,
    })
}

#[tauri::command]
pub fn load_sample(state: State<'_, AppState>) -> Result<SessionSnapshot, String> {
    // 3 处相互独立的冲突（README 的简介/安装/使用三段），对应前端 sample.ts。
    let (local, remote, base) = sample_trio();

    let doc = three_way(&local, &remote, Some(&base), "README.md".to_string());
    let unresolved = doc.unresolved_count() as u32;
    set_current(&state, doc.clone());

    Ok(SessionSnapshot {
        repo_root: None,
        files: vec![ConflictFileDto {
            path: "README.md".into(),
            is_binary: false,
            unresolved_count: unresolved,
        }],
        current: Some(doc),
        message: Some("已加载示例冲突（3 处，可逐个取左/取右）".into()),
    })
}

/// Update one conflict hunk's decision and return the refreshed document.
/// 决策会写回 `state.current`，后续 save_merged 才能按最新未解决数决定是否 git add。
#[tauri::command]
pub fn set_decision(
    state: State<'_, AppState>,
    hunk_id: u64,
    decision: Decision,
) -> Result<MergeDocument, String> {
    state.apply_decision(hunk_id, decision)
}

/// 保存结果：是否已 `git add`；warning 存放 add 失败等非致命信息。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveResult {
    pub added: bool,
    pub warning: Option<String>,
}

/// 把最终文本写入当前文件的保存目标；写盘后才允许退出码为 0。
/// 全部解决时顺便 `git add`（把文件移出 unmerged 状态）；未解决时只写盘。
#[tauri::command]
pub fn save_merged(state: State<'_, AppState>, text: String) -> Result<SaveResult, String> {
    let target = state.persist(&text)?;

    let unresolved = state
        .current
        .lock()
        .map(|g| g.as_ref().map(|d| d.unresolved_count()).unwrap_or(1))
        .unwrap_or(1);

    let mut added = false;
    let mut warning = None;
    if unresolved == 0 {
        if let Some((repo, rel)) = repo_rel_of(&target) {
            match git_bridge::stage_file(&repo, &rel) {
                Ok(()) => added = true,
                Err(e) => warning = Some(format!("git add 失败: {e}")),
            }
        }
    }
    Ok(SaveResult { added, warning })
}

/// 扫描当前仓库的所有未合并文件，返回左栏列表。
#[tauri::command]
pub fn scan_repo(state: State<'_, AppState>) -> Result<SessionSnapshot, String> {
    let root =
        repo_root_for_scan(&state).ok_or("未打开 Git 仓库（无工作空间，也非 mergetool 启动）")?;
    let files = scan_files(Path::new(&root))?;
    let current = state.current.lock().map_err(|e| e.to_string())?.clone();
    Ok(SessionSnapshot {
        repo_root: Some(root),
        files,
        current,
        message: None,
    })
}

/// 从仓库 index 读取指定冲突文件的三方内容并打开。
#[tauri::command]
pub fn open_file(state: State<'_, AppState>, rel_path: String) -> Result<SessionSnapshot, String> {
    let root =
        repo_root_for_scan(&state).ok_or("未打开 Git 仓库（无工作空间，也非 mergetool 启动）")?;
    let root = PathBuf::from(&root);

    let doc = three_way_from_index(&root, &rel_path)?;
    let files = scan_files(&root)?;
    set_current(&state, doc.clone());
    *state.save_target.lock().map_err(|e| e.to_string())? = Some(root.join(&rel_path));

    Ok(SessionSnapshot {
        repo_root: Some(root.to_string_lossy().into_owned()),
        files,
        current: Some(doc),
        message: None,
    })
}

/// 首次启动自动登记：已登记过则直接报现状；`merge.tool` 被用户改走则提示可手动重登。
/// 只在空壳启动（非 mergetool 调用）时由前端调用。
#[tauri::command]
pub fn ensure_mergetool_registered(app: tauri::AppHandle) -> Result<String, String> {
    let registered = git_bridge::merge_tool_is_mergedrag().map_err(|e| e.to_string())?;
    if registered {
        return Ok("已登记为系统 Git mergetool（merge.tool = mergedrag）。".into());
    }
    let marker = app
        .path()
        .app_data_dir()
        .map(|d| d.join("mergetool.registered"))
        .unwrap_or_default();
    if marker.exists() {
        return Ok("登记过但全局 merge.tool 被改动；可点击「重新登记」。".into());
    }
    let exe = std::env::current_exe().map_err(|e| format!("找不到可执行文件路径: {e}"))?;
    git_bridge::register_mergetool(&exe).map_err(|e| e.to_string())?;
    if let Some(dir) = marker.parent() {
        let _ = std::fs::create_dir_all(dir);
        let _ = std::fs::write(&marker, "1");
    }
    Ok(format!(
        "已登记为系统 Git mergetool：{}。\n冲突时执行 git mergetool 即可使用 MergeDrag。",
        exe.display()
    ))
}

/// 手动强制登记（按钮触发）：不管是否登记过都写一次全局 git config。
#[tauri::command]
pub fn register_mergetool(_app: tauri::AppHandle) -> Result<String, String> {
    let exe = std::env::current_exe().map_err(|e| format!("找不到可执行文件路径: {e}"))?;
    git_bridge::register_mergetool(&exe).map_err(|e| e.to_string())?;
    Ok(format!("已登记为系统 Git mergetool：{}。", exe.display()))
}

// ---- helpers -------------------------------------------------------------

/// 示例冲突三方文本（3 处独立冲突：简介/安装/使用），与前端 sample.ts 保持一致。
fn sample_trio() -> (String, String, String) {
    let (mut local, mut remote, mut base) = (String::new(), String::new(), String::new());
    for (head, l, r, b) in [
        (
            "# MergeDrag\n\n## 简介\n\n",
            "这是一个三方合并工具（本地版）。\n",
            "这是一个三方合并工具（远端版）。\n",
            "这是一个 IDEA 风格的三方合并工具。\n",
        ),
        (
            "\n## 安装\n\n",
            "使用 NSIS 安装包。\n",
            "使用 MSI 安装包。\n",
            "使用安装包。\n",
        ),
        (
            "\n## 使用\n\n",
            "用 git mergetool 启动（本地）。\n",
            "用 git mergetool 启动（远端）。\n",
            "用 git mergetool 启动。\n",
        ),
    ] {
        // 标题/段落分隔属于 clean 上下文，冲突行各侧取各自的改法。
        local.push_str(head);
        remote.push_str(head);
        base.push_str(head);
        local.push_str(l);
        remote.push_str(r);
        base.push_str(b);
    }
    let tail = "\n## 反馈\n\n欢迎反馈问题。\n";
    local.push_str(tail);
    remote.push_str(tail);
    base.push_str(tail);
    (local, remote, base)
}

fn read_text(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| format!("读取 {} 失败: {e}", path.display()))
}

/// 从 `--merged` 往上找仓库工作目录。
fn repo_root_of(from: &Path) -> Option<String> {
    let repo = git2::Repository::discover(from).ok()?;
    repo.workdir().map(|p| p.to_string_lossy().into_owned())
}

/// 从保存目标反推（仓库工作目录, 工作树内相对路径）；不在工作树内则 None。
fn repo_rel_of(target: &Path) -> Option<(PathBuf, PathBuf)> {
    // 先规范化：git mergetool 传的 $MERGED 可能是相对路径（相对进程 CWD），
    // workdir 是绝对路径，直接 strip_prefix 会因形式不同而失配。
    let target = canonical_abs(target).ok()?;
    let repo = git2::Repository::discover(&target).ok()?;
    let wd = repo.workdir()?.to_path_buf();
    let rel = target.strip_prefix(&wd).ok()?.to_path_buf();
    Some((wd, rel))
}

fn set_current(state: &State<'_, AppState>, doc: MergeDocument) {
    if let Ok(mut g) = state.current.lock() {
        *g = Some(doc);
    }
}

/// 从 index 读 stage 1/2/3 的 blob 文本并三方合并。
fn three_way_from_index(repo_root: &Path, rel: &str) -> Result<MergeDocument, String> {
    let repo = git2::Repository::open(repo_root).map_err(|e| format!("打开仓库失败: {e}"))?;
    let index = repo.index().map_err(|e| format!("读取 index 失败: {e}"))?;
    let blob_text = |stage: i32| -> Option<Result<String, String>> {
        let entry = index.get_path(Path::new(rel), stage)?;
        let blob = repo.find_blob(entry.id).ok()?;
        Some(
            String::from_utf8(blob.content().to_vec()).map_err(|_| format!("{rel} 不是有效 UTF-8")),
        )
    };
    let local = blob_text(2).transpose()?.unwrap_or_default();
    let remote = blob_text(3).transpose()?.unwrap_or_default();
    let base = blob_text(1).transpose()?;

    Ok(three_way(&local, &remote, base.as_deref(), rel.to_string()))
}

/// 列出仓库全部未合并文件及各自的未解决数。
fn scan_files(repo_root: &Path) -> Result<Vec<ConflictFileDto>, String> {
    let repo = git2::Repository::open(repo_root).map_err(|e| format!("打开仓库失败: {e}"))?;
    let index = repo.index().map_err(|e| format!("读取 index 失败: {e}"))?;

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

    let mut out = Vec::new();
    for path in seen {
        match three_way_from_index(repo_root, &path) {
            Ok(doc) => out.push(ConflictFileDto {
                path: path.clone(),
                is_binary: sniff_binary(&repo_root.join(&path)),
                unresolved_count: doc.unresolved_count() as u32,
            }),
            // 非 UTF-8 / 无三方内容：标记为二进制，不进入三栏。
            Err(_) => out.push(ConflictFileDto {
                path: path.clone(),
                is_binary: true,
                unresolved_count: 0,
            }),
        }
    }
    Ok(out)
}

// ---- workspace: status / pull / push / recent -------------------------------

/// 最近打开过的工作空间（持久化到 app_data_dir/workspaces.json，最近优先，最多 10 条）。
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentWorkspace {
    pub path: String,
    /// Unix 秒。
    pub last_opened: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceSnapshot {
    pub status: Option<git_bridge::WorkspaceStatus>,
    pub recent: Vec<RecentWorkspace>,
    pub active_path: Option<String>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PullOutcome {
    pub message: String,
    pub status: Option<git_bridge::WorkspaceStatus>,
    pub conflicted: bool,
}

/// 打开一个工作空间：校验是 git 仓库、设为当前工作区、记入最近列表。
#[tauri::command]
pub fn open_workspace(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    root: String,
) -> Result<WorkspaceSnapshot, String> {
    let status = git_bridge::repo_status(Path::new(&root)).map_err(|e| e.to_string())?;
    *state.workspace_root.lock().map_err(|e| e.to_string())? = Some(status.root.clone());
    let recent = push_recent(&app, &status.root)?;
    Ok(WorkspaceSnapshot {
        status: Some(status),
        recent,
        active_path: None,
        message: None,
    })
}

/// 刷新当前工作区：先 fetch 更新远端跟踪引用，再读一次状态。
/// 无远端/离线时 fetch 失败不致命——落后数反映上次 fetch 的结果。
#[tauri::command]
pub fn refresh_workspace(state: State<'_, AppState>) -> Result<WorkspaceSnapshot, String> {
    let root = current_workspace_root(&state)?;
    let _ = git_bridge::fetch(&root);
    let status = git_bridge::repo_status(&root).map_err(|e| e.to_string())?;
    Ok(WorkspaceSnapshot {
        status: Some(status),
        recent: vec![],
        active_path: None,
        message: None,
    })
}

/// `git pull --no-edit`；有冲突时 pull 报错但仍返回状态，UI 据此切到解决冲突视图。
#[tauri::command]
pub fn pull_now(state: State<'_, AppState>) -> Result<PullOutcome, String> {
    let root = current_workspace_root(&state)?;
    let message = match git_bridge::pull(&root) {
        Ok(m) => m,
        Err(e) => e.to_string(),
    };
    let status = git_bridge::repo_status(&root).ok();
    let conflicted = status
        .as_ref()
        .map(|s| s.merging || s.unmerged_count > 0)
        .unwrap_or(false);
    Ok(PullOutcome {
        message,
        status,
        conflicted,
    })
}

/// `git push`。
#[tauri::command]
pub fn push_now(state: State<'_, AppState>) -> Result<String, String> {
    let root = current_workspace_root(&state)?;
    git_bridge::push(&root).map_err(|e| e.to_string())
}

/// 最近打开过的工作空间列表。
#[tauri::command]
pub fn list_workspaces(app: tauri::AppHandle) -> Result<Vec<RecentWorkspace>, String> {
    read_recent(&app)
}

// ---- workspace: branch / stash / commit -------------------------------------

/// 列出全部本地分支。
#[tauri::command]
pub fn list_branches(state: State<'_, AppState>) -> Result<Vec<git_bridge::BranchInfo>, String> {
    let root = current_workspace_root(&state)?;
    git_bridge::list_branches(&root).map_err(|e| e.to_string())
}

/// 新建本地分支（不切换）。返回确认文案。
#[tauri::command]
pub fn create_branch(state: State<'_, AppState>, name: String) -> Result<String, String> {
    let root = current_workspace_root(&state)?;
    git_bridge::create_branch(&root, &name).map_err(|e| e.to_string())?;
    Ok(format!("已创建分支 {name}"))
}

/// 切换到分支 `name`；force=true 丢弃未提交改动。返回 git 切换输出。
#[tauri::command]
pub fn switch_branch(
    state: State<'_, AppState>,
    name: String,
    force: bool,
) -> Result<String, String> {
    let root = current_workspace_root(&state)?;
    git_bridge::switch_branch(&root, &name, force).map_err(|e| e.to_string())
}

/// 列出 stash。
#[tauri::command]
pub fn list_stashes(state: State<'_, AppState>) -> Result<Vec<git_bridge::StashEntry>, String> {
    let root = current_workspace_root(&state)?;
    git_bridge::list_stashes(&root).map_err(|e| e.to_string())
}

/// 保存改动到 stash（message 可空，空则用 git 自动生成的 WIP on …）。
/// `include_untracked` 为 true 时未跟踪文件也一并保存（`git stash push -u`）。
#[tauri::command]
pub fn stash_push(
    state: State<'_, AppState>,
    message: Option<String>,
    include_untracked: Option<bool>,
) -> Result<String, String> {
    let root = current_workspace_root(&state)?;
    let msg = message.unwrap_or_default();
    git_bridge::stash_push(&root, &msg, include_untracked.unwrap_or(false)).map_err(|e| e.to_string())
}

/// 恢复第 `index` 条 stash（缺省 0 = 最近一条）并从列表移除。返回 git 输出。
#[tauri::command]
pub fn stash_pop(state: State<'_, AppState>, index: Option<usize>) -> Result<String, String> {
    let root = current_workspace_root(&state)?;
    match index.unwrap_or(0) {
        0 => git_bridge::stash_pop(&root),
        n => git_bridge::stash_pop_index(&root, n),
    }
    .map_err(|e| e.to_string())
}

/// 删除第 `index` 条 stash。
#[tauri::command]
pub fn stash_drop(state: State<'_, AppState>, index: usize) -> Result<String, String> {
    let root = current_workspace_root(&state)?;
    git_bridge::stash_drop(&root, index).map_err(|e| e.to_string())?;
    Ok(format!("已删除 stash@{index}"))
}

/// 暂存并提交全部改动（`git add -A` + `git commit -m`）。返回 `短hash 标题`。
#[tauri::command]
pub fn commit_all(state: State<'_, AppState>, message: String) -> Result<String, String> {
    let root = current_workspace_root(&state)?;
    git_bridge::commit_all(&root, &message).map_err(|e| e.to_string())
}

/// 列出全部未提交改动（提交面板勾选用，含未跟踪/删除/重命名）。
#[tauri::command]
pub fn list_changes(state: State<'_, AppState>) -> Result<Vec<git_bridge::ChangeEntry>, String> {
    let root = current_workspace_root(&state)?;
    git_bridge::list_changes(&root).map_err(|e| e.to_string())
}

/// 只暂存并提交选中的文件。重命名路径（`旧 -> 新`）提交时两边都会 add。
#[tauri::command]
pub fn commit_files(
    state: State<'_, AppState>,
    paths: Vec<String>,
    message: String,
) -> Result<String, String> {
    let root = current_workspace_root(&state)?;
    git_bridge::commit_files(&root, paths, &message).map_err(|e| e.to_string())
}

/// 完成进行中的合并（`git merge --continue`）。要求所有冲突已解决，否则报可读错误。
#[tauri::command]
pub fn merge_continue(state: State<'_, AppState>) -> Result<String, String> {
    let root = current_workspace_root(&state)?;
    git_bridge::merge_continue(&root).map_err(|e| e.to_string())
}

/// 放弃进行中的合并（`git merge --abort`），回到合并前状态。
#[tauri::command]
pub fn merge_abort(state: State<'_, AppState>) -> Result<String, String> {
    let root = current_workspace_root(&state)?;
    git_bridge::merge_abort(&root).map_err(|e| e.to_string())
}

// ---- workspace helpers ------------------------------------------------------

/// 当前仓库根（供 scan/open_file 用）：优先工作空间，其次 mergetool 的 --merged 反推。
fn repo_root_for_scan(state: &State<'_, AppState>) -> Option<String> {
    if let Ok(guard) = state.workspace_root.lock() {
        if let Some(p) = guard.as_ref() {
            return Some(p.to_string_lossy().into_owned());
        }
    }
    let merged = state.launch.merged.as_ref()?;
    repo_root_of(merged)
}

fn current_workspace_root(state: &State<'_, AppState>) -> Result<PathBuf, String> {
    state
        .workspace_root
        .lock()
        .map_err(|e| e.to_string())?
        .clone()
        .ok_or_else(|| "尚未打开工作区".to_string())
}

fn recent_file(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("取 app_data_dir 失败: {e}"))?;
    Ok(dir.join("workspaces.json"))
}

fn read_recent(app: &tauri::AppHandle) -> Result<Vec<RecentWorkspace>, String> {
    let path = recent_file(app)?;
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Ok(Vec::new());
    };
    serde_json::from_str(&text).map_err(|e| format!("解析最近工作区失败: {e}"))
}

fn push_recent(app: &tauri::AppHandle, root: &Path) -> Result<Vec<RecentWorkspace>, String> {
    let mut list = read_recent(app)?;
    let path = root.to_string_lossy().into_owned();
    list.retain(|w| w.path != path);
    list.insert(
        0,
        RecentWorkspace {
            path,
            last_opened: now_unix_seconds(),
        },
    );
    list.truncate(10);
    let path = recent_file(app)?;
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    std::fs::write(
        &path,
        serde_json::to_string_pretty(&list).map_err(|e| e.to_string())?,
    )
    .map_err(|e| format!("写入最近工作区失败: {e}"))?;
    Ok(list)
}

fn now_unix_seconds() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Command, Output};

    fn git(dir: &Path, args: &[&str]) -> Output {
        Command::new("git")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .args([
                "-c",
                "init.defaultBranch=master",
                "-c",
                "core.autocrlf=false",
                "-C",
            ])
            .arg(dir)
            .args(args)
            .output()
            .expect("需要 git 可执行文件在 PATH 上")
    }

    fn run_ok(dir: &Path, args: &[&str]) -> String {
        let out = git(dir, args);
        assert!(
            out.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    fn commit(dir: &Path, msg: &str) {
        run_ok(
            dir,
            &[
                "-c",
                "user.name=md",
                "-c",
                "user.email=md@test",
                "commit",
                "--allow-empty",
                "-m",
                msg,
            ],
        );
    }

    /// 造一个真实冲突仓库（中间行替换 + 末尾双方追加 = 2 个冲突）。
    fn conflicted_repo() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        run_ok(root, &["init"]);
        commit(root, "根提交");
        std::fs::write(root.join("data.txt"), "a\nb\nc\n").unwrap();
        run_ok(root, &["add", "data.txt"]);
        commit(root, "base");

        run_ok(root, &["checkout", "-b", "side"]);
        std::fs::write(root.join("data.txt"), "a\nSIDE\nc\nSIDE-APPEND\n").unwrap();
        run_ok(root, &["add", "data.txt"]);
        commit(root, "side");

        run_ok(root, &["checkout", "master"]);
        std::fs::write(root.join("data.txt"), "a\nMAIN\nc\nMAIN-APPEND\n").unwrap();
        run_ok(root, &["add", "data.txt"]);
        commit(root, "main");

        let merged = git(root, &["merge", "side"]);
        assert!(!merged.status.success(), "merge 应当产生冲突");
        tmp
    }

    #[test]
    fn scan_files_finds_unmerged_with_counts() {
        let tmp = conflicted_repo();
        let files = scan_files(tmp.path()).unwrap();
        assert_eq!(files.len(), 1, "应只扫描出一个冲突文件");
        assert_eq!(files[0].path, "data.txt");
        assert!(!files[0].is_binary, "文本文件不应标记为二进制");
        assert_eq!(files[0].unresolved_count, 2);
    }

    #[test]
    fn three_way_from_index_reads_all_stages() {
        let tmp = conflicted_repo();
        let doc = three_way_from_index(tmp.path(), "data.txt").unwrap();
        assert_eq!(doc.file_label, "data.txt");
        assert_eq!(doc.unresolved_count(), 2, "中间行 + 末尾双方追加");
        let applied = doc.apply();
        assert!(applied.contains("MAIN") && applied.contains("SIDE"));
    }

    #[test]
    fn sample_trio_produces_three_conflicts() {
        let (local, remote, base) = sample_trio();
        let doc = three_way(&local, &remote, Some(&base), "README.md".to_string());
        assert_eq!(
            doc.unresolved_count(),
            3,
            "示例应生成 3 处独立冲突：\n{hunks}",
            hunks = doc
                .hunks
                .iter()
                .map(|h| match h {
                    merge_core::Hunk::Clean { .. } => "clean".to_string(),
                    merge_core::Hunk::Conflict { id, decision, .. } => {
                        format!("conflict#{id}: {decision:?}")
                    }
                })
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
}
