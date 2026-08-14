mod commands;
mod launch;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use clap::Parser;
use launch::LaunchArgs;
use merge_core::{Decision, MergeDocument};
use tauri::Manager;

/// Shared state managed by Tauri.
pub struct AppState {
    pub launch: LaunchArgs,
    pub current: Mutex<Option<MergeDocument>>,
    /// 当前文件保存目标（mergetool 启动时为 --merged；多文件时为工作树路径）。
    pub save_target: Mutex<Option<PathBuf>>,
    /// 当前打开的工作空间（本地含 .git 的文件夹），空壳启动时使用。
    pub workspace_root: Mutex<Option<PathBuf>>,
    /// 是否已把最终文本写入磁盘（决定退出码）。
    pub saved: AtomicBool,
}

impl AppState {
    fn new(launch: LaunchArgs) -> Self {
        let save_target = launch.merged.clone();
        Self {
            launch,
            current: Mutex::new(None),
            save_target: Mutex::new(save_target),
            workspace_root: Mutex::new(None),
            saved: AtomicBool::new(false),
        }
    }

    /// 把最终文本写入当前保存目标，写盘成功后标记已保存（决定退出码）。
    pub fn persist(&self, text: &str) -> Result<PathBuf, String> {
        let target = self
            .save_target
            .lock()
            .map_err(|e| e.to_string())?
            .clone()
            .or_else(|| self.launch.merged.clone())
            .ok_or_else(|| "没有可保存的目标路径".to_string())?;
        // 规范化成真实绝对路径：git mergetool 传的是相对 $MERGED（相对本进程 CWD），
        // 直接写盘没问题，但 repo_rel_of 要按 workdir 反推相对路径，必须用绝对路径。
        // 目标文件本身可能还不存在（首次保存就是由 persist 创建），
        // 不能直接 canonicalize 整个路径——改成规范化必然存在的父目录再拼文件名。
        let target = canonical_abs(&target).unwrap_or_else(|_| {
            match (target.parent(), target.file_name()) {
                (Some(dir), Some(name)) => canonical_abs(dir)
                    .unwrap_or_else(|_| dir.to_path_buf())
                    .join(name),
                _ => target.to_path_buf(),
            }
        });
        std::fs::write(&target, text)
            .map_err(|e| format!("写入 {} 失败: {e}", target.display()))?;
        self.saved.store(true, Ordering::SeqCst);
        Ok(target)
    }

    /// 应用一处冲突块决策并写回 `state.current`，返回新文档。
    /// 不写回的话 save_merged 会按旧文档算未解决数，导致全解决后也不 git add。
    pub fn apply_decision(
        &self,
        hunk_id: u64,
        decision: Decision,
    ) -> Result<MergeDocument, String> {
        let mut guard = self.current.lock().map_err(|e| e.to_string())?;
        let doc = guard.as_ref().ok_or_else(|| "尚未打开文件".to_string())?;
        let decided = doc.with_decision(hunk_id, decision);
        *guard = Some(decided.clone());
        Ok(decided)
    }
}

/// mergetool 协议（trustExitCode）要求的退出码：已保存且全部解决 → 0，否则 → 1。
pub fn exit_code_for(saved: bool, unresolved: usize) -> i32 {
    if saved && unresolved == 0 {
        0
    } else {
        1
    }
}

/// 把任意路径解析成真实绝对路径；Windows 上剥掉 `\\?\` 前缀，
/// 否则它和 git2 workdir（普通 `C:\...` 形式）做 strip_prefix 会失配。
pub(crate) fn canonical_abs(p: &Path) -> Result<PathBuf, String> {
    let c = std::fs::canonicalize(p).map_err(|e| format!("解析路径 {} 失败: {e}", p.display()))?;
    #[cfg(windows)]
    let c = PathBuf::from(c.to_string_lossy().trim_start_matches(r"\\?\"));
    Ok(c)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let args = LaunchArgs::parse();

    tauri::Builder::default()
        .plugin(tauri_plugin_log::Builder::default().build())
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::new(args))
        .invoke_handler(tauri::generate_handler![
            commands::open_session,
            commands::load_sample,
            commands::set_decision,
            commands::save_merged,
            commands::scan_repo,
            commands::open_file,
            commands::ensure_mergetool_registered,
            commands::register_mergetool,
            // 工作空间
            commands::open_workspace,
            commands::refresh_workspace,
            commands::pull_now,
            commands::push_now,
            commands::list_workspaces,
        ])
        // 关闭窗口时按 mergetool 协议定退出码：已保存且全部解决 → 0；否则 → 1。
        // 非 mergetool 启动（工作空间/示例）不参与该协议，直接退出码 0。
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let app = window.app_handle();
                let state = app.state::<AppState>();
                let code = if state.launch.is_mergetool_launch() {
                    let unresolved = state
                        .current
                        .lock()
                        .map(|g| g.as_ref().map(|d| d.unresolved_count()).unwrap_or(1))
                        .unwrap_or(1);
                    let saved = state.saved.load(Ordering::SeqCst);
                    exit_code_for(saved, unresolved)
                } else {
                    0
                };
                // `app.exit(code)` 在 Windows 上不一定把 code 透传成进程退出码
                // （run 循环正常结束会返回 Ok(()) → 退出码 0），直接进程退出：
                // mergetool 协议（trustExitCode）依赖真实退出码，无清理可做。
                api.prevent_close();
                std::process::exit(code);
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running MergeDrag");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::launch::LaunchArgs;
    use merge_core::Hunk;

    #[test]
    fn exit_code_follows_mergetool_protocol() {
        // trustExitCode：已保存且全部解决 → 0，其余情况 → 1。
        assert_eq!(exit_code_for(true, 0), 0);
        assert_eq!(exit_code_for(true, 1), 1);
        assert_eq!(exit_code_for(false, 0), 1);
        assert_eq!(exit_code_for(false, 2), 1);
    }

    #[test]
    fn persist_writes_text_and_sets_saved_flag() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("out.txt");
        let launch = LaunchArgs {
            merged: Some(target.clone()),
            ..Default::default()
        };
        let state = AppState::new(launch);
        state.persist("hello\n").unwrap();
        assert!(state.saved.load(Ordering::SeqCst), "写盘成功应标记 saved");
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "hello\n");
    }

    #[test]
    fn persist_defaults_to_launch_merged_when_target_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("merged.txt");
        let launch = LaunchArgs {
            merged: Some(target.clone()),
            ..Default::default()
        };
        let state = AppState::new(launch);
        // 手动清空 save_target，模拟打开第二个文件前的状态。
        *state.save_target.lock().unwrap() = None;
        state.persist("x\n").unwrap();
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "x\n");
    }

    #[test]
    fn apply_decision_persists_back_into_state() {
        // 回归：set_decision 只返回新文档不写回 state 的话，
        // save_merged 会按旧文档算未解决数（>0），全解决后也不 git add。
        let doc = merge_core::three_way("X\n", "Y\n", Some("B\n"), "t.rs");
        assert_eq!(doc.unresolved_count(), 1);
        let state = AppState::new(LaunchArgs::default());
        *state.current.lock().unwrap() = Some(doc);

        let id = {
            let binding = state.current.lock().unwrap();
            let doc = binding.as_ref().unwrap();
            let Hunk::Conflict { id, .. } = &doc.hunks[0] else {
                panic!("expected conflict");
            };
            *id
        };
        let decided = state
            .apply_decision(id, Decision::TakeRemote)
            .expect("decision should apply");
        assert_eq!(decided.unresolved_count(), 0);
        // 写回验证：直接从 state 读，不应是旧文档。
        assert_eq!(
            state
                .current
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .unresolved_count(),
            0,
            "决策必须写回 state.current"
        );
    }
}
