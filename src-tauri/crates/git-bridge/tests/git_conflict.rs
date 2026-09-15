//! End-to-end fixture: a genuinely conflicted git repo exercised through
//! `list_unmerged` (index stage parsing) and the merge engine, mimicking the
//! data path of a real `git mergetool` launch.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use git_bridge::list_unmerged;
use merge_core::{three_way, Decision, Hunk};

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

/// Read a blob from the index: `git show :<stage>:<path>`.  Blob 内容要保持
/// 原样（含结尾换行），所以不能 trim。
fn index_blob(dir: &Path, path: &str, stage: u32) -> String {
    let spec = format!(":{stage}:{path}");
    let out = git(dir, &["show", &spec]);
    assert!(out.status.success(), "git show {spec} failed");
    String::from_utf8(out.stdout).expect("测试文件应为 UTF-8")
}

/// Commit the current index with a fixed identity.
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

#[test]
fn real_conflict_end_to_end() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();

    run_ok(root, &["init"]);
    commit(root, "根提交");
    std::fs::write(root.join("data.txt"), "a\nb\nc\n").unwrap();
    run_ok(root, &["add", "data.txt"]);
    commit(root, "base");

    // 分支 A：改中间行 + 末尾追加
    run_ok(root, &["checkout", "-b", "side"]);
    std::fs::write(root.join("data.txt"), "a\nSIDE\nc\nSIDE-APPEND\n").unwrap();
    run_ok(root, &["add", "data.txt"]);
    commit(root, "side");

    // 主干：改同一行 + 末尾追加（不同内容）
    run_ok(root, &["checkout", "master"]);
    std::fs::write(root.join("data.txt"), "a\nMAIN\nc\nMAIN-APPEND\n").unwrap();
    run_ok(root, &["add", "data.txt"]);
    commit(root, "main");

    // 合并：中间行 + 末尾双方追加 = 经典 2 个冲突
    let merged = git(root, &["merge", "side"]);
    assert!(!merged.status.success(), "merge 应当产生冲突");

    // 1) git-bridge 从 index 里扫出未合并文件
    let files = list_unmerged(root).unwrap();
    assert_eq!(files.len(), 1, "应只有一个冲突文件");
    assert_eq!(files[0].path, PathBuf::from("data.txt"));
    assert!(!files[0].is_binary, "文本文件不应被标记为二进制");

    // 2) 取三个 stage 做三方合并
    let base = index_blob(root, "data.txt", 1);
    let local = index_blob(root, "data.txt", 2);
    let remote = index_blob(root, "data.txt", 3);
    assert_eq!(base, "a\nb\nc\n");

    let doc = three_way(&local, &remote, Some(&base), "data.txt");
    assert_eq!(
        doc.unresolved_count(),
        2,
        "中间行替换 + 末尾双方追加都是冲突"
    );
    assert_eq!(doc.hunks.len(), 4, "Clean + Conflict + Clean + Conflict");

    // 未解决时 apply() 输出必须保留双方内容（含标记）
    let raw = doc.apply();
    assert!(raw.contains("<<<<<<<") && raw.contains("SIDE") && raw.contains("MAIN"));

    // 3) 两个冲突都取双方 → 产出预期文本
    let resolved = doc
        .hunks
        .iter()
        .filter_map(|h| match h {
            Hunk::Conflict { id, .. } => Some(*id),
            _ => None,
        })
        .fold(doc.clone(), |d, id| {
            d.with_decision(id, Decision::TakeBoth { local_first: true })
        });
    assert_eq!(resolved.unresolved_count(), 0);
    assert_eq!(
        resolved.apply(),
        "a\nMAIN\nSIDE\nc\nMAIN-APPEND\nSIDE-APPEND\n"
    );

    // 4) 全部解决后 stage_file：文件脱离 unmerged 状态，进 staged。
    std::fs::write(root.join("data.txt"), resolved.apply()).unwrap();
    git_bridge::stage_file(root, Path::new("data.txt")).unwrap();
    let unmerged_after = git(root, &["ls-files", "-u"]);
    assert!(
        unmerged_after.stdout.is_empty(),
        "stage 后不应再有 unmerged 条目"
    );
    assert_eq!(run_ok(root, &["status", "--porcelain"]), "M  data.txt");
}
